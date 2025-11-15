use crate::{
    ecs::{meta_of, ArchetypeLayout, Component, ComponentId, ComponentMeta, EntityId},
    pool::{PagedPool, PoolError},
};
use latch_env::memory::Memory;
use std::{
    alloc::{alloc, dealloc, handle_alloc_error, Layout},
    collections::HashMap,
    mem,
    num::NonZeroUsize,
    ops::Range,
    ptr::{self, NonNull},
    slice,
    sync::Arc,
};
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct ColumnPlan {
    pub component_id: ComponentId,
    pub meta: ComponentMeta,
}

#[derive(Debug, Clone)]
pub struct ArchetypePlan {
    pub layout: ArchetypeLayout,
    pub bytes_per_row: NonZeroUsize,
    pub rows_per_page: NonZeroUsize,
    pub page_bytes: NonZeroUsize,
    pub columns: Vec<ColumnPlan>,
}

#[derive(Debug, Clone, Copy)]
pub struct PageBudget {
    pub l2_bytes: NonZeroUsize,
}

impl PageBudget {
    #[inline]
    pub fn detect() -> Self {
        let l2 = Memory::detect().l2;
        let l2_bytes = NonZeroUsize::new(l2).expect("L2 cache size must be non-zero");
        Self { l2_bytes }
    }

    #[inline]
    pub fn with_l2_bytes(bytes: NonZeroUsize) -> Self {
        Self { l2_bytes: bytes }
    }
}

impl Default for PageBudget {
    fn default() -> Self {
        Self::detect()
    }
}

pub fn plan_archetype(
    layout: ArchetypeLayout,
    budget: PageBudget,
) -> Result<ArchetypePlan, PlanError> {
    let mut columns = Vec::with_capacity(layout.components().len());
    let mut bytes_per_row = std::mem::size_of::<EntityId>();

    for &component_id in layout.components() {
        let meta =
            meta_of(component_id).ok_or(PlanError::ComponentNotRegistered { component_id })?;
        bytes_per_row = bytes_per_row
            .checked_add(meta.stride)
            .ok_or(PlanError::BytesPerRowOverflow)?;
        columns.push(ColumnPlan { component_id, meta });
    }

    let bytes_per_row = NonZeroUsize::new(bytes_per_row).ok_or(PlanError::BytesPerRowZero)?;
    let rows_per_page = compute_rows_per_page(bytes_per_row, budget.l2_bytes);
    let rows_per_page =
        NonZeroUsize::new(rows_per_page).ok_or(PlanError::RowsPerPageZero { bytes_per_row })?;
    let page_bytes = rows_per_page
        .get()
        .checked_mul(bytes_per_row.get())
        .and_then(NonZeroUsize::new)
        .ok_or(PlanError::PageBytesOverflow {
            rows_per_page,
            bytes_per_row,
        })?;

    Ok(ArchetypePlan {
        layout,
        bytes_per_row,
        rows_per_page,
        page_bytes,
        columns,
    })
}

struct BytePage {
    ptr: NonNull<u8>,
    len: usize,
    capacity_rows: usize,
    stride: usize,
    align: usize,
    alloc_size: usize,
}

impl BytePage {
    fn with_capacity(rows: usize, stride: usize, align: usize) -> Self {
        debug_assert!(
            align.is_power_of_two(),
            "page alignment must be power-of-two"
        );
        let total = rows
            .checked_mul(stride)
            .expect("byte page allocation overflow");
        let alloc_size = total.max(align);
        let layout = Layout::from_size_align(alloc_size, align).expect("invalid layout");
        let ptr = unsafe { alloc(layout) };
        let ptr = NonNull::new(ptr).unwrap_or_else(|| handle_alloc_error(layout));
        Self {
            ptr,
            len: 0,
            capacity_rows: rows,
            stride,
            align,
            alloc_size,
        }
    }

    #[inline]
    fn len(&self) -> usize {
        self.len
    }

