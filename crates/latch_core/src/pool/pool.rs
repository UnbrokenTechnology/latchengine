use crate::pool::page::Page;
use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PoolError {
    RangeOutOfBounds {
        start: usize,
        end: usize,
        len: usize,
    },
    RangeCrossesPage {
        start: usize,
        end: usize,
        rows_per_page: usize,
    },
    PageMissing {
        page: usize,
    },
    IndexOutOfBounds {
        index: usize,
        len: usize,
    },
}

/// Public pool that hides individual pages and acts like a contiguous array.
pub struct PagedPool<T> {
    rows_per_page: usize,
    shift: u32,
    mask: usize,
    pages: Vec<Page<T>>,
}

impl<T> PagedPool<T> {
    pub fn with_rows_per_page(rows_per_page: usize) -> Self {
        assert!(rows_per_page.is_power_of_two() && rows_per_page > 0);
        Self {
            rows_per_page,
            shift: rows_per_page.trailing_zeros(),
            mask: rows_per_page - 1,
            pages: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        for page in &mut self.pages {
            page.clear();
        }
        if self.pages.len() > 1 {
            self.pages.truncate(1);
        }
    }

    #[inline]
    pub fn rows_per_page(&self) -> usize {
        self.rows_per_page
    }

    #[inline]
    fn page_of(&self, gidx: usize) -> usize {
        gidx >> self.shift
    }

    #[inline]
    fn local_of(&self, gidx: usize) -> usize {
        gidx & self.mask
    }

    #[inline]
    fn end_of_page(&self, start: usize) -> usize {
        (start | self.mask) + 1
    }

    fn localize_range(&self, range: Range<usize>) -> Result<(usize, Range<usize>), PoolError> {
        if range.start > range.end {
            return Err(PoolError::RangeOutOfBounds {
                start: range.start,
                end: range.end,
                len: self.len_total(),
            });
        }
        if range.end > self.len_total() {
            return Err(PoolError::RangeOutOfBounds {
                start: range.start,
                end: range.end,
                len: self.len_total(),
            });
        }
        if range.is_empty() {
            let page = self.page_of(range.start);
            let local = self.local_of(range.start);
            return Ok((page, local..local));
        }

        let p0 = self.page_of(range.start);
        let p1 = self.page_of(range.end - 1);
        if p0 != p1 {
            return Err(PoolError::RangeCrossesPage {
                start: range.start,
                end: range.end,
                rows_per_page: self.rows_per_page,
            });
        }

        let local_start = self.local_of(range.start);
        Ok((p0, local_start..local_start + range.len()))
    }

    pub fn len_total(&self) -> usize {
        if let Some(last) = self.pages.last() {
            (self.pages.len() - 1) * self.rows_per_page + last.len()
        } else {
            0
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len_total() == 0
    }

    fn ensure_page_with_space(&mut self) -> usize {
        if let Some((idx, _)) = self
            .pages
            .iter()
            .enumerate()
            .rev()
            .find(|(_, p)| !p.is_full())
        {
            idx
        } else {
            let idx = self.pages.len();
            self.pages.push(Page::with_capacity(self.rows_per_page));
            idx
        }
    }

    pub fn alloc_one(&mut self) -> usize {
        let pid = self.ensure_page_with_space();
        let local = self.pages[pid]
            .alloc_one()
            .expect("page should have capacity");
        (pid << self.shift) | local
    }

    pub fn alloc_bulk(&mut self, mut count: usize) -> Vec<Range<usize>> {
        let mut spans = Vec::new();
        while count > 0 {
            let pid = self.ensure_page_with_space();
            let page = &mut self.pages[pid];
            let available = page.capacity() - page.len();
            let take = available.min(count);
            let local = page
                .alloc_bulk(take)
                .expect("allocation within page must succeed");
            let start = (pid << self.shift) | local.start;
            spans.push(start..start + local.len());
            count -= take;
        }
        spans
    }

    #[inline]
    pub fn write_at(&mut self, gidx: usize, value: T) {
        let pid = self.page_of(gidx);
        let loc = self.local_of(gidx);
        self.pages[pid].write_at(loc, value);
    }

    pub fn get(&self, gidx: usize) -> Result<&T, PoolError> {
        if gidx >= self.len_total() {
            return Err(PoolError::IndexOutOfBounds {
                index: gidx,
                len: self.len_total(),
            });
        }
        let pid = self.page_of(gidx);
        let loc = self.local_of(gidx);
        self.pages
            .get(pid)
            .ok_or(PoolError::PageMissing { page: pid })?
            .get(loc)
    }

    pub fn get_mut(&mut self, gidx: usize) -> Result<&mut T, PoolError> {
        if gidx >= self.len_total() {
            return Err(PoolError::IndexOutOfBounds {
                index: gidx,
                len: self.len_total(),
            });
        }
        let pid = self.page_of(gidx);
        let loc = self.local_of(gidx);
        self.pages
            .get_mut(pid)
            .ok_or(PoolError::PageMissing { page: pid })?
            .get_mut(loc)
    }

    #[inline]
    pub fn clamp_to_page(
        &self,
        start: usize,
        nominal_len: usize,
        total_len: usize,
    ) -> Range<usize> {
        let page_end = self.end_of_page(start).min(total_len);
        let end = (start + nominal_len).min(page_end);
        start..end
    }

    pub fn slice_tile(&self, range: Range<usize>) -> Result<&[T], PoolError> {
        let (page, local) = self.localize_range(range)?;
        let page_ref = self
            .pages
            .get(page)
            .ok_or(PoolError::PageMissing { page })?;
        page_ref.slice(local)
    }

    pub fn slice_tile_mut(&mut self, range: Range<usize>) -> Result<&mut [T], PoolError> {
        let (page, local) = self.localize_range(range)?;
        let page_ref = self
            .pages
            .get_mut(page)
            .ok_or(PoolError::PageMissing { page })?;
        page_ref.slice_mut(local)
    }

    pub fn free_one_swap_remove(
        &mut self,
        gidx: usize,
        mut fix_index: impl FnMut(usize, usize),
    ) -> Result<(), PoolError> {
        let page_idx = self.page_of(gidx);
        let local = self.local_of(gidx);
        let page = self
            .pages
            .get_mut(page_idx)
            .ok_or(PoolError::PageMissing { page: page_idx })?;
        if let Some((from_local, to_local)) = page.free_one(local)? {
            let from_g = (page_idx << self.shift) | from_local;
            let to_g = (page_idx << self.shift) | to_local;
            fix_index(from_g, to_g);
        }
        Ok(())
    }

    pub fn free_bulk_swap_remove(
        &mut self,
        mut gidxs: Vec<usize>,
        mut fix_index: impl FnMut(usize, usize),
    ) -> Result<(), PoolError> {
        if gidxs.is_empty() {
            return Ok(());
        }
        gidxs.sort_unstable_by_key(|&g| (self.page_of(g), self.local_of(g)));
        let mut cursor = 0;
        while cursor < gidxs.len() {
            let page_idx = self.page_of(gidxs[cursor]);
            let start = cursor;
            while cursor < gidxs.len() && self.page_of(gidxs[cursor]) == page_idx {
                cursor += 1;
            }
            let locals: Vec<usize> = gidxs[start..cursor]
                .iter()
                .map(|&g| self.local_of(g))
                .collect();
            let page = self
                .pages
                .get_mut(page_idx)
                .ok_or(PoolError::PageMissing { page: page_idx })?;
            page.free_bulk(locals, |from_local, to_local| {
                let from_g = (page_idx << self.shift) | from_local;
                let to_g = (page_idx << self.shift) | to_local;
                fix_index(from_g, to_g);
            })?;
        }

        while self.pages.last().map_or(false, |p| p.len() == 0) {
            self.pages.pop();
        }
        Ok(())
    }
}
