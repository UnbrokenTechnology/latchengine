// world.rs
use crate::archetype::{Archetype, ArchetypeId};
use crate::component::{meta_of, Component, ComponentId, ComponentMeta};
use crate::entity::{Entity, EntityBuilder};
use rayon::prelude::*;
use std::collections::{HashMap, hash_map::Entry};

pub struct ComponentRef<'a> {
    pub cid: ComponentId,
    pub bytes: &'a [u8],
}

#[derive(Default)]
pub struct World {
    next_entity_id: u64,
    storages: HashMap<ArchetypeId, ArchetypeStorage>,
    comp_index: HashMap<ComponentId, Vec<ArchetypeId>>,
}

impl World {
    pub fn new() -> Self {
        Self { next_entity_id: 1, storages: HashMap::new(), comp_index: HashMap::new() }
    }

    /// Spawn an entity from a builder. Uses a per-archetype pool to reuse slots.
    pub fn spawn(&mut self, builder: EntityBuilder) -> Entity {
        let archetype = builder.archetype();

        // Detect first-time creation of this archetype storage.
        let storage = match self.storages.entry(archetype.id) {
            Entry::Occupied(o) => o.into_mut(),
            Entry::Vacant(v) => {
                // Insert storage…
                let s = v.insert(ArchetypeStorage::new(archetype.clone()));
                // …and update reverse index once.
                for &cid in &archetype.components {
                    self.comp_index.entry(cid).or_default().push(archetype.id);
                }
                s
            }
        };

        // Ensure columns exist (registration must already be done).
        for &cid in &archetype.components {
            let meta = meta_of(cid).expect("component not registered");
            storage.ensure_column(meta);
        }

        let id = self.next_entity_id;
        self.next_entity_id += 1;

        let row = storage.alloc_row(); // pool reuse
        for (cid, bytes) in builder.into_components() {
            storage.write_component(row, cid, &bytes);
        }
        storage.set_entity(row, id);

        Entity { id, archetype: archetype.id, index: row }
    }

    /// Despawn (return slot to pool).
    pub fn despawn(&mut self, e: Entity) {
        if let Some(storage) = self.storages.get_mut(&e.archetype) {
            storage.free_row(e.index);
        }
    }

    /// Get a typed immutable column slice for a component in an archetype.
    pub fn column<T: Component>(&self, a: ArchetypeId) -> Option<&[T]> {
        self.storages.get(&a)?.column_as_slice::<T>()
    }

    /// Get a typed mutable column slice for a component in an archetype.
    pub fn column_mut<T: Component>(&mut self, a: ArchetypeId) -> Option<&mut [T]> {
        self.storages.get_mut(&a)?.column_as_slice_mut::<T>()
    }

    /// Return archetype IDs that contain `cid`.
    pub fn archetypes_with(&self, cid: ComponentId) -> Vec<ArchetypeId> {
        self.storages
            .iter()
            .filter(|(_, s)| s.archetype.contains(cid))
            .map(|(id, _)| *id)
            .collect()
    }

    /// Parallel map over all rows of an archetype touching **one** component.
    pub fn for_each<T, F>(&mut self, f: F)
    where
        T: Component + Send,
        F: Fn(&mut T) + Sync + Send,
    {
        // Take a *shared* reference to the archetype list first.
        let Some(ids_ref) = self.comp_index.get(&T::ID) else {
            return;
        };

        // Iterate over each archetype ID; we borrow storages mutably one by one.
        for &arch_id in ids_ref {
            if let Some(storage) = self.storages.get_mut(&arch_id) {
                if let Some(slice) = storage.column_as_slice_mut::<T>() {
                    slice.par_chunks_mut(1024).for_each(|chunk| {
                        for x in chunk {
                            f(x);
                        }
                    });
                }
            }
        }
    }

    /// Retrieve all archetypes (e.g., to schedule systems).
    pub fn archetypes(&self) -> impl Iterator<Item = &ArchetypeStorage> {
        self.storages.values()
    }

    /// Immutable slices of T across all archetypes that contain T.
    pub fn all_components<T: Component>(&self) -> Vec<&[T]> {
        let Some(ids) = self.comp_index.get(&T::ID) else { return Vec::new(); };
        let mut out = Vec::with_capacity(ids.len());
        for &arch in ids {
            if let Some(s) = self.storages.get(&arch) {
                if let Some(slice) = s.column_as_slice::<T>() {
                    out.push(slice);
                }
            }
        }
        out
    }

    /// Mutate all T across all archetypes without allocating.
    pub fn with_all_components_mut<T, F>(&mut self, mut f: F)
    where
        T: Component + Send,
        F: FnMut(&mut [T]),
    {
        let Some(ids) = self.comp_index.get(&T::ID) else { return; };
        for &arch in ids {
            if let Some(s) = self.storages.get_mut(&arch) {
                if let Some(slice) = s.column_as_slice_mut::<T>() {
                    f(slice);
                }
            }
        }
    }

    pub fn get_component<T: Component>(&self, e: Entity) -> Option<&T> {
        self.storages.get(&e.archetype)?.row_typed::<T>(e.index)
    }
    pub fn get_component_mut<T: Component>(&mut self, e: Entity) -> Option<&mut T> {
        self.storages.get_mut(&e.archetype)?.row_typed_mut::<T>(e.index)
    }