    #[inline]
    fn is_full(&self) -> bool {
        self.len == self.capacity_rows
    }

    fn alloc_one(&mut self) -> usize {
        assert!(self.len < self.capacity_rows);
        let idx = self.len;
        self.len += 1;
        idx
    }

    fn extend(&mut self, count: usize) -> Range<usize> {
        assert!(self.len + count <= self.capacity_rows);
        let start = self.len;
        self.len += count;
        start..self.len
    }

    fn slice_bytes(&self, row_start: usize, rows: usize) -> &[u8] {
        let offset = row_start * self.stride;
        let len_bytes = rows * self.stride;
        unsafe {
            // SAFETY: callers validate range, so the slice stays within the allocated buffer.
            slice::from_raw_parts(self.ptr.as_ptr().add(offset), len_bytes)
        }
    }

    fn slice_bytes_mut(&mut self, row_start: usize, rows: usize) -> &mut [u8] {
        let offset = row_start * self.stride;
        let len_bytes = rows * self.stride;
        unsafe {
            // SAFETY: callers validate range, giving exclusive access within the buffer.
            slice::from_raw_parts_mut(self.ptr.as_ptr().add(offset), len_bytes)
        }
    }

    fn row_bytes(&self, row: usize) -> &[u8] {
        self.slice_bytes(row, 1)
    }

    fn write_row(&mut self, row: usize, bytes: &[u8]) {
        debug_assert_eq!(bytes.len(), self.stride);
        let dst = self.slice_bytes_mut(row, 1);
        unsafe {
            // SAFETY: `dst` has length `stride`, matching the source buffer.
            ptr::copy_nonoverlapping(bytes.as_ptr(), dst.as_mut_ptr(), self.stride);
        }
    }

    fn pop_one(&mut self) {
        if self.len > 0 {
            self.len -= 1;
        }
    }
}

impl Drop for BytePage {
    fn drop(&mut self) {
        let layout = Layout::from_size_align(self.alloc_size, self.align).expect("invalid layout");
        unsafe {
            dealloc(self.ptr.as_ptr(), layout);
        }
    }
}

fn compute_rows_per_page(bytes_per_row: NonZeroUsize, budget_bytes: NonZeroUsize) -> usize {
    let max_rows = budget_bytes.get() / bytes_per_row.get();
    let max_rows = max_rows.max(1);
    floor_power_of_two(max_rows)
}

fn floor_power_of_two(value: usize) -> usize {
    debug_assert!(value > 0);
    if value == 0 {
        return 0;
    }
    let highest_bit = usize::BITS - value.leading_zeros() - 1;
    1usize << highest_bit
}

#[derive(Debug, Error)]
pub enum PlanError {
    #[error("component id {component_id} not registered")]
    ComponentNotRegistered { component_id: ComponentId },
    #[error("bytes per row evaluated to zero")]
    BytesPerRowZero,
    #[error("bytes per row overflowed usize::MAX")]
    BytesPerRowOverflow,
    #[error("rows per page computed as zero (bytes_per_row={bytes_per_row})")]
    RowsPerPageZero { bytes_per_row: NonZeroUsize },
    #[error(
        "page byte budget overflow (rows_per_page={rows_per_page}, bytes_per_row={bytes_per_row})"
    )]
    PageBytesOverflow {
        rows_per_page: NonZeroUsize,
        bytes_per_row: NonZeroUsize,
    },
}

