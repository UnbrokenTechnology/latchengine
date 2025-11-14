use crate::pool::{PagedPool, PoolError};
use std::ops::Range;

/// Double-buffered, page-aware column for a single component type.
///
/// Storage never relocates elements once written; growth allocates
/// additional pages in lockstep across both buffers. Reading always
/// happens from the `cur` pool, writing from `nxt`, and `swap_buffers`
/// flips their roles at the end of the tick.
pub struct Column<T> {
    cur: PagedPool<T>,
    nxt: PagedPool<T>,
}

impl<T> Column<T> {
    /// Create a column whose pages each contain `rows_per_page` rows.
    /// `rows_per_page` must be a non-zero power of two.
    pub fn with_rows_per_page(rows_per_page: usize) -> Self {
        debug_assert!(rows_per_page.is_power_of_two() && rows_per_page > 0);
        Self {
            cur: PagedPool::with_rows_per_page(rows_per_page),
            nxt: PagedPool::with_rows_per_page(rows_per_page),
        }
    }

    #[inline]
    pub fn rows_per_page(&self) -> usize {
        self.cur.rows_per_page()
    }

    #[inline]
    pub fn len_total(&self) -> usize {
        self.cur.len_total()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.cur.is_empty()
    }

    /// Clamp the requested tile length to the end of the backing page.
    #[inline]
    pub fn clamp_to_page(&self, start: usize, nominal_len: usize) -> Range<usize> {
        self.cur.clamp_to_page(start, nominal_len, self.len_total())
    }

    /// Allocate a single row, returning its global index.
    pub fn alloc_one(&mut self) -> usize {
        let gidx = self.cur.alloc_one();
        let mirror = self.nxt.alloc_one();
        debug_assert_eq!(gidx, mirror, "paged column buffers diverged");
        gidx
    }

    /// Allocate `count` rows, returning disjoint spans that never cross pages.
    pub fn alloc_bulk(&mut self, count: usize) -> Vec<Range<usize>> {
        let spans = self.cur.alloc_bulk(count);
        let mirror = self.nxt.alloc_bulk(count);
        debug_assert_eq!(spans.len(), mirror.len(), "paged column spans mismatch");
        debug_assert!(spans.iter().zip(mirror.iter()).all(|(a, b)| a == b));
        spans
    }

    /// Initialize the read buffer for an allocated row.
    #[inline]
    pub fn init_cur_at(&mut self, gidx: usize, value: T) {
        self.cur.write_at(gidx, value);
    }

    /// Initialize the write buffer for an allocated row.
    #[inline]
    pub fn init_next_at(&mut self, gidx: usize, value: T) {
        self.nxt.write_at(gidx, value);
    }

    /// Initialize both buffers for a freshly allocated row.
    #[inline]
    pub fn init_both_at(&mut self, gidx: usize, cur_value: T, next_value: T) {
        self.cur.write_at(gidx, cur_value);
        self.nxt.write_at(gidx, next_value);
    }

    /// Borrow a read-only slice from the current buffer.
    #[inline]
    pub fn slice_read(&self, range: Range<usize>) -> Result<&[T], PoolError> {
        self.cur.slice_tile(range)
    }

    /// Borrow a mutable slice from the next buffer.
    #[inline]
    pub fn slice_write(&mut self, range: Range<usize>) -> Result<&mut [T], PoolError> {
        self.nxt.slice_tile_mut(range)
    }

    /// Borrow matching tiles from the current and next buffers.
    pub fn slice_rw(&mut self, range: Range<usize>) -> Result<(&[T], &mut [T]), PoolError> {
        let read = self.cur.slice_tile(range.clone())?;
        let write = self.nxt.slice_tile_mut(range)?;
        Ok((read, write))
    }

    /// Swap current/next pools at the end of a tick.
    #[inline]
    pub fn swap_buffers(&mut self) {
        std::mem::swap(&mut self.cur, &mut self.nxt);
    }

    /// Remove a single row via swap-remove, invoking `fix_index` if an element moved.
    pub fn free_one_swap_remove(
        &mut self,
        gidx: usize,
        mut fix_index: impl FnMut(usize, usize),
    ) -> Result<(), PoolError> {
        let mut moved = None;
        self.cur
            .free_one_swap_remove(gidx, |from, to| moved = Some((from, to)))?;
        self.nxt
            .free_one_swap_remove(gidx, |_from, _to| {})?;
        if let Some((from, to)) = moved {
            fix_index(from, to);
        }
        Ok(())
    }

    /// Remove multiple rows deterministically via swap-remove.
    pub fn free_bulk_swap_remove(
        &mut self,
        gidxs: Vec<usize>,
        mut fix_index: impl FnMut(usize, usize),
    ) -> Result<(), PoolError> {
        if gidxs.is_empty() {
            return Ok(());
        }

        let mut moves = Vec::new();
        self.cur
            .free_bulk_swap_remove(gidxs.clone(), |from, to| moves.push((from, to)))?;
        self.nxt
            .free_bulk_swap_remove(gidxs, |_from, _to| {})?;
        for (from, to) in moves {
            fix_index(from, to);
        }
        Ok(())
    }
}