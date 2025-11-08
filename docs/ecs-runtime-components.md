# Runtime Component System — Design Insights

**Date:** November 8, 2025  
**Context:** Analysis of `latch_core2` ECS implementation  
**Author:** Lessons learned from building a TypeScript-compatible ECS

---

## Executive Summary

This document captures critical architectural insights from the `latch_core2` ECS implementation. The core innovation: **components are bytes with IDs**, not Rust types. This enables seamless interop between Rust-defined and TypeScript-defined components while maintaining zero-cost abstractions for performance-critical paths.

---

## 1. The Fundamental Insight

### Components Are Just Bytes

```rust
// WRONG: Type-based storage (latch_core v1)
HashMap<TypeId, Box<dyn ComponentStorage>>

// RIGHT: ID-based storage (latch_core2)
HashMap<ComponentId, Column>  // Column = Vec<u8>
```

**Key principle:** The ECS doesn't care about types—only component IDs, sizes, and alignments. The type system is an **optional ergonomics layer** on top of raw byte storage.

### Why This Matters

1. **TypeScript Compatibility:** Components defined in JSON/TS can register at runtime with the same `ComponentId` space
2. **Language Agnostic:** Storage layer has no Rust-specific concepts (no `TypeId`, no generics)
3. **Zero Overhead:** Typed access via unsafe transmute—no runtime dispatch
4. **Deterministic:** Component layout controlled by IDs, not Rust's opaque `TypeId` hashing

---

## 2. Architecture Overview

### Component Registration

```rust
pub type ComponentId = u32;  // NOT TypeId!

pub struct ComponentMeta {
    pub id: ComponentId,
    pub name: &'static str,
    pub size: usize,
    pub align: usize,
}

// Global registry bridges Rust and TypeScript
static REGISTRY: Lazy<RwLock<HashMap<ComponentId, ComponentMeta>>> = ...;
```

**Sources of ComponentId:**
- **Rust:** `const ID: ComponentId = 1;` (compile-time constant)
- **TypeScript:** Hash of component name or assigned by tooling
- **JSON:** Schema-driven ID assignment during asset compilation

### Storage Model

```rust
pub struct ArchetypeStorage {
    archetype: Archetype,
    columns: HashMap<ComponentId, Column>,  // One Vec<u8> per component type
    len: usize,
    free: Vec<usize>,  // Object pool for despawned entities
    entity_ids: Vec<Option<u64>>,
}

struct Column {
    elem_size: usize,
    elem_align: usize,
    bytes: Vec<u8>,  // Tightly packed POD elements
}
```

**SoA Layout:**
- One `Column` per component type in the archetype
- `bytes.len() = num_entities * elem_size`
- Typed access via `unsafe { slice::from_raw_parts(ptr as *const T, len) }`

### Archetype Calculation

```rust
impl Archetype {
    pub fn from_components(cids: &[ComponentId]) -> Self {
        let mut sorted = cids.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        
        let mut hasher = DefaultHasher::new();
        for &id in &sorted {
            hasher.write_u32(id);
        }
        
        Self { id: hasher.finish(), components: sorted }
    }
}
```

**Key property:** Archetype ID is deterministic hash of **sorted** component IDs. Same components = same archetype, regardless of insertion order.

---

## 3. The EntityBuilder Pattern

### Why Not Direct Spawn?

**Agent's failed approach (latch_core v1):**
```rust
// Tried to create spawn!(e, Health { hp: 100 }, Position { x: 0.0 })
// Required macros to generate __spawn1, __spawn2, __spawn3, __spawn4...
// Broke down with arbitrary component counts
```

**The problem:** Can't calculate archetype until you know **all** components. Macro couldn't handle unbounded argument lists without generating infinite methods.

### EntityBuilder Solution