#[derive(Debug, Error)]
pub enum ColumnError {
    #[error("range [{start}, {end}) is out of bounds for len {len}")]
    RangeOutOfBounds {
        start: usize,
        end: usize,
        len: usize,
    },
    #[error("range [{start}, {end}) crosses page boundary (rows_per_page={rows_per_page})")]
    RangeCrossesPage {
        start: usize,
        end: usize,
        rows_per_page: usize,
    },
    #[error("index {index} out of bounds for len {len}")]
    IndexOutOfBounds { index: usize, len: usize },
    #[error("stride mismatch: expected {expected} bytes, got {got} bytes")]
    StrideMismatch { expected: usize, got: usize },
    #[error("type mismatch for component: expected stride {expected_stride} bytes (align {expected_align}), but got stride {actual_stride} (align {actual_align})")]
    TypeMismatch {
        expected_stride: usize,
        expected_align: usize,
        actual_stride: usize,
        actual_align: usize,
    },
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("component id {component_id} not present in archetype")]
    ColumnMissing { component_id: ComponentId },
    #[error("duplicate column request for component id {component_id}")]
    DuplicateColumnRequest { component_id: ComponentId },
    #[error(transparent)]
    Column(#[from] ColumnError),
    #[error("entity pool error: {0:?}")]
    EntityPool(PoolError),
    #[error("index {index} out of bounds for len {len}")]
    IndexOutOfBounds { index: usize, len: usize },
    #[error("not enough entity ids provided (expected {expected}, got {got})")]
    NotEnoughEntities { expected: usize, got: usize },
}

pub struct ComponentColumn {
    plan: ColumnPlan,
    rows_per_page: usize,
    stride: usize,
    align: usize,
    shift: u32,
    mask: usize,
    cur_pages: Vec<BytePage>,
    nxt_pages: Vec<BytePage>,
    len: usize,
}

impl ComponentColumn {
    pub fn new(plan: ColumnPlan, rows_per_page: usize) -> Self {
        debug_assert!(rows_per_page.is_power_of_two());
        let stride = plan.meta.stride;
        let align = plan.meta.align;
        let shift = rows_per_page.trailing_zeros();
        let mask = rows_per_page - 1;
        Self {
            plan,
            rows_per_page,
            stride,
            align,
            shift,
            mask,
            cur_pages: Vec::new(),
            nxt_pages: Vec::new(),
            len: 0,
        }
    }

    #[inline]
    pub fn plan(&self) -> &ColumnPlan {
        &self.plan
    }

    #[inline]
    pub fn rows_per_page(&self) -> usize {
        self.rows_per_page
    }

    #[inline]
    pub fn stride(&self) -> usize {
        self.stride
    }

