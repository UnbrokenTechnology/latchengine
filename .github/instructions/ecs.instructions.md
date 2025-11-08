```instructions
---
applyTo: "crates/latch_core/src/ecs/**"
---

# ECS Design & Implementation

## Core Architecture

**Components = Bytes with IDs**, not Rust types. This enables TypeScript interop.

```rust
pub type ComponentId = u32;  // NOT TypeId!
HashMap<ComponentId, Column>  // Column = Vec<u8>
```

### Key Types

- **Entity**: 8-byte handle with generation (prevents use-after-free)
- **ComponentId**: u32 (stable across languages, not Rust's TypeId)
- **Archetype**: SoA storage for entities with identical component sets
- **Column**: Vec<u8> with type metadata (size, align)

### Module Responsibilities

- `entity.rs`: Entity handles + metadata (50 lines)
- `component.rs`: Component trait + storage (90 lines)
- `archetype.rs`: Archetype ID + SoA layout (100 lines)
- `world.rs`: Main container + queries (419 lines)
- `macros.rs`: spawn! macro (49 lines)

## Static Component Model (ENFORCED)

**Components cannot be added/removed after entity creation.**

### API Design

```rust
// ✅ CORRECT: All components at spawn
let entity = spawn!(world, Position::default(), Velocity::default());

// ❌ FORBIDDEN: No add_component method exists
// world.add_component(entity, Health { hp: 100 });
```

### Why Static?

1. **No archetype migrations** - entities never move after spawn
2. **Better cache locality** - stable memory layout
3. **Forces good design** - state in fields, not component presence

### Patterns

**State as fields:**
```rust
struct Burnable { is_burning: bool, damage: f32 }  // ✅
struct Burning;  // ❌ Requires add/remove
```

**Optional components:**
```rust
struct Target { entity: Option<Entity> }  // ✅
```

**Temporary effects:**
```rust
struct StatusEffects { effects: Vec<Effect> }  // ✅ Mutate the Vec
```

## Object Pooling Per Archetype

Free list prevents fragmentation:

```rust
pub struct ArchetypeStorage {
    len: usize,
    free: Vec<usize>,  // Reuse despawned slots
    entity_ids: Vec<Option<u64>>,
}
```

- Despawn: O(1) push to free list
- Spawn: O(1) pop (or grow if empty)
- No shifting, no reallocations

## Parallel Iteration

SoA enables safe parallelism:

```rust
world.for_each::<Position>(|pos| {
    pos.x += 1.0;  // Rayon splits slice into chunks
});
```

Each column is `&mut [T]` - exclusive access, no data races.

## Critical Missing Features (Add if needed)

### Generational Indices (HIGH PRIORITY)

```rust
pub struct Entity {
    pub id: u64,
    pub generation: u32,  // ← Add this
    // ...
}

fn free_row(&mut self, idx: usize) {
    self.entity_generations[idx] += 1;  // Invalidate handles
}
```

Prevents use-after-free when entity IDs are reused.

### Alignment Safety (MEDIUM)

Vec<u8> has align=1. Use aligned allocator or debug_assert:

```rust
debug_assert_eq!(col.bytes.as_ptr() as usize % align_of::<T>(), 0);
```

## TypeScript Integration

Components defined in TS register with same ComponentId space:

```rust
// Rust
const POSITION_ID: ComponentId = 1;

// TypeScript
export const Position = { id: 1, size: 12, align: 4 };
```

Use `EntityBuilder::with_raw(cid, bytes)` for TS components.

## Performance

- Spawn: O(1) with pooling
- Query single component: O(A) where A = archetype count (typically <100)
- Iteration: O(E) sequential, O(E/T) parallel
- Memory: 16 bytes overhead per entity + component data
```
