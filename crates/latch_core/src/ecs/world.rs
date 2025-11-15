use crate::ecs::{
    storage::{plan_archetype, ArchetypeStorage, PageBudget, PlanError, StorageError},
    ArchetypeId, ArchetypeLayout, Component, ComponentId, Entity, EntityBuilder,
    EntityBuilderError, EntityId, EntityLoc, Generation, SystemDescriptor, SystemHandle,
    SystemRegistrationError, SystemRegistry,
};
use std::{collections::HashMap, convert::TryFrom};
use thiserror::Error;

struct ArchetypeEntry {
    storage: ArchetypeStorage,
    pending_despawns: Vec<usize>,
}

impl ArchetypeEntry {
    fn new(storage: ArchetypeStorage) -> Self {
        Self {
            storage,
            pending_despawns: Vec::new(),
        }
    }
}

#[derive(Copy, Clone, Debug)]
struct SlotLocation {
    archetype: ArchetypeId,
    row: usize,
}

#[derive(Debug)]
struct EntitySlot {
    generation: Generation,
    location: Option<SlotLocation>,
}

impl EntitySlot {
    fn new() -> Self {
        Self {
            generation: 0,
            location: None,
        }
    }
}

#[derive(Debug, Error)]
pub enum WorldError {
    #[error(transparent)]
    Builder(#[from] EntityBuilderError),
    #[error(transparent)]
    Plan(#[from] PlanError),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("entity {entity:?} refers to unknown index")]
    UnknownEntity { entity: Entity },
    #[error("entity {entity:?} is stale")]
    StaleEntity { entity: Entity },
    #[error("entity {entity:?} is not alive")]
    EntityNotAlive { entity: Entity },
    #[error("entity index overflow: {index}")]
    EntityIndexOverflow { index: usize },
    #[error("entity slot missing for id {entity_id}")]
    UnknownEntityIndex { entity_id: EntityId },
    #[error("storage for archetype {archetype_id} missing")]
    MissingArchetype { archetype_id: ArchetypeId },
}

pub struct World {
    page_budget: PageBudget,
    storages: HashMap<ArchetypeId, ArchetypeEntry>,
    component_index: HashMap<ComponentId, Vec<ArchetypeId>>,
    systems: SystemRegistry,
    slots: Vec<EntitySlot>,
    free_list: Vec<EntityId>,
    live_count: usize,
}

impl World {
    pub fn new() -> Self {
        Self::with_page_budget(PageBudget::detect())
    }

    pub fn with_page_budget(page_budget: PageBudget) -> Self {
        Self {
            page_budget,
            storages: HashMap::new(),
            component_index: HashMap::new(),
            systems: SystemRegistry::new(),
            slots: Vec::new(),
            free_list: Vec::new(),
            live_count: 0,
        }
    }

    pub fn page_budget(&self) -> PageBudget {
        self.page_budget
    }

    pub fn set_page_budget(&mut self, budget: PageBudget) {
        self.page_budget = budget;
    }

    pub fn spawn(&mut self, builder: EntityBuilder) -> Result<Entity, WorldError> {
        let blueprint = builder.build()?;
        let archetype_id = blueprint.layout().id();
        self.ensure_archetype_exists(blueprint.layout())?;

        let (entity, entity_id) = self.allocate_entity()?;

        let row = {
            let entry = self
                .storages
                .get_mut(&archetype_id)
                .ok_or(WorldError::MissingArchetype { archetype_id })?;
            let row = entry.storage.alloc_row(entity_id)?;
            for component in blueprint.components() {
                entry.storage.write_component(
                    component.component_id(),
                    row,
                    component.bytes(),
                    None,
                )?;
            }
            row
        };

        self.record_location(
            entity_id,
            SlotLocation {
                archetype: archetype_id,
                row,
            },
        )?;
        self.live_count += 1;
        Ok(entity)
    }

    pub fn despawn(&mut self, entity: Entity) -> Result<(), WorldError> {
        let index = entity.index() as usize;
        let slot = self
            .slots
            .get_mut(index)
            .ok_or(WorldError::UnknownEntity { entity })?;
        if slot.generation != entity.generation() {
            return Err(WorldError::StaleEntity { entity });
        }
        let location = slot
            .location
            .take()
            .ok_or(WorldError::EntityNotAlive { entity })?;
        let entry =
            self.storages
                .get_mut(&location.archetype)
                .ok_or(WorldError::MissingArchetype {
                    archetype_id: location.archetype,
                })?;
        entry.pending_despawns.push(location.row);
        self.live_count = self.live_count.saturating_sub(1);
        Ok(())
    }

    pub fn flush_despawns(&mut self) -> Result<(), WorldError> {
        let archetype_ids: Vec<ArchetypeId> = self
            .storages
            .iter()
            .filter(|(_, entry)| !entry.pending_despawns.is_empty())
            .map(|(id, _)| *id)
            .collect();

        for archetype_id in archetype_ids {
            let mut victims = Vec::new();
            let mut move_updates = Vec::new();
            {
                let entry = self
                    .storages
                    .get_mut(&archetype_id)
                    .ok_or(WorldError::MissingArchetype { archetype_id })?;
                if entry.pending_despawns.is_empty() {
                    continue;
                }
                entry.pending_despawns.sort_unstable();
                entry.pending_despawns.dedup();

                for &row in &entry.pending_despawns {
                    victims.push(entry.storage.entity_id_at(row)?);
                }

                let mut move_rows = Vec::new();
                entry.storage.free_bulk_swap_remove(
                    entry.pending_despawns.clone(),
                    |from, to| {
                        move_rows.push((from, to));
                    },
                )?;
                entry.pending_despawns.clear();

                for (_from, to) in move_rows {
                    let entity_id = entry.storage.entity_id_at(to)?;
                    move_updates.push((entity_id, to));
                }
            }

            for entity_id in victims {
                self.finish_despawn(entity_id)?;
            }
            for (entity_id, row) in move_updates {
                self.update_entity_location(entity_id, archetype_id, row)?;
            }
        }

        Ok(())
    }

    pub fn locate(&self, entity: Entity) -> Result<EntityLoc, WorldError> {
        let index = entity.index() as usize;
        let slot = self
            .slots
            .get(index)
            .ok_or(WorldError::UnknownEntity { entity })?;
        if slot.generation != entity.generation() {
            return Err(WorldError::StaleEntity { entity });
        }
        let location = slot.location.ok_or(WorldError::EntityNotAlive { entity })?;
        Ok(EntityLoc::new(
            location.archetype,
            location.row,
            slot.generation,
        ))
    }

    pub fn storage(&self, archetype: ArchetypeId) -> Option<&ArchetypeStorage> {
        self.storages.get(&archetype).map(|entry| &entry.storage)
    }

    pub fn storage_mut(&mut self, archetype: ArchetypeId) -> Option<&mut ArchetypeStorage> {
        self.storages
            .get_mut(&archetype)
            .map(|entry| &mut entry.storage)
    }

    pub fn archetypes_with(&self, component_id: ComponentId) -> &[ArchetypeId] {
        self.component_index
            .get(&component_id)
            .map(|ids| ids.as_slice())
            .unwrap_or(&[])
    }

    pub fn register_system(
        &mut self,
        descriptor: SystemDescriptor,
    ) -> Result<SystemHandle, SystemRegistrationError> {
        self.systems.register(descriptor)
    }

    pub fn system_descriptor(&self, handle: SystemHandle) -> Option<&SystemDescriptor> {
        self.systems.descriptor(handle)
    }

    pub fn system_components(&self, handle: SystemHandle) -> Option<&[ComponentId]> {
        self.systems.component_filter(handle)
    }

    pub fn system_read_components(&self, handle: SystemHandle) -> Option<&[ComponentId]> {
        self.systems.read_components(handle)
    }

    pub fn system_write_components(&self, handle: SystemHandle) -> Option<&[ComponentId]> {
        self.systems.write_components(handle)
    }

    pub fn systems(&self) -> impl Iterator<Item = (SystemHandle, &SystemDescriptor)> {
        self.systems.iter()
    }

    pub fn live_entity_count(&self) -> usize {
        self.live_count
    }

    pub fn allocated_slots(&self) -> usize {
        self.slots.len()
    }

    pub fn swap_buffers(&mut self) {
        for entry in self.storages.values_mut() {
            entry.storage.swap_buffers();
        }
    }

    pub fn for_each(
        &mut self,
        component_ids: &[ComponentId],
        mut f: impl FnMut(&mut ArchetypeStorage),
    ) {
        if component_ids.is_empty() {
            return;
        }

        let mut ids = component_ids.to_vec();
        ids.sort_unstable();
        ids.dedup();

        for entry in self.storages.values_mut() {
            if entry.storage.is_empty() {
                continue;
            }
            let layout_components = entry.storage.plan().layout.components();
            if ids.iter().all(|id| layout_components.contains(id)) {
                f(&mut entry.storage);
            }
        }
    }

    pub fn column<T: Component>(&self, archetype: ArchetypeId) -> Option<&[T]> {
        self.storages
            .get(&archetype)
            .and_then(|entry| entry.storage.column_slice::<T>().ok())
    }

    pub fn entity_count(&self) -> usize {
        self.live_count
    }

    fn ensure_archetype_exists(&mut self, layout: &ArchetypeLayout) -> Result<(), WorldError> {
        let archetype_id = layout.id();
        if self.storages.contains_key(&archetype_id) {
            return Ok(());
        }

        let plan = plan_archetype(layout.clone(), self.page_budget)?;
        let component_ids: Vec<ComponentId> =
            plan.columns.iter().map(|col| col.component_id).collect();
        let storage = ArchetypeStorage::from_plan(plan);
        self.storages
            .insert(archetype_id, ArchetypeEntry::new(storage));
        for component_id in component_ids {
            self.component_index
                .entry(component_id)
                .or_default()
                .push(archetype_id);
        }
        Ok(())
    }

    fn allocate_entity(&mut self) -> Result<(Entity, EntityId), WorldError> {
        let entity_id = if let Some(id) = self.free_list.pop() {
            id
        } else {
            let index = self.slots.len();
            let id = u32::try_from(index).map_err(|_| WorldError::EntityIndexOverflow { index })?;
            self.slots.push(EntitySlot::new());
            id
        };

        let slot_index = entity_id as usize;
        let generation = self
            .slots
            .get(slot_index)
            .map(|slot| slot.generation)
            .ok_or(WorldError::UnknownEntityIndex { entity_id })?;
        let entity = Entity::new(entity_id, generation);
        Ok((entity, entity_id))
    }

    fn record_location(
        &mut self,
        entity_id: EntityId,
        loc: SlotLocation,
    ) -> Result<(), WorldError> {
        let slot = self
            .slots
            .get_mut(entity_id as usize)
            .ok_or(WorldError::UnknownEntityIndex { entity_id })?;
        slot.location = Some(loc);
        Ok(())
    }

    fn finish_despawn(&mut self, entity_id: EntityId) -> Result<(), WorldError> {
        let slot = self
            .slots
            .get_mut(entity_id as usize)
            .ok_or(WorldError::UnknownEntityIndex { entity_id })?;
        debug_assert!(slot.location.is_none());
        slot.generation = slot.generation.wrapping_add(1);
        self.free_list.push(entity_id);
        Ok(())
    }

    fn update_entity_location(
        &mut self,
        entity_id: EntityId,
        archetype: ArchetypeId,
        row: usize,
    ) -> Result<(), WorldError> {
        let slot = self
            .slots
            .get_mut(entity_id as usize)
            .ok_or(WorldError::UnknownEntityIndex { entity_id })?;
        match &mut slot.location {
            Some(loc) => {
                debug_assert_eq!(loc.archetype, archetype);
                loc.row = row;
            }
            None => {
                slot.location = Some(SlotLocation { archetype, row });
            }
        }
        Ok(())
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}