```rust
pub struct EntityBuilder {
    components: HashMap<ComponentId, Vec<u8>>,
}

impl EntityBuilder {
    // Rust typed components
    pub fn with<T: Component>(mut self, comp: T) -> Self {
        let bytes = unsafe {
            std::slice::from_raw_parts(&comp as *const T as *const u8, size_of::<T>())
        };
        self.components.insert(T::ID, bytes.to_vec());
        std::mem::forget(comp);  // Prevent double-drop
        self
    }
    
    // TypeScript/JSON raw components
    pub fn with_raw(mut self, cid: ComponentId, bytes: Vec<u8>) -> Self {
        self.components.insert(cid, bytes);
        self
    }
    
    // Calculate archetype ONCE after all components added
    pub fn archetype(&self) -> Archetype {
        let cids: Vec<_> = self.components.keys().copied().collect();
        Archetype::from_components(&cids)
    }
}
```

**Usage:**
```rust
// Rust components
let entity = world.spawn(
    EntityBuilder::new()
        .with(Stats { level: 5 })
        .with(Position { x: 10.0, y: 20.0 })
);

// Mixed Rust + TypeScript
let entity = world.spawn(
    EntityBuilder::new()
        .with(Stats { level: 5 })           // Rust (ID=1)
        .with_raw(2, health_bytes.clone())  // TypeScript (ID=2)
);
```

**Benefits:**
1. Clean API—no macro complexity
2. Archetype calculated once after all components collected
3. TypeScript components use same code path (`with_raw`)
4. No performance penalty—builder consumed by `spawn()`

---

## 4. Object Pooling Per Archetype

### The Problem: Fragmentation

Naive approach: `Vec::remove()` when despawning creates holes, requires shifting all subsequent elements.

### The Solution: Free List

```rust
pub struct ArchetypeStorage {
    len: usize,           // Total slots (alive + dead)
    free: Vec<usize>,     // Indices of despawned entities (pool)
    entity_ids: Vec<Option<u64>>,
}

fn alloc_row(&mut self) -> usize {
    if let Some(idx) = self.free.pop() {
        self.entity_ids[idx] = None;
        idx  // Reuse existing slot
    } else {
        let row = self.len;
        self.len += 1;
        for col in self.columns.values_mut() {
            col.grow_one();  // Extend Vec<u8> by elem_size
        }
        self.entity_ids.push(None);
        row  // Allocate new slot
    }
}

fn free_row(&mut self, idx: usize) {
    self.entity_ids[idx] = None;  // Mark as dead
    self.free.push(idx);           // Return to pool
}
```

**Performance:**
- Despawn: O(1) push to free list
- Spawn: O(1) pop from free list (or O(k) grow if empty, k = num columns)
- No shifting, no fragmentation
- Columns may contain dead entities—iteration skips via `entity_ids[row].is_some()`

**Trade-off:** Memory not released until archetype destroyed. Acceptable for games with stable entity counts per archetype.

---

## 5. Reverse Index for Fast Queries

### The comp_index HashMap

```rust
pub struct World {
    storages: HashMap<ArchetypeId, ArchetypeStorage>,
    comp_index: HashMap<ComponentId, Vec<ArchetypeId>>,  // Reverse lookup
}
```

**Population:** When a new archetype is created:
```rust
for &cid in &archetype.components {
    self.comp_index.entry(cid).or_default().push(archetype.id);
}
```

**Query Pattern:**
```rust
// "Which archetypes contain Stats?"
let archetypes = &world.comp_index[&Stats::ID];

// "Which archetypes contain BOTH Stats AND Health?"
let stats_archs = &world.comp_index[&Stats::ID];
let health_archs = &world.comp_index[&Health::ID];
let intersection: Vec<_> = stats_archs.iter()
    .filter(|a| health_archs.contains(a))
    .copied()
    .collect();
```

**Complexity:**
- Build: O(A×C) where A=archetypes, C=avg components per archetype
- Query single component: O(1) lookup
- Query N components: O(A×N) intersection (A typically small, <100)

