use super::PoolError;
use std::{mem::MaybeUninit, ops::Range, ptr};

pub struct Page<T> {
    buf: Box<[MaybeUninit<T>]>,
    len: usize,
}

impl<T> Page<T> {
    pub fn with_capacity(rows: usize) -> Self {
        let mut vec: Vec<MaybeUninit<T>> = Vec::with_capacity(rows);
        unsafe {
            vec.set_len(rows);
        }
        Self {
            buf: vec.into_boxed_slice(),
            len: 0,
        }
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.buf.len()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        self.len == self.capacity()
    }

    #[inline]
    pub fn alloc_one(&mut self) -> Option<usize> {
        if self.len < self.capacity() {
            let idx = self.len;
            self.len += 1;
            Some(idx)
        } else {
            None
        }
    }

    #[inline]
    pub fn alloc_bulk(&mut self, n: usize) -> Option<Range<usize>> {
        let new_len = self.len.checked_add(n)?;
        if new_len <= self.capacity() {
            let start = self.len;
            self.len = new_len;
            Some(start..new_len)
        } else {
            None
        }
    }

    #[inline]
    pub fn write_at(&mut self, idx: usize, val: T) {
        debug_assert!(idx < self.len);
        unsafe {
            ptr::write(self.buf[idx].as_mut_ptr(), val);
        }
    }

    #[inline]
    pub fn get(&self, idx: usize) -> Result<&T, PoolError> {
        if idx >= self.len {
            return Err(PoolError::IndexOutOfBounds {
                index: idx,
                len: self.len,
            });
        }
        Ok(unsafe {
            // SAFETY: index bounds checked above and entries 0..len are initialized.
            &*self.buf[idx].as_ptr()
        })
    }

    #[inline]
    pub fn get_mut(&mut self, idx: usize) -> Result<&mut T, PoolError> {
        if idx >= self.len {
            return Err(PoolError::IndexOutOfBounds {
                index: idx,
                len: self.len,
            });
        }
        Ok(unsafe {
            // SAFETY: index bounds checked above and entries 0..len are initialized.
            &mut *self.buf[idx].as_mut_ptr()
        })
    }

    pub fn slice(&self, range: Range<usize>) -> Result<&[T], PoolError> {
        ensure_range(range.clone(), self.len)?;
        Ok(unsafe {
            std::slice::from_raw_parts(self.buf.as_ptr().add(range.start) as *const T, range.len())
        })
    }

    pub fn slice_mut(&mut self, range: Range<usize>) -> Result<&mut [T], PoolError> {
        ensure_range(range.clone(), self.len)?;
        Ok(unsafe {
            std::slice::from_raw_parts_mut(
                self.buf.as_mut_ptr().add(range.start) as *mut T,
                range.len(),
            )
        })
    }

    pub fn free_one(&mut self, idx: usize) -> Result<Option<(usize, usize)>, PoolError> {
        if idx >= self.len {
            return Err(PoolError::IndexOutOfBounds {
                index: idx,
                len: self.len,
            });
        }
        let last = self.len - 1;
        unsafe {
            if idx != last {
                let moved = self.buf[last].assume_init_read();
                self.buf[idx].assume_init_drop();
                ptr::write(self.buf[idx].as_mut_ptr(), moved);
                self.len -= 1;
                Ok(Some((last, idx)))
            } else {
                self.buf[last].assume_init_drop();
                self.len -= 1;
                Ok(None)
            }
        }
    }

    pub fn free_bulk(
        &mut self,
        mut idxs: Vec<usize>,
        mut fix_index_local: impl FnMut(usize, usize),
    ) -> Result<(), PoolError> {
        if idxs.is_empty() {
            return Ok(());
        }
        idxs.sort_unstable();
        let mut len = self.len;

        while let Some(mut target) = idxs.pop() {
            while target >= len {
                if let Some(next) = idxs.pop() {
                    target = next;
                } else {
                    break;
                }
            }
            if target >= len {
                break;
            }

            let last = len - 1;
            unsafe {
                if target != last {
                    let moved = self.buf[last].assume_init_read();
                    self.buf[target].assume_init_drop();
                    ptr::write(self.buf[target].as_mut_ptr(), moved);
                    fix_index_local(last, target);
                } else {
                    self.buf[last].assume_init_drop();
                }
            }
            len -= 1;

            while idxs.last().copied() == Some(len) {
                idxs.pop();
            }
        }

        self.len = len;
        Ok(())
    }
}

impl<T> Drop for Page<T> {
    fn drop(&mut self) {
        unsafe {
            for i in 0..self.len {
                self.buf[i].assume_init_drop();
            }
        }
    }
}

fn ensure_range(range: Range<usize>, len: usize) -> Result<(), PoolError> {
    if range.start > range.end || range.end > len {
        return Err(PoolError::RangeOutOfBounds {
            start: range.start,
            end: range.end,
            len,
        });
    }
    Ok(())
}
