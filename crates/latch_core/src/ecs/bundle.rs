//! Component bundles for spawning entities

use std::any::TypeId;

use super::archetype::{Archetype, ArchetypeId};
use super::component::Component;

/// Trait for a group of components that can be added to an entity atomically
pub trait ComponentBundle {
    /// Calculate the archetype ID for this bundle
    fn archetype_id(&self) -> ArchetypeId;
    
    /// Get the list of component TypeIds
    fn type_ids(&self) -> Vec<TypeId>;
    
    /// Insert components into archetype storage
    fn insert_into(self, archetype: &mut Archetype);
}

// Implement for single component
impl<T1> ComponentBundle for (T1,)
where
    T1: Component + Clone,
{
    fn archetype_id(&self) -> ArchetypeId {
        ArchetypeId::from_types(&[TypeId::of::<T1>()])
    }
    
    fn type_ids(&self) -> Vec<TypeId> {
        vec![TypeId::of::<T1>()]
    }
    
    fn insert_into(self, archetype: &mut Archetype) {
        if !archetype.components.contains_key(&TypeId::of::<T1>()) {
            archetype.add_storage::<T1>();
        }
        archetype.get_storage_mut::<T1>().unwrap().push(self.0);
    }
}

// Implement for two components
impl<T1, T2> ComponentBundle for (T1, T2)
where
    T1: Component + Clone,
    T2: Component + Clone,
{
    fn archetype_id(&self) -> ArchetypeId {
        ArchetypeId::from_types(&[TypeId::of::<T1>(), TypeId::of::<T2>()])
    }
    
    fn type_ids(&self) -> Vec<TypeId> {
        vec![TypeId::of::<T1>(), TypeId::of::<T2>()]
    }
    
    fn insert_into(self, archetype: &mut Archetype) {
        if !archetype.components.contains_key(&TypeId::of::<T1>()) {
            archetype.add_storage::<T1>();
        }
        if !archetype.components.contains_key(&TypeId::of::<T2>()) {
            archetype.add_storage::<T2>();
        }
        archetype.get_storage_mut::<T1>().unwrap().push(self.0);
        archetype.get_storage_mut::<T2>().unwrap().push(self.1);
    }
}

// Implement for three components
impl<T1, T2, T3> ComponentBundle for (T1, T2, T3)
where
    T1: Component + Clone,
    T2: Component + Clone,
    T3: Component + Clone,
{
    fn archetype_id(&self) -> ArchetypeId {
        ArchetypeId::from_types(&[TypeId::of::<T1>(), TypeId::of::<T2>(), TypeId::of::<T3>()])
    }
    
    fn type_ids(&self) -> Vec<TypeId> {
        vec![TypeId::of::<T1>(), TypeId::of::<T2>(), TypeId::of::<T3>()]
    }
    
    fn insert_into(self, archetype: &mut Archetype) {
        if !archetype.components.contains_key(&TypeId::of::<T1>()) {
            archetype.add_storage::<T1>();
        }
        if !archetype.components.contains_key(&TypeId::of::<T2>()) {
            archetype.add_storage::<T2>();
        }
        if !archetype.components.contains_key(&TypeId::of::<T3>()) {
            archetype.add_storage::<T3>();
        }
        archetype.get_storage_mut::<T1>().unwrap().push(self.0);
        archetype.get_storage_mut::<T2>().unwrap().push(self.1);
        archetype.get_storage_mut::<T3>().unwrap().push(self.2);
    }
}

// Implement for four components
impl<T1, T2, T3, T4> ComponentBundle for (T1, T2, T3, T4)
where
    T1: Component + Clone,
    T2: Component + Clone,
    T3: Component + Clone,
    T4: Component + Clone,
{
    fn archetype_id(&self) -> ArchetypeId {
        ArchetypeId::from_types(&[
            TypeId::of::<T1>(),
            TypeId::of::<T2>(),
            TypeId::of::<T3>(),
            TypeId::of::<T4>(),
        ])
    }
    
    fn type_ids(&self) -> Vec<TypeId> {
        vec![
            TypeId::of::<T1>(),
            TypeId::of::<T2>(),
            TypeId::of::<T3>(),
            TypeId::of::<T4>(),
        ]
    }
    
    fn insert_into(self, archetype: &mut Archetype) {
        if !archetype.components.contains_key(&TypeId::of::<T1>()) {
            archetype.add_storage::<T1>();
        }
        if !archetype.components.contains_key(&TypeId::of::<T2>()) {
            archetype.add_storage::<T2>();
        }
        if !archetype.components.contains_key(&TypeId::of::<T3>()) {
            archetype.add_storage::<T3>();
        }
        if !archetype.components.contains_key(&TypeId::of::<T4>()) {
            archetype.add_storage::<T4>();
        }
        archetype.get_storage_mut::<T1>().unwrap().push(self.0);
        archetype.get_storage_mut::<T2>().unwrap().push(self.1);
        archetype.get_storage_mut::<T3>().unwrap().push(self.2);
        archetype.get_storage_mut::<T4>().unwrap().push(self.3);
    }
}
