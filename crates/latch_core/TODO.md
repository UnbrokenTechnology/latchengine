# latch_core TODO

- [ ] Bring `time` module in line with ECS design (50 Hz tick, deterministic budgeting, remove 60 Hz assumptions).
- [ ] Tighten `PagedPool` API error propagation and document invariants for tiled iteration.
- [x] Rebuild `ecs::storage::Column` around the new paged double-buffer abstraction (Result-returning tile access, lockstep frees).
- [x] Reimplement `ecs::storage::ArchetypeStorage` with page-sized column planning and deterministic bulk despawn flow.
- [x] Introduce `ArchetypePlan` with cache-aware page sizing based on `latch_env::memory::Memory::detect()`.
- [x] Build the runtime storage wrapper on top of the new planning API (columns + entity sidecar).
- [ ] Replace the raw `ComponentColumn` byte pages with an adapter that leverages the typed `Column<T>` API once world integration clarifies the needs.
- [x] Replace the legacy world/builder stack with a generational entity index and batched despawn queues wired to the new storage layer.
- [ ] Integrate scheduler-facing archetype tiling and system plan generation on top of the new world/runtime plumbing.
- [ ] Design tile iteration helpers (erosion/batching API) to satisfy L1/L2 cache goals and integrate with scheduler jobs.