**Why query2<T1, T2> Is Optional:**

```rust
// Explicit (using comp_index directly)
for &arch in &world.comp_index[&Stats::ID] {
    if world.comp_index[&Health::ID].contains(&arch) {
        // Process archetype with both Stats and Health
    }
}

// Sugar (query2 helper)
for (stats, health) in world.query2::<Stats, Health>() {
    // Zipped slices from intersecting archetypes
}
```

The helper is pure ergonomics—no performance difference. Decision: **defer until usage patterns emerge.**

---

## 6. Parallel Iteration with Rayon

### Safe Parallelism via SoA

```rust
pub fn for_each<T, F>(&mut self, f: F)
where
    T: Component + Send,
    F: Fn(&mut T) + Sync + Send,
{
    let Some(ids) = self.comp_index.get(&T::ID) else { return; };
    
    for &arch_id in ids {
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
```

**Safety guarantees:**
1. Each archetype processed sequentially (one `get_mut` at a time)
2. Within archetype, column is `&mut [T]`—exclusive access
3. Rayon splits slice into non-overlapping chunks—no data races
4. Chunk size (1024) balances parallelism overhead vs. cache locality

**Why SoA Enables This:**
- AoS (Array of Structs): Entities interleaved → borrowck hell
- SoA (Struct of Arrays): Homogeneous slices → trivial parallel split

**Usage:**
```rust
world.for_each::<Stats>(|stats| {
    stats.xp += 1;  // Parallel mutation across all Stats components
});
```

---

## 7. Critical Missing Features

### 7.1 Generational Indices

**Current Entity:**
```rust
pub struct Entity {
    pub id: u64,
    pub archetype: ArchetypeId,
    pub index: usize,
}
```

**Problem:** Despawn entity A → spawn entity B reuses same ID+index → stale handle to A now points to B.

**Solution:**
```rust
pub struct Entity {
    pub id: u64,
    pub generation: u32,  // Increment on despawn
    pub archetype: ArchetypeId,
    pub index: usize,
}

// In ArchetypeStorage:
entity_generations: Vec<u32>,

fn free_row(&mut self, idx: usize) {
    self.entity_generations[idx] += 1;  // Invalidate old handles
    self.free.push(idx);
}

fn get_component<T>(&self, e: Entity) -> Option<&T> {
    if self.entity_generations[e.index] != e.generation {
        return None;  // Stale handle
    }
    // ... rest of access logic
}
```

**Priority:** HIGH—prevents entire class of bugs (dangling entity references).

### 7.2 Alignment Safety in Column

**Current:**
```rust
unsafe fn column_as_slice<T>(&self) -> &[T] {
    let ptr = col.bytes.as_ptr();
    std::slice::from_raw_parts(ptr as *const T, len)
}
```

**Risk:** `Vec<u8>` has align=1. Types with align=4 or 8 may cause UB if misaligned.

**Mitigation:**
```rust
pub fn column_as_slice<T: Component>(&self) -> Option<&[T]> {
    let col = self.columns.get(&T::ID)?;
    debug_assert_eq!(col.bytes.as_ptr() as usize % std::mem::align_of::<T>(), 0);
    // ... rest
}
```

**Proper fix:** Use aligned allocator (e.g., `aligned_vec` crate) or over-align `Vec<u8>` to max component alignment.

**Priority:** MEDIUM—works in practice (allocator usually over-aligns), but UB is UB.

### 7.3 Component Migration (Intentionally Omitted)

**Decision:** Static component model = entities never change archetypes.

**If needed later:**
- Despawn from old archetype → return to pool
- Spawn in new archetype with copied/moved components
- Update entity handle's archetype ID

**Defer until gameplay requirements demand it.**

---

## 8. Key Learnings (What Went Wrong Initially)

### Mistake 1: Using Rust's TypeId

```rust
// WRONG
HashMap<TypeId, Box<dyn ComponentStorage>>
```

