use crate::ecs::{Entity, EntityId};
use crate::pool::PagedPool;
use std::collections::hash_map::Entry;
use std::collections::HashMap;

/// Identifier describing the semantic meaning of a relation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RelationType(u16);

impl RelationType {
    pub const fn new(raw: u16) -> Self {
        Self(raw)
    }

    #[inline]
    pub const fn raw(self) -> u16 {
        self.0
    }
}

/// Optional payload attached to a relation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelationPayloadRange {
    pub start: u32,
    pub len: u32,
}

impl RelationPayloadRange {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// Canonical record describing a pair of related entities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelationRecord {
    pub entity_a: Entity,
    pub entity_b: Entity,
    pub relation_type: RelationType,
    pub payload: Option<RelationPayloadRange>,
}

impl RelationRecord {
    #[inline]
    pub fn new(
        a: Entity,
        b: Entity,
        relation_type: RelationType,
        payload: Option<RelationPayloadRange>,
    ) -> Self {
        Self {
            entity_a: a,
            entity_b: b,
            relation_type,
            payload,
        }
    }
}

/// Optional offset metadata describing the relative delta between two entities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelationDelta {
    pub dx: i32,
    pub dy: i32,
}

impl RelationDelta {
    #[inline]
    pub fn flipped(self) -> Self {
        Self {
            dx: -self.dx,
            dy: -self.dy,
        }
    }
}

/// Per-entity view of emitted relations to avoid rebuilding component columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntityRelationEntry {
    pub other: Entity,
    pub relation_type: RelationType,
    pub payload: Option<RelationPayloadRange>,
    pub delta: Option<RelationDelta>,
}

#[derive(Debug)]
struct EntityRelationBucket {
    entity_id: EntityId,
    entries: Vec<EntityRelationEntry>,
}

impl EntityRelationBucket {
    fn new(entity_id: EntityId) -> Self {
        Self {
            entity_id,
            entries: Vec::new(),
        }
    }
}

/// Paged arena that stores relation headers and optional payloads without
/// reallocation each tick.
pub struct RelationBuffer {
    records: PagedPool<RelationRecord>,
    payload_bytes: PagedPool<u8>,
    record_count: usize,
    payload_count: usize,
    entity_buckets: Vec<EntityRelationBucket>,
    active_buckets: Vec<usize>,
    free_buckets: Vec<usize>,
    bucket_lookup: HashMap<EntityId, usize>,
}

impl RelationBuffer {
    pub fn new(record_rows_per_page: usize, payload_bytes_per_page: usize) -> Self {
        Self {
            records: PagedPool::with_rows_per_page(record_rows_per_page.max(1).next_power_of_two()),
            payload_bytes: PagedPool::with_rows_per_page(
                payload_bytes_per_page.max(1).next_power_of_two(),
            ),
            record_count: 0,
            payload_count: 0,
            entity_buckets: Vec::new(),
            active_buckets: Vec::new(),
            free_buckets: Vec::new(),
            bucket_lookup: HashMap::new(),
        }
    }

    pub fn clear(&mut self) {
        self.records.clear();
        self.payload_bytes.clear();
        self.record_count = 0;
        self.payload_count = 0;
        for idx in self.active_buckets.drain(..) {
            if let Some(bucket) = self.entity_buckets.get_mut(idx) {
                bucket.entries.clear();
            }
            self.free_buckets.push(idx);
        }
        self.bucket_lookup.clear();
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.record_count
    }

    pub fn push_relation(
        &mut self,
        record: RelationRecord,
        payload_bytes: &[u8],
        delta: Option<RelationDelta>,
    ) {
        let payload = if payload_bytes.is_empty() {
            None
        } else {
            let start = self.payload_count;
            let len = payload_bytes.len();
            let spans = self
                .payload_bytes
                .alloc_bulk(payload_bytes.len())
                .into_iter()
                .flat_map(|range| range);
            for (offset, gidx) in spans.enumerate() {
                self.payload_bytes.write_at(gidx, payload_bytes[offset]);
            }
            self.payload_count += len;
            Some(RelationPayloadRange {
                start: start as u32,
                len: len as u32,
            })
        };

        let mut stored = record;
        stored.payload = payload;
        let index = self.records.alloc_one();
        self.records.write_at(index, stored);
        self.record_count += 1;

        let delta_a = delta;
        let delta_b = delta.map(RelationDelta::flipped);

        self.append_bucket_edge(
            stored.entity_a.index(),
            stored.entity_b,
            stored.relation_type,
            stored.payload,
            delta_a,
        );
        self.append_bucket_edge(
            stored.entity_b.index(),
            stored.entity_a,
            stored.relation_type,
            stored.payload,
            delta_b,
        );
    }

    pub fn iter(&self) -> RelationIter<'_> {
        RelationIter {
            buffer: self,
            cursor: 0,
        }
    }

    pub fn relations_for_entity_id(&self, entity_id: EntityId) -> &[EntityRelationEntry] {
        if let Some(idx) = self.bucket_lookup.get(&entity_id) {
            return &self.entity_buckets[*idx].entries;
        }
        &[]
    }

    pub fn relations_for(&self, entity: Entity) -> &[EntityRelationEntry] {
        self.relations_for_entity_id(entity.index())
    }

    pub fn payload_slice(&self, range: RelationPayloadRange) -> Option<Vec<u8>> {
        if range.is_empty() {
            return Some(Vec::new());
        }
        let start = range.start as usize;
        let end = start + range.len as usize;
        let mut bytes = Vec::with_capacity(range.len as usize);
        let mut cursor = start;
        while cursor < end {
            let tile = self
                .payload_bytes
                .clamp_to_page(cursor, end - cursor, self.payload_count);
            match self.payload_bytes.slice_tile(tile.clone()) {
                Ok(slice) => bytes.extend_from_slice(slice),
                Err(_) => return None,
            }
            cursor = tile.end;
        }
        Some(bytes)
    }

    fn append_bucket_edge(
        &mut self,
        entity_id: EntityId,
        other: Entity,
        relation_type: RelationType,
        payload: Option<RelationPayloadRange>,
        delta: Option<RelationDelta>,
    ) {
        let bucket = self.bucket_mut(entity_id);
        bucket.entries.push(EntityRelationEntry {
            other,
            relation_type,
            payload,
            delta,
        });
    }

    fn bucket_mut(&mut self, entity_id: EntityId) -> &mut EntityRelationBucket {
        match self.bucket_lookup.entry(entity_id) {
            Entry::Occupied(slot) => {
                let idx = *slot.get();
                &mut self.entity_buckets[idx]
            }
            Entry::Vacant(slot) => {
                let idx = if let Some(free_idx) = self.free_buckets.pop() {
                    let bucket = &mut self.entity_buckets[free_idx];
                    bucket.entity_id = entity_id;
                    bucket.entries.clear();
                    free_idx
                } else {
                    self.entity_buckets
                        .push(EntityRelationBucket::new(entity_id));
                    self.entity_buckets.len() - 1
                };
                self.active_buckets.push(idx);
                slot.insert(idx);
                &mut self.entity_buckets[idx]
            }
        }
    }
}

pub struct RelationIter<'a> {
    buffer: &'a RelationBuffer,
    cursor: usize,
}

impl<'a> Iterator for RelationIter<'a> {
    type Item = RelationRecord;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor >= self.buffer.record_count {
            return None;
        }
        let gidx = self.cursor;
        self.cursor += 1;
        match self.buffer.records.get(gidx) {
            Ok(record) => Some(*record),
            Err(_) => None,
        }
    }
}
