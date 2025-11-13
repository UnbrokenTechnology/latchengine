use std::{mem::MaybeUninit, ops::Range, ptr};

pub struct Page<T> {
    buf: Box<[MaybeUninit<T>]>, // backing storage (len == capacity)
    len: usize, // number of initialized rows in [0..len)
}

impl<T> Page<T> {
    pub const DEFAULT_CAPACITY: usize = 4096;
    pub fn new() -> Self {
        Self::with_rows(Self::DEFAULT_CAPACITY)
    }
    pub fn with_rows(rows: usize) -> Self {
        let mut v: Vec<MaybeUninit<T>> = Vec::with_capacity(rows);
        unsafe { v.set_len(rows); }
        Self { buf: v.into_boxed_slice(), len: 0 }
    }
    #[inline] pub fn capacity(&self) -> usize { self.buf.len() }
    #[inline] pub fn len(&self) -> usize { self.len }
    #[inline] pub fn is_full(&self) -> bool { self.len == self.capacity() }

    #[inline] pub fn alloc_one(&mut self) -> Option<usize> {
        if self.len < self.capacity() { let i = self.len; self.len += 1; Some(i) } else { None }
    }
    #[inline] pub fn alloc_bulk(&mut self, n: usize) -> Option<Range<usize>> {
        let new_len = self.len.checked_add(n)?;
        if new_len <= self.capacity() { let s = self.len; self.len = new_len; Some(s..new_len) } else { None }
    }

    #[inline] pub fn write_at(&mut self, idx: usize, val: T) {
        debug_assert!(idx < self.len);
        unsafe { ptr::write(self.buf[idx].as_mut_ptr(), val); }
    }
    #[inline] pub fn slice(&self, r: Range<usize>) -> &[T] {
        debug_assert!(r.end <= self.len);
        unsafe { std::slice::from_raw_parts(self.buf.as_ptr().add(r.start) as *const T, r.len()) }
    }
    #[inline] pub fn slice_mut(&mut self, r: Range<usize>) -> &mut [T] {
        debug_assert!(r.end <= self.len);
        unsafe { std::slice::from_raw_parts_mut(self.buf.as_mut_ptr().add(r.start) as *mut T, r.len()) }
    }

    /// swap-remove one; returns Some((moved_from_last, moved_to_idx)) if a move happened
    pub fn free_one(&mut self, idx: usize) -> Option<(usize, usize)> {
        if idx >= self.len { return None; }
        let last = self.len - 1;
        unsafe {
            if idx != last {
                let moved: T = self.buf[last].assume_init_read();
                self.buf[idx].assume_init_drop();
                ptr::write(self.buf[idx].as_mut_ptr(), moved);
                self.len -= 1;
                Some((last, idx))
            } else {
                self.buf[last].assume_init_drop();
                self.len -= 1;
                None
            }
        }
    }

    pub fn free_bulk(
        &mut self,
        mut idxs: Vec<usize>,
        mut fix_index_local: impl FnMut(usize /*from_last*/, usize /*to_idx*/),
    ) {
        if idxs.is_empty() { return; }
        idxs.sort_unstable();
        let mut len = self.len;

        while let Some(mut r) = idxs.pop() {
            while r >= len {
                if let Some(x) = idxs.pop() { r = x; } else { break; }
            }
            if r >= len { break; }

            let last = len - 1;
            unsafe {
                if r != last {
                    let moved: T = self.buf[last].assume_init_read();
                    self.buf[r].assume_init_drop();
                    ptr::write(self.buf[r].as_mut_ptr(), moved);
                    fix_index_local(last, r);
                } else {
                    self.buf[last].assume_init_drop();
                }
            }
            len -= 1;
            while idxs.last().copied() == Some(len) { idxs.pop(); }
        }

        self.len = len;
    }
}
impl<T> Drop for Page<T> {
    fn drop(&mut self) {
        unsafe { for i in 0..self.len { self.buf[i].assume_init_drop(); } }
    }
}