**Problem:** `TypeId` is Rust-specific, opaque, non-deterministic across builds. Can't be shared with TypeScript.

**Fix:** `ComponentId = u32` controlled by developers, stable across languages.

### Mistake 2: Type-Based Spawn Methods

```rust
// WRONG
fn spawn1<T1>(c1: T1) -> Entity { ... }
fn spawn2<T1, T2>(c1: T1, c2: T2) -> Entity { ... }
fn spawn3<T1, T2, T3>(c1: T1, c2: T2, c3: T3) -> Entity { ... }
// ...ad infinitum
```

**Problem:** Can't generate infinite methods. Macros exploded in complexity.

**Fix:** Builder pattern collects components, calculates archetype once.

### Mistake 3: ComponentBundle Trait

```rust
// WRONG
trait ComponentBundle {
    fn archetype(&self) -> ArchetypeId;
    fn write_to(&self, storage: &mut ArchetypeStorage, row: usize);
}

impl<T1, T2> ComponentBundle for (T1, T2) { ... }
impl<T1, T2, T3> ComponentBundle for (T1, T2, T3) { ... }
// ...tuple hell
```

**Problem:** Still requires infinite trait impls. Tuples max out at 12 elements in std.

**Fix:** Builder with `with<T>()` chains—unbounded component counts.

### Mistake 4: Mixing Concerns

**Agent's approach:** Tried to make storage layer generic over `Component` trait.

**Correct separation:**
- **Storage layer:** ComponentId + Vec<u8> (no generics)
- **Access layer:** Unsafe transmute to `&[T]` (typed view)
- **Component trait:** Metadata provider (const ID, size, align)

**Insight:** Generics belong in the **access API**, not the storage.

---

## 9. TypeScript Integration Blueprint

### Component Definition (TypeScript)

```typescript
// game/components/health.ts
export const Health = {
    id: 2,  // Assigned by tooling or hash
    size: 8,
    align: 4,
    fields: {
        current: { type: 'f32', offset: 0 },
        max: { type: 'f32', offset: 4 },
    }
};
```

### Registration at Runtime

```rust
// During engine startup
fn register_typescript_components(json: &str) {
    let schemas: Vec<ComponentSchema> = serde_json::from_str(json)?;
    for schema in schemas {
        register_component_raw(ComponentMeta {
            id: schema.id,
            name: Box::leak(schema.name.into_boxed_str()),
            size: schema.size,
            align: schema.align,
        });
    }
}
```

### Spawning from TypeScript

```rust
// FFI bridge (pseudocode)
#[no_mangle]
pub extern "C" fn ecs_spawn(builder_ptr: *mut EntityBuilder) -> u64 {
    let builder = unsafe { Box::from_raw(builder_ptr) };
    WORLD.lock().spawn(*builder).id
}

#[no_mangle]
pub extern "C" fn builder_add_component(
    builder_ptr: *mut EntityBuilder,
    cid: u32,
    bytes_ptr: *const u8,
    bytes_len: usize,
) {
    let builder = unsafe { &mut *builder_ptr };
    let bytes = unsafe { std::slice::from_raw_parts(bytes_ptr, bytes_len) }.to_vec();
    builder.components.insert(cid, bytes);
}
```

**TypeScript (via FFI):**
```typescript
const builder = ecs_builder_new();
builder_add_component(builder, Health.id, healthBytes);
builder_add_component(builder, Position.id, positionBytes);
const entity = ecs_spawn(builder);
```

**Same code path as Rust—`with_raw()` is the universal interface.**

---

## 10. Performance Characteristics

### Spawn
- Archetype lookup: O(1) hash
- Column allocation (first spawn in archetype): O(C) where C = component count
- Column reuse (pool hit): O(1)
- Component write: O(C × S) where S = avg component size

### Despawn
- O(1) free list push

### Query (Single Component)
- O(1) comp_index lookup → O(A) iteration over archetypes

