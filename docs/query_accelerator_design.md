# Generic Query Accelerator System Design

## 1. Core Idea: Query Accelerators (QAs)
A **QA** is a read-only structure (per tick) built from the **current** world buffer and consumed by systems to answer queries without per-entity searches/allocations.

Each QA implements:
- **Build/Update**: `build(view)` → produces a compact, immutable index for the tick.
- **Query (single & batch)**: `query(ctx, key)`, `batch_query(ctx, keys[])` (vectorized).
- **Pair enumeration (optional)**: `for_pairs(ctx, fun)` — emits candidate pairs once per tick (e.g., for collisions), so systems don’t repeat the same neighborhood query N times.

### Example QA Types
- **Uniform Spatial Grid (Cell List)** — near-neighbor queries; predictable, SIMD-friendly; best general default.
- **Sweep-and-Prune (SAP)** — broadphase for AABBs; great when velocity is mostly axis-aligned / coherent.
- **BVH** — for sparse scenes/meshes; slower to build but tighter culling.
- **CSR Graph** — adjacency (social, nav, constraints).
- **Inverted Index / Radix Bins** — attributes & exact/range lookups (e.g., “Faction==Orc”, “Health<20%”).
- **Event Index** — per-tick append-only queues mapped to subscribers (topic → entity spans).

All QAs must be **deterministic**: fixed cell ordering, stable ID sorting, and canonical tie-breaks.

---

## 2. Schema-Driven Integration
Components opt into query “views”:
```yaml
Position:
  query_views:
    - type: spatial_grid
      key: position
      radius_hint: 2..64
Collision:
  query_views:
    - type: sweep_prune
      key: aabb
Faction:
  query_views:
    - type: inverted_index
      key: faction_id
```
The engine wires these to concrete QAs. At runtime, when a system requests a `QueryHandle<PositionNeighborhood>`, it receives the QA’s **read-only view** for the current tick.

---

## 3. Memory Model (No Hot-Path Allocations)
- **Double-buffered world + QA buffers** (`current` / `next`).
- **Scratch arenas per worker** for transient iterators.
- **SoA + CSR-style layouts**; all QA outputs are spans into preallocated blocks.
- **Overflow tracking** (no blocking allocs, used for tuning).

---

## 4. The Workhorse: Cell List + CSR Neighbor Graph
This avoids per-entity re-querying.

### Build Phase
1. Choose **cell size** ≈ max query radius.
2. Compute **cell key** (integer grids or Morton codes).
3. **Count pass** → number of occupants per cell.
4. **Prefix sum** → compute `cells_index[]` (cell → [begin,end)).
5. **Scatter pass** → write `entity_ids` contiguously per cell.
6. **Neighbor CSR** → for each cell, inspect 26 neighbors and fill adjacency (`nbr_offsets[]`, `nbr_ids[]`).

### System Usage
```cpp
for (auto batch : world.iter_archetype<Position, Velocity, Collision>()) {
  auto neigh = qa.neighbor_view();
  for (Entity e : batch.entities) {
    for (Entity n : neigh.span(e)) {
      // resolve interactions
    }
  }
}
```
Zero allocations; cache-friendly; deterministic.

---

## 5. Batch Queries & Tiling
To maximize SIMD and cache locality:
- **Cell-tiled iteration**: `world.iter_cells(view)` yields cell-local batches.
- **Batch query API**: `qa.batch_span(cell_id)` returns contiguous IDs.
- **for_pairs(cell_id, fun)**: process all pairs in a cell once (symmetric operations).

This reuses the query result per tile rather than per entity.

---

## 6. Updates & Mutability
- Archetypes are fixed (immutable composition).
- Movement changes component values only.
- **Rebuild QAs per tick (O(N))**, or **incremental updates** for smaller motions.
- Shard large worlds to update touched regions.

---

## 7. Other Query Accelerators
- **SAP**: Three sorted arrays (`minX`, `minY`, `minZ`); good for variable radius.
- **BVH**: LBVH via Morton codes; used for raycasts or sparse scenes.
- **Graph QA**: Maintain `adj_offsets[]`, `adj_ids[]`; O(1) neighbor access.
- **Inverted Index**: Map discrete attributes to `[begin,end)` spans.
- **Event Index**: Append-only topic queues cleared per tick.

---

## 8. Scheduling & Threads
1. Freeze `current` buffer.
2. Build/refresh QAs in parallel.
3. Run systems using **const views** + QAs.
4. Write to `next` buffer.

Each worker has isolated arenas; avoid false sharing and maintain cell affinity for cache reuse.

---

## 9. Determinism
- Fixed-point arithmetic everywhere.
- Stable entity ordering within cells.
- Canonical tie-breakers for sort collisions.
- No randomized hashing.

---

## 10. Minimal API Surface
### Engine Side
```cpp
register_query_view(Position, SpatialGrid{/*params*/});
register_query_view(Collision, SweepAndPrune{/*params*/});
register_query_view(Faction, InvertedIndex{/*params*/});
```

### System Side
```cpp
world.system<Position, Velocity, Collision>([&](SysCtx& ctx,
                                               Col<Position> pos,
                                               Col<Velocity> vel,
                                               Col<Collision> col) {
  auto grid = ctx.query<Position, SpatialGrid>();
  for (auto cell : grid.cells()) {
    grid.for_pairs(cell, [&](Entity a, Entity b){
      // collision or flocking
    });
  }
});
```

Per-entity query:
```cpp
for (Entity e : batch.entities) {
  for (Entity n : grid.neighbors(e)) { /* ... */ }
}
```

---

## 11. Preallocation Guidelines
| Structure | Initial Capacity | Notes |
|------------|------------------|-------|
| `cell_entities[]` | `ceil(N * (1 + ε))` | Per entity |
| `cells_index[]` | `num_cells + 1` | Prefix sum |
| `nbr_ids[]` | `N * k_avg` | Neighbor CSR |
| `SAP` | `N`, `N*k_axis` | Per axis overlap buffer |
| `CSR Graph` | Fixed edge count | Dynamic via rate limits |

---

## 12. Why This Meets Goals
| Goal | How Achieved |
|------|---------------|
| High-performance | Linear builds, contiguous memory, SIMD-ready, zero allocs |
| Cross-compatible | Pure data layouts; CPU/GPU portability |
| Simple & opinionated | Single uniform CSR/span model |
| Deterministic | Stable ordering, fixed-point math |
| Massive scale | Linear passes, sharded regions, double-buffering |