    pub fn components_of_entity_raw<'a>(&'a self, e: Entity) -> Vec<ComponentRef<'a>> {
        let Some(s) = self.storages.get(&e.archetype) else { return Vec::new(); };
        let mut out = Vec::with_capacity(s.archetype.components.len());
        for &cid in &s.archetype.components {
            if let Some(b) = s.row_bytes(cid, e.index) {
                out.push(ComponentRef { cid, bytes: b });
            }
        }
        out
    }
}

/// One SoA storage per archetype (columns keyed by component id).
pub struct ArchetypeStorage {
    pub archetype: Archetype,
    columns: HashMap<ComponentId, Column>,
    len: usize,
    free: Vec<usize>, // pool of free row indices
    entity_ids: Vec<Option<u64>>,
}

impl ArchetypeStorage {
    fn new(archetype: Archetype) -> Self {
        Self { archetype, columns: HashMap::new(), len: 0, free: Vec::new(), entity_ids: Vec::new() }
    }

    fn ensure_column(&mut self, meta: ComponentMeta) {
        self.columns.entry(meta.id).or_insert_with(|| Column::new(meta));
    }

    fn alloc_row(&mut self) -> usize {
        if let Some(idx) = self.free.pop() {
            self.entity_ids[idx] = None;
            idx
        } else {
            let row = self.len;
            self.len += 1;
            // Grow each column by one element worth of bytes.
            for col in self.columns.values_mut() {
                col.grow_one();
            }
            self.entity_ids.push(None); // keep in sync with columns
            row
        }
    }

    fn free_row(&mut self, idx: usize) {
        // Optional: write tombstone data. We simply push to free list.
        self.entity_ids[idx] = None;
        self.free.push(idx);
    }

    fn write_component(&mut self, row: usize, cid: ComponentId, bytes: &[u8]) {
        let col = self.columns.get_mut(&cid).expect("column missing");
        col.write_row(row, bytes);
    }

    fn set_entity(&mut self, row: usize, eid: u64) {
        self.entity_ids[row] = Some(eid);
    }

    fn entity_at(&self, row: usize) -> Option<u64> {
        self.entity_ids.get(row).and_then(|x| *x)
    }

    // Raw view of a single row for a given component
    fn row_bytes(&self, cid: ComponentId, row: usize) -> Option<&[u8]> {
        let col = self.columns.get(&cid)?;
        let size = col.elem_size;
        let start = row * size;
        Some(&col.bytes[start..start + size])
    }

    // Typed single-row access
    fn row_typed<T: Component>(&self, row: usize) -> Option<&T> {
        let col = self.columns.get(&T::ID)?;
        let start = row * std::mem::size_of::<T>();
        Some(unsafe { &*(col.bytes[start..].as_ptr() as *const T) })
    }
    fn row_typed_mut<T: Component>(&mut self, row: usize) -> Option<&mut T> {
        let col = self.columns.get_mut(&T::ID)?;
        let start = row * std::mem::size_of::<T>();
        Some(unsafe { &mut *(col.bytes[start..].as_mut_ptr() as *mut T) })
    }

    pub fn len(&self) -> usize { self.len - self.free.len() }

    pub fn column_as_slice<T: Component>(&self) -> Option<&[T]> {
        let meta = meta_of(T::ID)?;
        let col = self.columns.get(&T::ID)?;
        assert_eq!(meta.size, col.elem_size);
        let ptr = col.bytes.as_ptr();
        let len = col.bytes.len() / meta.size;
        // SAFETY: Column holds tightly packed POD elements of T with correct alignment.
        Some(unsafe { std::slice::from_raw_parts(ptr as *const T, len) })
    }

    pub fn column_as_slice_mut<T: Component>(&mut self) -> Option<&mut [T]> {
        let meta = meta_of(T::ID)?;
        let col = self.columns.get_mut(&T::ID)?;
        assert_eq!(meta.size, col.elem_size);
        let ptr = col.bytes.as_mut_ptr();
        let len = col.bytes.len() / meta.size;
        Some(unsafe { std::slice::from_raw_parts_mut(ptr as *mut T, len) })
    }
}

/// A raw byte column (SoA) for a given component type.
struct Column {
    elem_size: usize,
    elem_align: usize,
    bytes: Vec<u8>, // len = rows * elem_size
}

impl Column {
    fn new(meta: ComponentMeta) -> Self {
        Self { elem_size: meta.size, elem_align: meta.align, bytes: Vec::new() }
    }

    fn grow_one(&mut self) {
        let target = self.bytes.len() + self.elem_size;
        self.bytes.resize(target, 0);
        // alignment is guaranteed by allocation; elements are tightly packed POD
    }

    fn write_row(&mut self, row: usize, src: &[u8]) {
        assert_eq!(src.len(), self.elem_size);
        let start = row * self.elem_size;
        self.bytes[start..start + self.elem_size].copy_from_slice(src);
    }

    unsafe fn as_mut_slice<T>(&mut self) -> &mut [T] {
        debug_assert_eq!(self.elem_size, std::mem::size_of::<T>());
        let len = self.bytes.len() / self.elem_size;
        std::slice::from_raw_parts_mut(self.bytes.as_mut_ptr() as *mut T, len)
    }
}