### Query (Multi-Component)
- O(A × N) intersection, typically <10ms for 100 archetypes × 5 components

### Iteration
- Sequential: O(E) where E = entity count
- Parallel: O(E / T) where T = thread count (ideal with SoA)

### Memory
- Overhead per entity: 16 bytes (entity_ids + free list worst case)
- Per archetype: ~48 bytes + columns
- Dead entities held until archetype destroyed (pool never shrinks)

---

## 11. Comparison: Type-Based vs. ID-Based

| Aspect | Type-Based (latch_core v1) | ID-Based (latch_core2) |
|--------|---------------------------|------------------------|
| **Storage** | `HashMap<TypeId, Box<dyn Trait>>` | `HashMap<ComponentId, Vec<u8>>` |
| **TypeScript** | ❌ Impossible (TypeId opaque) | ✅ `ComponentId = u32` universal |
| **Determinism** | ❌ TypeId non-deterministic | ✅ ID assigned explicitly |
| **Spawn API** | Infinite methods or macros | Builder pattern (unbounded) |
| **Access** | Downcast + borrow split hell | Unsafe transmute (zero-cost) |
| **Parallelism** | Hard (borrow checker fights) | Trivial (SoA slices) |
| **Complexity** | ~500 LOC, trait objects | ~450 LOC, raw pointers |

**Verdict:** ID-based wins on every axis except "type safety." The typed access layer recovers safety where needed.

---

## 12. Actionable Recommendations

### Immediate (Before Production Use)
1. **Add generational indices** (prevent stale handle bugs)
2. **Alignment safety** (debug_assert or aligned allocator)
3. **Tests:** Pool reuse, mixed Rust/TS, parallel mutation

### Short-Term (Ergonomics)
1. **Query helpers:** `query2<T1, T2>()`, `query3<T1, T2, T3>()` as sugar
2. **Component derive macro:** Auto-generate `Component` impl for Rust structs
3. **TypeScript codegen:** JSON schema → TS types + FFI wrappers

### Medium-Term (Optimization)
1. **Chunk allocator:** Allocate columns in 64KB chunks, not per-element
2. **Sparse sets:** For rare components (e.g., <5% of entities)
3. **Change detection:** Bitset per column, track dirty entities
4. **SIMD queries:** Exploit SoA layout for vectorized filters

### Long-Term (Advanced Features)
1. **Hierarchical archetypes:** Parent/child entity relationships
2. **Reactive queries:** Incremental updates on component add/remove (if migration enabled)
3. **Snapshot/replay:** Serialize entire World state for deterministic replay
4. **GPU upload:** Column → SSBO direct memcpy for GPU-driven rendering

---

## 13. References & Further Reading

**Related Docs:**
- `static-component-model.md` — Rationale for no runtime migration
- `ecs-modularization.md` — File structure and module breakdown
- Project plan § 3.1 — ECS architecture overview

**External Inspirations:**
- EnTT (C++): Type-safe sparse sets + SoA
- bevy_ecs (Rust): Archetype-based with parallel scheduling
- flecs (C): Entity relationships + query DSL
- Our Machinery's blog: "Data-Oriented ECS" series

**Key Papers:**
- "Data-Oriented Design" (Richard Fabian)
- "Understanding Component-Entity Systems" (T-Machine blog)

---

## Conclusion

The `latch_core2` ECS is **not** a Rust ECS. It's a **runtime-defined, language-agnostic component system** that happens to be implemented in Rust. This distinction unlocks:

- ✅ TypeScript components as first-class citizens
- ✅ Deterministic replays (stable IDs, no TypeId hashing)
- ✅ Zero-cost abstractions (unsafe transmute to types when needed)
- ✅ Trivial parallelism (SoA slices, no borrow checker fights)

**The lesson:** When building cross-language systems, **don't let the host language's type system leak into your data model.** Components are bytes. Everything else is convenience.
