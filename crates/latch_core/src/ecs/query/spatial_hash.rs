//! Spatial hash accelerator that emits broad-phase relation pairs in a single pass.

use super::{RelationAccelerator, RelationBuffer, RelationDelta, RelationRecord, RelationType};
use crate::ecs::{ComponentId, Entity, World};
use std::collections::{hash_map::Entry, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::LazyLock;
use std::time::Instant;

#[derive(Clone, Copy, Debug)]
pub struct SpatialHashConfig {
    pub component_id: ComponentId,
    pub cell_size: i32,
    pub radius: i32,
    pub relation: RelationType,
}

impl SpatialHashConfig {
    pub fn new(
        component_id: ComponentId,
        cell_size: i32,
        radius: i32,
        relation: RelationType,
    ) -> Self {
        Self {
            component_id,
            cell_size: cell_size.max(1),
            radius: radius.max(1),
            relation,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct CellCoord {
    x: i32,
    y: i32,
}

impl CellCoord {
    fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    fn neighbors(&self) -> [CellCoord; 8] {
        [
            CellCoord::new(self.x + 1, self.y - 1),
            CellCoord::new(self.x + 1, self.y),
            CellCoord::new(self.x + 1, self.y + 1),
            CellCoord::new(self.x, self.y + 1),
            CellCoord::new(self.x, self.y - 1),
            CellCoord::new(self.x - 1, self.y - 1),
            CellCoord::new(self.x - 1, self.y),
            CellCoord::new(self.x - 1, self.y + 1),
        ]
    }
}

#[derive(Clone, Copy, Debug)]
struct GridEntry {
    entity: Entity,
    coord: CellCoord,
    x: i32,
    y: i32,
}

pub struct SpatialHashGrid {
    config: SpatialHashConfig,
    buckets: HashMap<CellCoord, Vec<GridEntry>>,
    bucket_pool: Vec<Vec<GridEntry>>,
}

#[derive(Default)]
struct StageMetric {
    nanos: AtomicU64,
    calls: AtomicU64,
}

impl StageMetric {
    fn record(&self, duration_ns: u64) {
        self.nanos.fetch_add(duration_ns, Ordering::Relaxed);
        self.calls.fetch_add(1, Ordering::Relaxed);
    }

    fn snapshot(&self) -> (u64, u64) {
        (
            self.nanos.load(Ordering::Relaxed),
            self.calls.load(Ordering::Relaxed),
        )
    }

    fn reset(&self) {
        self.nanos.store(0, Ordering::Relaxed);
        self.calls.store(0, Ordering::Relaxed);
    }
}

struct SpatialHashMetrics {
    total: StageMetric,
    recycle: StageMetric,
    emit: StageMetric,
    entities: AtomicU64,
    relations: AtomicU64,
    bucket_lookups: AtomicU64,
    bucket_hits: AtomicU64,
    bucket_reuses: AtomicU64,
    bucket_allocs: AtomicU64,
}

impl SpatialHashMetrics {
    const fn new() -> Self {
        Self {
            total: StageMetric {
                nanos: AtomicU64::new(0),
                calls: AtomicU64::new(0),
            },
            recycle: StageMetric {
                nanos: AtomicU64::new(0),
                calls: AtomicU64::new(0),
            },
            emit: StageMetric {
                nanos: AtomicU64::new(0),
                calls: AtomicU64::new(0),
            },
            entities: AtomicU64::new(0),
            relations: AtomicU64::new(0),
            bucket_lookups: AtomicU64::new(0),
            bucket_hits: AtomicU64::new(0),
            bucket_reuses: AtomicU64::new(0),
            bucket_allocs: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> SpatialHashMetricsSnapshot {
        let (total_ns, total_calls) = self.total.snapshot();
        let (recycle_ns, recycle_calls) = self.recycle.snapshot();
        let (emit_ns, emit_calls) = self.emit.snapshot();
        SpatialHashMetricsSnapshot {
            total_ns,
            total_calls,
            recycle_ns,
            recycle_calls,
            emit_ns,
            emit_calls,
            entities: self.entities.load(Ordering::Relaxed),
            relations: self.relations.load(Ordering::Relaxed),
            bucket_lookups: self.bucket_lookups.load(Ordering::Relaxed),
            bucket_hits: self.bucket_hits.load(Ordering::Relaxed),
            bucket_reuses: self.bucket_reuses.load(Ordering::Relaxed),
            bucket_allocs: self.bucket_allocs.load(Ordering::Relaxed),
        }
    }

    fn reset(&self) {
        self.total.reset();
        self.recycle.reset();
        self.emit.reset();
        self.entities.store(0, Ordering::Relaxed);
        self.relations.store(0, Ordering::Relaxed);
        self.bucket_lookups.store(0, Ordering::Relaxed);
        self.bucket_hits.store(0, Ordering::Relaxed);
        self.bucket_reuses.store(0, Ordering::Relaxed);
        self.bucket_allocs.store(0, Ordering::Relaxed);
    }
}

static SPATIAL_HASH_METRICS: LazyLock<SpatialHashMetrics> = LazyLock::new(SpatialHashMetrics::new);

#[derive(Clone, Copy, Debug, Default)]
pub struct SpatialHashMetricsSnapshot {
    pub total_ns: u64,
    pub total_calls: u64,
    pub recycle_ns: u64,
    pub recycle_calls: u64,
    pub emit_ns: u64,
    pub emit_calls: u64,
    pub entities: u64,
    pub relations: u64,
    pub bucket_lookups: u64,
    pub bucket_hits: u64,
    pub bucket_reuses: u64,
    pub bucket_allocs: u64,
}

pub fn spatial_hash_metrics_snapshot() -> SpatialHashMetricsSnapshot {
    SPATIAL_HASH_METRICS.snapshot()
}

pub fn reset_spatial_hash_metrics() {
    SPATIAL_HASH_METRICS.reset();
}

impl SpatialHashGrid {
    pub fn new(config: SpatialHashConfig) -> Self {
        Self {
            config,
            buckets: HashMap::new(),
            bucket_pool: Vec::new(),
        }
    }

    fn recycle_buckets(&mut self) {
        for (_, mut bucket) in self.buckets.drain() {
            bucket.clear();
            self.bucket_pool.push(bucket);
        }
    }

    fn bucket_mut(&mut self, coord: CellCoord) -> &mut Vec<GridEntry> {
        match self.buckets.entry(coord) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(vacant) => {
                let mut bucket = if let Some(mut reused) = self.bucket_pool.pop() {
                    reused.clear();
                    SPATIAL_HASH_METRICS
                        .bucket_reuses
                        .fetch_add(1, Ordering::Relaxed);
                    reused
                } else {
                    SPATIAL_HASH_METRICS
                        .bucket_allocs
                        .fetch_add(1, Ordering::Relaxed);
                    Vec::with_capacity(32)
                };
                bucket.clear();
                vacant.insert(bucket)
            }
        }
    }

    fn pos_to_cell(&self, x: i32, y: i32) -> CellCoord {
        CellCoord::new(
            x.div_euclid(self.config.cell_size),
            y.div_euclid(self.config.cell_size),
        )
    }

    fn emit_against(
        entry: &GridEntry,
        bucket: &[GridEntry],
        radius_sq: i64,
        buffer: &mut RelationBuffer,
        relation: RelationType,
    ) {
        let start = Instant::now();
        let mut emitted = 0u64;
        for other in bucket {
            if Self::overlap(entry, other, radius_sq) {
                let delta = RelationDelta {
                    dx: entry.x - other.x,
                    dy: entry.y - other.y,
                };
                buffer.push_relation(
                    RelationRecord::new(other.entity, entry.entity, relation, None),
                    &[],
                    Some(delta),
                );
                emitted += 1;
            }
        }
        if emitted > 0 {
            SPATIAL_HASH_METRICS
                .relations
                .fetch_add(emitted, Ordering::Relaxed);
        }
        SPATIAL_HASH_METRICS
            .emit
            .record(start.elapsed().as_nanos() as u64);
    }

    fn process_entry(&mut self, entry: GridEntry, radius_sq: i64, buffer: &mut RelationBuffer) {
        SPATIAL_HASH_METRICS
            .entities
            .fetch_add(1, Ordering::Relaxed);
        {
            SPATIAL_HASH_METRICS
                .bucket_lookups
                .fetch_add(1, Ordering::Relaxed);
            if let Some(bucket) = self.buckets.get(&entry.coord) {
                SPATIAL_HASH_METRICS
                    .bucket_hits
                    .fetch_add(1, Ordering::Relaxed);
                Self::emit_against(&entry, bucket, radius_sq, buffer, self.config.relation);
            }
            for neighbor in entry.coord.neighbors() {
                SPATIAL_HASH_METRICS
                    .bucket_lookups
                    .fetch_add(1, Ordering::Relaxed);
                if let Some(bucket) = self.buckets.get(&neighbor) {
                    SPATIAL_HASH_METRICS
                        .bucket_hits
                        .fetch_add(1, Ordering::Relaxed);
                    Self::emit_against(&entry, bucket, radius_sq, buffer, self.config.relation);
                }
            }
        }

        self.bucket_mut(entry.coord).push(entry);
    }

    #[inline]
    fn overlap(a: &GridEntry, b: &GridEntry, radius_sq: i64) -> bool {
        let dx = (a.x - b.x) as i64;
        let dy = (a.y - b.y) as i64;
        dx * dx + dy * dy <= radius_sq
    }
}

impl RelationAccelerator for SpatialHashGrid {
    fn relation_type(&self) -> RelationType {
        self.config.relation
    }

    fn rebuild(&mut self, world: &World, buffer: &mut RelationBuffer) {
        let total_start = Instant::now();
        let recycle_start = Instant::now();
        self.recycle_buckets();
        SPATIAL_HASH_METRICS
            .recycle
            .record(recycle_start.elapsed().as_nanos() as u64);

        let radius_sq = (self.config.radius as i64) * (self.config.radius as i64);
        let archetypes = world.archetypes_with(self.config.component_id);
        for &arch in archetypes {
            let storage = match world.storage(arch) {
                Some(storage) => storage,
                None => continue,
            };
            let column = match storage.column(self.config.component_id) {
                Ok(col) => col,
                Err(_) => continue,
            };
            let stride = column.stride();
            for page_idx in 0..column.page_count() {
                let range = column.page_range(page_idx);
                if range.is_empty() {
                    continue;
                }
                let entity_ids = match storage.entity_ids_slice(range.clone()) {
                    Ok(ids) => ids,
                    Err(_) => continue,
                };
                let bytes = match column.slice_read(range.clone()) {
                    Ok(slice) => slice,
                    Err(_) => continue,
                };
                for (row, &entity_id) in entity_ids.iter().enumerate() {
                    let base = row * stride;
                    if base + 8 > bytes.len() {
                        break;
                    }
                    let x = i32::from_ne_bytes(bytes[base..base + 4].try_into().unwrap());
                    let y = i32::from_ne_bytes(bytes[base + 4..base + 8].try_into().unwrap());
                    let entity = match world.resolve_entity(entity_id) {
                        Some(entity) => entity,
                        None => continue,
                    };
                    let coord = self.pos_to_cell(x, y);
                    let entry = GridEntry {
                        entity,
                        coord,
                        x,
                        y,
                    };
                    self.process_entry(entry, radius_sq, buffer);
                }
            }
        }

        SPATIAL_HASH_METRICS
            .total
            .record(total_start.elapsed().as_nanos() as u64);
    }
}