    #[inline]
    pub fn align(&self) -> usize {
        self.align
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn page_count(&self) -> usize {
        self.cur_pages.len()
    }

    pub fn page_range(&self, page_idx: usize) -> Range<usize> {
        let page = self
            .cur_pages
            .get(page_idx)
            .expect("page index out of bounds");
        let start = page_idx << self.shift;
        let end = start + page.len();
        debug_assert!(end <= self.len, "page range exceeds column length");
        start..end
    }

    pub fn alloc_one(&mut self) -> usize {
        let page_idx = self.ensure_page_with_space();
        let local = self.cur_pages[page_idx].alloc_one();
        self.nxt_pages[page_idx].alloc_one();
        let gidx = (page_idx << self.shift) | local;
        self.len += 1;
        gidx
    }

    pub fn alloc_bulk(&mut self, mut count: usize) -> Vec<Range<usize>> {
        let mut spans = Vec::new();
        while count > 0 {
            let page_idx = self.ensure_page_with_space();
            let available = self.rows_per_page - self.cur_pages[page_idx].len();
            let take = available.min(count);
            let range_local = self.cur_pages[page_idx].extend(take);
            self.nxt_pages[page_idx].extend(take);
            let start = (page_idx << self.shift) | range_local.start;
            let end = start + take;
            spans.push(start..end);
            self.len += take;
            count -= take;
        }
        spans
    }

    pub fn write_cur_at(&mut self, gidx: usize, bytes: &[u8]) -> Result<(), ColumnError> {
        self.validate_stride(bytes.len())?;
        let (page_idx, local_idx) = self.global_to_local(gidx)?;
        self.cur_pages[page_idx].write_row(local_idx, bytes);
        Ok(())
    }

    pub fn write_next_at(&mut self, gidx: usize, bytes: &[u8]) -> Result<(), ColumnError> {
        self.validate_stride(bytes.len())?;
        let (page_idx, local_idx) = self.global_to_local(gidx)?;
        self.nxt_pages[page_idx].write_row(local_idx, bytes);
        Ok(())
    }

    pub fn copy_cur_to_next(&mut self, gidx: usize) -> Result<(), ColumnError> {
        let (page_idx, local_idx) = self.global_to_local(gidx)?;
        let cur = self.cur_pages[page_idx].row_bytes(local_idx);
        self.nxt_pages[page_idx].write_row(local_idx, cur);
        Ok(())
    }

    pub fn slice_read(&self, range: Range<usize>) -> Result<&[u8], ColumnError> {
        let (page_idx, local) = self.localize_range(range)?;
        Ok(self.cur_pages[page_idx].slice_bytes(local.start, local.len()))
    }

    pub fn slice_write(&mut self, range: Range<usize>) -> Result<&mut [u8], ColumnError> {
        let (page_idx, local) = self.localize_range(range)?;
        Ok(self.nxt_pages[page_idx].slice_bytes_mut(local.start, local.len()))
    }

    pub fn slice_rw(&mut self, range: Range<usize>) -> Result<(&[u8], &mut [u8]), ColumnError> {
        let (page_idx, local) = self.localize_range(range)?;
        let read = self.cur_pages[page_idx].slice_bytes(local.start, local.len());
        let write = self.nxt_pages[page_idx].slice_bytes_mut(local.start, local.len());
        Ok((read, write))
    }

    pub fn slice_read_typed<T>(&self, range: Range<usize>) -> Result<&[T], ColumnError> {
        self.validate_typed::<T>()?;
        let (page_idx, local) = self.localize_range(range)?;
        let bytes = self.cur_pages[page_idx].slice_bytes(local.start, local.len());
        Ok(Self::cast_bytes::<T>(bytes, local.len()))
    }

    pub fn slice_write_typed<T>(&mut self, range: Range<usize>) -> Result<&mut [T], ColumnError> {
        self.validate_typed::<T>()?;
        let (page_idx, local) = self.localize_range(range)?;
        let bytes = self.nxt_pages[page_idx].slice_bytes_mut(local.start, local.len());
        Ok(Self::cast_bytes_mut::<T>(bytes, local.len()))
    }

    pub fn slice_rw_typed<T>(
        &mut self,
        range: Range<usize>,
    ) -> Result<(&[T], &mut [T]), ColumnError> {
        self.validate_typed::<T>()?;
        let (page_idx, local) = self.localize_range(range)?;
        let read = self.cur_pages[page_idx].slice_bytes(local.start, local.len());
        let write = self.nxt_pages[page_idx].slice_bytes_mut(local.start, local.len());
        Ok((
            Self::cast_bytes::<T>(read, local.len()),
            Self::cast_bytes_mut::<T>(write, local.len()),
        ))
    }

    pub fn column_slice_read<T>(&self) -> Result<&[T], ColumnError> {
        self.slice_read_typed::<T>(0..self.len)
    }

    pub fn column_slice_write<T>(&mut self) -> Result<&mut [T], ColumnError> {
        self.slice_write_typed::<T>(0..self.len)
    }

    pub fn swap_buffers(&mut self) {
        std::mem::swap(&mut self.cur_pages, &mut self.nxt_pages);
    }

    pub fn free_one_swap_remove(
        &mut self,
        gidx: usize,
    ) -> Result<Option<(usize, usize)>, ColumnError> {
        if gidx >= self.len {
            return Err(ColumnError::IndexOutOfBounds {
                index: gidx,
                len: self.len,
            });
        }
        let last_idx = self.len - 1;
        let moved = if gidx != last_idx {
            self.move_row(last_idx, gidx)?;
            Some((last_idx, gidx))
        } else {
            None
        };
        self.pop_last();
        self.len -= 1;
        self.trim_trailing_pages();
        Ok(moved)
    }

    pub fn free_bulk_swap_remove(
        &mut self,
        mut gidxs: Vec<usize>,
    ) -> Result<Vec<(usize, usize)>, ColumnError> {
        if gidxs.is_empty() {
            return Ok(Vec::new());
        }
        gidxs.sort_unstable();
        gidxs.dedup();
        let mut moves = Vec::new();
        for &gidx in gidxs.iter().rev() {
            if gidx >= self.len {
                return Err(ColumnError::IndexOutOfBounds {
                    index: gidx,
                    len: self.len,
                });
            }
            let last_idx = self.len - 1;
            if gidx != last_idx {
                self.move_row(last_idx, gidx)?;
                moves.push((last_idx, gidx));
            }
            self.pop_last();
            self.len -= 1;
        }
        self.trim_trailing_pages();
        moves.reverse();
        Ok(moves)
    }

    #[inline]
    pub fn clamp_to_page(&self, start: usize, nominal_len: usize) -> Range<usize> {
        let page_end = self.end_of_page(start).min(self.len);
        let end = (start + nominal_len).min(page_end);
        start..end
    }

    fn ensure_page_with_space(&mut self) -> usize {
        if self
            .cur_pages
            .last()
            .map(|page| page.is_full())
            .unwrap_or(true)
        {
            self.cur_pages.push(BytePage::with_capacity(
                self.rows_per_page,
                self.stride,
                self.align,
            ));
            self.nxt_pages.push(BytePage::with_capacity(
                self.rows_per_page,
                self.stride,
                self.align,
            ));
        }
        self.cur_pages.len() - 1
    }

    fn validate_typed<T>(&self) -> Result<(), ColumnError> {
        let expected_stride = self.stride;
        let expected_align = self.align;
        let actual_stride = mem::size_of::<T>();
        let actual_align = mem::align_of::<T>();
        if actual_stride != expected_stride || actual_align != expected_align {
            return Err(ColumnError::TypeMismatch {
                expected_stride,
                expected_align,
                actual_stride,
                actual_align,
            });
        }
        Ok(())
    }

    fn cast_bytes<T>(bytes: &[u8], rows: usize) -> &[T] {
        debug_assert_eq!(bytes.len(), rows * mem::size_of::<T>());
        unsafe {
            // SAFETY: caller validated the stride and alignment match `T` and there are `rows`
            // consecutive elements in the byte slice.
            slice::from_raw_parts(bytes.as_ptr() as *const T, rows)
        }
    }

    fn cast_bytes_mut<T>(bytes: &mut [u8], rows: usize) -> &mut [T] {
        debug_assert_eq!(bytes.len(), rows * mem::size_of::<T>());
        unsafe {
            // SAFETY: caller validated the stride and alignment match `T` and there are `rows`
            // consecutive elements in the byte slice, giving exclusive access through `bytes`.
            slice::from_raw_parts_mut(bytes.as_mut_ptr() as *mut T, rows)
        }
    }

    fn move_row(&mut self, from: usize, to: usize) -> Result<(), ColumnError> {
        let (from_page, from_local) = self.global_to_local(from)?;
        let (to_page, to_local) = self.global_to_local(to)?;
        if from_page == to_page && from_local == to_local {
            return Ok(());
        }

        let mut cur_tmp = vec![0u8; self.stride];
        {
            let src = self.cur_pages[from_page].row_bytes(from_local);
            cur_tmp.copy_from_slice(src);
        }
        let mut nxt_tmp = vec![0u8; self.stride];
        {
            let src = self.nxt_pages[from_page].row_bytes(from_local);
            nxt_tmp.copy_from_slice(src);
        }
        self.cur_pages[to_page].write_row(to_local, &cur_tmp);
        self.nxt_pages[to_page].write_row(to_local, &nxt_tmp);
        Ok(())
    }

    fn pop_last(&mut self) {
        if self.len == 0 {
            return;
        }
        let last_idx = self.len - 1;
        let (page_idx, _) = self.global_to_local(last_idx).expect("len guards index");
        self.cur_pages[page_idx].pop_one();
        self.nxt_pages[page_idx].pop_one();
    }

    fn trim_trailing_pages(&mut self) {
        while self
            .cur_pages
            .last()
            .map(|page| page.len() == 0)
            .unwrap_or(false)
        {
            self.cur_pages.pop();
            self.nxt_pages.pop();
        }
    }

    fn global_to_local(&self, gidx: usize) -> Result<(usize, usize), ColumnError> {
        if gidx >= self.len {
            return Err(ColumnError::IndexOutOfBounds {
                index: gidx,
                len: self.len,
            });
        }
        let page = gidx >> self.shift;
        let local = gidx & self.mask;
        Ok((page, local))
    }

    fn localize_range(&self, range: Range<usize>) -> Result<(usize, Range<usize>), ColumnError> {
        if range.start > range.end || range.end > self.len {
            return Err(ColumnError::RangeOutOfBounds {
                start: range.start,
                end: range.end,
                len: self.len,
            });
        }
        if range.is_empty() {
            let page = range.start >> self.shift;
            let local = range.start & self.mask;
            return Ok((page, local..local));
        }
        let p0 = range.start >> self.shift;
        let p1 = (range.end - 1) >> self.shift;
        if p0 != p1 {
            return Err(ColumnError::RangeCrossesPage {
                start: range.start,
                end: range.end,
                rows_per_page: self.rows_per_page,
            });
        }
        let local_start = range.start & self.mask;
        Ok((p0, local_start..local_start + range.len()))
    }

    fn validate_stride(&self, got: usize) -> Result<(), ColumnError> {
        if got != self.stride {
            return Err(ColumnError::StrideMismatch {
                expected: self.stride,
                got,
            });
        }
        Ok(())
    }

    #[inline]
    fn end_of_page(&self, start: usize) -> usize {
        (start | self.mask) + 1
    }
}

pub struct ArchetypeStorage {
    plan: Arc<ArchetypePlan>,
    entity_ids: PagedPool<EntityId>,
    columns: Vec<ComponentColumn>,
    index_by_component: HashMap<ComponentId, usize>,
    len: usize,
}

impl ArchetypeStorage {
    pub fn from_plan(plan: ArchetypePlan) -> Self {
        let rows_per_page = plan.rows_per_page.get();
        let columns: Vec<ComponentColumn> = plan
            .columns
            .iter()
            .cloned()
            .map(|col_plan| ComponentColumn::new(col_plan, rows_per_page))
            .collect();
        let index_by_component = columns
            .iter()
            .enumerate()
            .map(|(idx, col)| (col.plan.component_id, idx))
            .collect();

        Self {
            entity_ids: PagedPool::with_rows_per_page(rows_per_page),
            plan: Arc::new(plan),
            columns,
            index_by_component,
            len: 0,
        }
    }

