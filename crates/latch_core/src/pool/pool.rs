use std::ops::Range;
use crate::pool::page::Page;

/// Public pool that hides pages; treats storage as one logical array.
/// * rows_per_page must be a power of two.
/// * All tile ranges must remain within a single page; we assert this.
pub struct PagedPool<T> {
    rows_per_page: usize,  // power-of-two
    k: u32,                // log2(rows_per_page)
    pages: Vec<Page<T>>,
}

impl<T> PagedPool<T> {
    pub fn with_rows_per_page(rows_per_page: usize) -> Self {
        assert!(rows_per_page.is_power_of_two() && rows_per_page > 0);
        Self {
            rows_per_page,
            k: rows_per_page.trailing_zeros(),
            pages: Vec::new(),
        }
    }

    #[inline] pub fn rows_per_page(&self) -> usize { self.rows_per_page }

    // ---------- internal mapping helpers ----------
    #[inline] fn page_of(&self, g: usize) -> u32 { (g >> self.k) as u32 }
    #[inline] fn local_of(&self, g: usize) -> usize { g & (self.rows_per_page - 1) }
    #[inline] fn end_of_page(&self, start: usize) -> usize {
        (start | (self.rows_per_page - 1)) + 1
    }

    /// Map a global range to (page, local range). Panics if it crosses a page boundary.
    #[inline]
    fn localize_range(&self, r: Range<usize>) -> (u32, Range<usize>) {
        let p0 = self.page_of(r.start);
        let p1 = self.page_of(r.end.saturating_sub(1));
        assert!(p0 == p1, "tile range crosses page boundary");
        let s = self.local_of(r.start);
        (p0, s..(s + r.len()))
    }

    // ---------- logical length ----------
    /// Total logical length across all pages.
    pub fn len_total(&self) -> usize {
        if self.pages.is_empty() { return 0; }
        let full_rows = (self.pages.len() - 1) * self.rows_per_page;
        full_rows + self.pages.last().unwrap().len()
    }
    pub fn is_empty(&self) -> bool { self.len_total() == 0 }

    // ---------- allocation ----------
    fn ensure_page_with_space(&mut self) -> u32 {
        if let Some((pid, _)) = self.pages.iter().enumerate().rev().find(|(_, p)| !p.is_full()) {
            return pid as u32;
        }
        let pid = self.pages.len() as u32;
        self.pages.push(Page::with_rows(self.rows_per_page));
        pid
    }

    /// Allocate one row; returns its **global index**.
    pub fn alloc_one(&mut self) -> usize {
        let pid = self.ensure_page_with_space();
        let page = &mut self.pages[pid as usize];
        let local = page.alloc_one().expect("page reported space");
        ((pid as usize) << self.k) | local
    }

    /// Allocate `n` rows; returns **global spans**. Spans never cross pages.
    pub fn alloc_bulk(&mut self, mut n: usize) -> Vec<Range<usize>> {
        let mut spans = Vec::new();
        while n > 0 {
            let pid = self.ensure_page_with_space();
            let page = &mut self.pages[pid as usize];
            let avail = page.capacity() - page.len();
            let take = avail.min(n);
            let local_r = page.alloc_bulk(take).expect("must fit");
            let start_g = ((pid as usize) << self.k) | local_r.start;
            spans.push(start_g..(start_g + local_r.len()));
            n -= take;
        }
        spans
    }

    /// Construct a value at an allocated global index.
    #[inline] pub fn write_at(&mut self, gidx: usize, val: T) {
        let pid = self.page_of(gidx);
        let loc = self.local_of(gidx);
        self.pages[pid as usize].write_at(loc, val);
    }

    // ---------- slicing (tile = single page range) ----------
    /// Clamp a nominal tile to end-of-page (optional helper).
    #[inline]
    pub fn clamp_to_page(&self, start: usize, nominal_len: usize, total_len: usize) -> Range<usize> {
        let page_end = self.end_of_page(start).min(total_len);
        let end = (start + nominal_len).min(page_end);
        start..end
    }

    /// Borrow a **page-local** tile as a read slice (global range input).
    pub fn slice_tile(&self, r: Range<usize>) -> &[T] {
        let (p, lr) = self.localize_range(r);
        self.pages[p as usize].slice(lr)
    }

    /// Borrow a **page-local** tile as a write slice (global range input).
    pub fn slice_tile_mut(&mut self, r: Range<usize>) -> &mut [T] {
        let (p, lr) = self.localize_range(r);
        self.pages[p as usize].slice_mut(lr)
    }

    // ---------- free / swap-remove ----------
    /// Free one row by swap-remove (in-page). Calls `fix_index(from_global, to_global)` if a move occurred.
    pub fn free_one_swap_remove(
        &mut self,
        gidx: usize,
        mut fix_index: impl FnMut(usize, usize),
    ) {
        let pid = self.page_of(gidx);
        let loc = self.local_of(gidx);
        if let Some((from_local, to_local)) = self.pages[pid as usize].free_one(loc) {
            let from_g = ((pid as usize) << self.k) | from_local;
            let to_g   = ((pid as usize) << self.k) | to_local;
            fix_index(from_g, to_g);
        }
    }

    /// Deterministic batched swap-remove. `gidxs` may be unsorted / cross pages.
    pub fn free_bulk_swap_remove(
        &mut self,
        mut gidxs: Vec<usize>,
        mut fix_index: impl FnMut(usize /*from_global*/, usize /*to_global*/),
    ) {
        if gidxs.is_empty() { return; }
        // Sort by (page, local) so we can process one page at a time deterministically
        gidxs.sort_unstable_by_key(|&g| (self.page_of(g), self.local_of(g)));
        let mut i = 0;
        while i < gidxs.len() {
            let pid = self.page_of(gidxs[i]);
            let start = i;
            while i < gidxs.len() && self.page_of(gidxs[i]) == pid { i += 1; }
            // Collect locals for this page
            let locals: Vec<usize> = gidxs[start..i].iter().map(|&g| self.local_of(g)).collect();

            self.pages[pid as usize].free_bulk(
                locals,
                |from_local, to_local| {
                    let from_g = ((pid as usize) << self.k) | from_local;
                    let to_g   = ((pid as usize) << self.k) | to_local;
                    fix_index(from_g, to_g);
                },
            );
        }

        // Optionally reclaim fully empty tail pages (keep some reserve if you like)
        while self.pages.last().map_or(false, |p| p.len() == 0) {
            self.pages.pop();
        }
    }
}