    #[inline]
    pub fn plan(&self) -> &ArchetypePlan {
        self.plan.as_ref()
    }

    #[inline]
    pub fn entity_count(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn columns(&self) -> &[ComponentColumn] {
        &self.columns
    }

    #[inline]
    pub fn rows_per_page(&self) -> usize {
        self.plan.rows_per_page.get()
    }

    pub fn entity_id_at(&self, gidx: usize) -> Result<EntityId, StorageError> {
        self.entity_ids
            .get(gidx)
            .map(|id| *id)
            .map_err(StorageError::EntityPool)
    }

    pub fn entity_ids_slice(&self, range: Range<usize>) -> Result<&[EntityId], StorageError> {
        self.entity_ids
            .slice_tile(range)
            .map_err(StorageError::EntityPool)
    }

    pub fn set_entity_id(&mut self, gidx: usize, entity_id: EntityId) -> Result<(), StorageError> {
        self.entity_ids
            .get_mut(gidx)
            .map(|slot| *slot = entity_id)
            .map_err(StorageError::EntityPool)
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn column_mut(
        &mut self,
        component_id: ComponentId,
    ) -> Result<&mut ComponentColumn, StorageError> {
        let idx = self
            .index_by_component
            .get(&component_id)
            .copied()
            .ok_or(StorageError::ColumnMissing { component_id })?;
        Ok(&mut self.columns[idx])
    }

    pub fn columns_mut_pair(
        &mut self,
        a: ComponentId,
        b: ComponentId,
    ) -> Result<(&mut ComponentColumn, &mut ComponentColumn), StorageError> {
        if a == b {
            return Err(StorageError::DuplicateColumnRequest { component_id: a });
        }

        let idx_a = self
            .index_by_component
            .get(&a)
            .copied()
            .ok_or(StorageError::ColumnMissing { component_id: a })?;
        let idx_b = self
            .index_by_component
            .get(&b)
            .copied()
            .ok_or(StorageError::ColumnMissing { component_id: b })?;

        if idx_a < idx_b {
            let (left, right) = self.columns.split_at_mut(idx_b);
            Ok((&mut left[idx_a], &mut right[0]))
        } else {
            let (left, right) = self.columns.split_at_mut(idx_a);
            Ok((&mut right[0], &mut left[idx_b]))
        }
    }

    pub fn column(&self, component_id: ComponentId) -> Result<&ComponentColumn, StorageError> {
        let idx = self
            .index_by_component
            .get(&component_id)
            .copied()
            .ok_or(StorageError::ColumnMissing { component_id })?;
        Ok(&self.columns[idx])
    }

    pub fn column_slice<T: Component>(&self) -> Result<&[T], StorageError> {
        let component_id = <T as Component>::id();
        let column = self.column(component_id)?;
        column.column_slice_read::<T>().map_err(StorageError::from)
    }

    pub fn column_slice_mut<T: Component>(&mut self) -> Result<&mut [T], StorageError> {
        let component_id = <T as Component>::id();
        let column = self.column_mut(component_id)?;
        column.column_slice_write::<T>().map_err(StorageError::from)
    }

    pub fn alloc_row(&mut self, entity_id: EntityId) -> Result<usize, StorageError> {
        let gidx = self.entity_ids.alloc_one();
        for column in &mut self.columns {
            let col_gidx = column.alloc_one();
            debug_assert_eq!(gidx, col_gidx, "column allocation mismatch");
        }
        self.entity_ids.write_at(gidx, entity_id);
        self.len += 1;
        debug_assert_eq!(self.len, self.entity_ids.len_total());
        Ok(gidx)
    }

    pub fn alloc_bulk(
        &mut self,
        count: usize,
        mut entities: impl Iterator<Item = EntityId>,
    ) -> Result<Vec<Range<usize>>, StorageError> {
        let spans = self.entity_ids.alloc_bulk(count);
        let mut written = 0usize;
        for span in &spans {
            for gidx in span.clone() {
                if let Some(eid) = entities.next() {
                    self.entity_ids.write_at(gidx, eid);
                    written += 1;
                } else {
                    return Err(StorageError::NotEnoughEntities {
                        expected: count,
                        got: written,
                    });
                }
            }
        }
        for column in &mut self.columns {
            let column_spans = column.alloc_bulk(count);
            debug_assert_eq!(spans, column_spans, "column bulk allocation mismatch");
        }
        self.len += count;
        debug_assert_eq!(self.len, self.entity_ids.len_total());
        Ok(spans)
    }

    pub fn write_component(
        &mut self,
        component_id: ComponentId,
        gidx: usize,
        current: &[u8],
        next: Option<&[u8]>,
    ) -> Result<(), StorageError> {
        let column = self.column_mut(component_id)?;
        column.write_cur_at(gidx, current)?;
        if let Some(next_bytes) = next {
            column.write_next_at(gidx, next_bytes)?;
        } else {
            column.copy_cur_to_next(gidx)?;
        }
        Ok(())
    }

    pub fn slice_rw(
        &mut self,
        component_id: ComponentId,
        range: Range<usize>,
    ) -> Result<(&[u8], &mut [u8]), StorageError> {
        let column = self.column_mut(component_id)?;
        column.slice_rw(range).map_err(StorageError::from)
    }

    pub fn swap_buffers(&mut self) {
        for column in &mut self.columns {
            column.swap_buffers();
        }
    }

    pub fn free_one_swap_remove(
        &mut self,
        gidx: usize,
        mut on_move: impl FnMut(usize, usize),
    ) -> Result<(), StorageError> {
        if gidx >= self.len {
            return Err(StorageError::IndexOutOfBounds {
                index: gidx,
                len: self.len,
            });
        }
        let mut moved = None;
        self.entity_ids
            .free_one_swap_remove(gidx, |from, to| moved = Some((from, to)))
            .map_err(StorageError::EntityPool)?;
        for column in &mut self.columns {
            if let Some((from, to)) = column.free_one_swap_remove(gidx)? {
                debug_assert!(moved.map_or(true, |m| m == (from, to)));
                moved = Some((from, to));
            }
        }
        if let Some((from, to)) = moved {
            on_move(from, to);
        }
        self.len -= 1;
        debug_assert_eq!(self.len, self.entity_ids.len_total());
        Ok(())
    }

    pub fn free_bulk_swap_remove(
        &mut self,
        gidxs: Vec<usize>,
        mut on_move: impl FnMut(usize, usize),
    ) -> Result<(), StorageError> {
        if gidxs.is_empty() {
            return Ok(());
        }
        let mut moved = Vec::new();
        self.entity_ids
            .free_bulk_swap_remove(gidxs.clone(), |from, to| moved.push((from, to)))
            .map_err(StorageError::EntityPool)?;
        for column in &mut self.columns {
            let mut column_moves = column.free_bulk_swap_remove(gidxs.clone())?;
            moved.append(&mut column_moves);
        }
        moved.sort_unstable();
        moved.dedup();
        for (from, to) in moved {
            on_move(from, to);
        }
        self.len = self.entity_ids.len_total();
        debug_assert_eq!(self.len, self.entity_ids.len_total());
        Ok(())
    }

    #[inline]
    pub fn current_buffer_index(&self) -> usize {
        0
    }

    #[inline]
    pub fn next_buffer_index(&self) -> usize {
        1
    }

    pub fn get_column_ptr_const(
        &self,
        component_id: ComponentId,
    ) -> Option<*const ComponentColumn> {
        self.index_by_component
            .get(&component_id)
            .map(|&idx| &self.columns[idx] as *const ComponentColumn)
    }

    pub fn get_column_ptr(&mut self, component_id: ComponentId) -> Option<*mut ComponentColumn> {
        self.index_by_component
            .get(&component_id)
            .map(|&idx| &mut self.columns[idx] as *mut ComponentColumn)
    }

    pub unsafe fn column_ptr_to_slice_const<'a, T: Component>(
        column_ptr: *const ComponentColumn,
        _buffer_index: usize,
    ) -> &'a [T] {
        let column = &*column_ptr;
        if column.page_count() > 1 {
            panic!(
                "columns! macro requires component '{}' to fit within a single page (found {} pages)",
                column.plan().meta.name,
                column.page_count()
            );
        }
        column
            .column_slice_read::<T>()
            .unwrap_or_else(|err| panic!("failed to borrow column for read: {err}"))
    }

    pub unsafe fn column_ptr_to_slice<'a, T: Component>(
        column_ptr: *mut ComponentColumn,
        _buffer_index: usize,
    ) -> &'a mut [T] {
        let column = &mut *column_ptr;
        if column.page_count() > 1 {
            panic!(
                "columns_mut! macro requires component '{}' to fit within a single page (found {} pages)",
                column.plan().meta.name,
                column.page_count()
            );
        }
        column
            .column_slice_write::<T>()
            .unwrap_or_else(|err| panic!("failed to borrow column for write: {err}"))
    }
}
