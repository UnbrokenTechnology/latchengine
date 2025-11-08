# ECS Static Component Model Implementation

**Date:** November 8, 2024  
**Status:** ✅ Complete

## Summary

Refactored the ECS to enforce the **static component model**: components can only be added during entity construction, never at runtime. This aligns with the engine's design principle documented in `docs/static-component-model.md`.

## Changes Made

### Removed Methods
- ❌ `World::add_component()` - Runtime component addition (disallowed)
- ❌ `World::add_component2()` - Old atomic 2-component helper
- ❌ `World::add_component3()` - Old atomic 3-component helper  
- ❌ `EntityBuilder` - Fluent API (violated static component principle)

### Added Methods
- ✅ `World::__spawn1<T>()` - Spawn with 1 component
- ✅ `World::__spawn2<T1, T2>()` - Spawn with 2 components
- ✅ `World::__spawn3<T1, T2, T3>()` - Spawn with 3 components
- ✅ `World::__spawn4<T1, T2, T3, T4>()` - Spawn with 4 components
- ✅ `World::__spawn_with_components()` - Internal helper

### Macro Simplification

**Before** (old approach - archetype migrations):
```rust
macro_rules! spawn {
    ($world:expr, $c1:expr, $c2:expr) => {{
        let entity = $world.spawn();
        $world.add_component2(entity, $c1, $c2); // Separate call, could migrate
        entity
    }};
}
```

**After** (new approach - single archetype placement):
```rust
macro_rules! spawn {
    ($world:expr, $c1:expr, $c2:expr) => {{
        $world.__spawn2($c1, $c2) // All components added atomically
    }};
}
```

## Design Benefits

### 1. **Archetype Calculated Once**
- Old: Spawn entity → Add component 1 → Maybe migrate → Add component 2 → Maybe migrate
- New: Calculate archetype from ALL components → Place entity in correct archetype ONCE

### 2. **No Runtime Migrations**
Entities never move between archetypes after creation. This means:
- ✅ No hash lookups for archetype transitions
- ✅ No component data copying
- ✅ No storage reallocation
- ✅ Predictable cache behavior

### 3. **Cleaner API Surface**
- Removed 3 public methods (`add_component*`)
- Removed EntityBuilder (28 lines)
- Reduced world.rs from 419 → 370 lines

### 4. **Enforced Design Principle**
The static component model is now **enforced by the API**:

```rust
// ❌ IMPOSSIBLE - no add_component method
world.add_component(entity, Burning);

// ✅ CORRECT - boolean field for dynamic state
struct Flammable {
    is_burning: bool,
    heat: f32,
}
let entity = spawn!(world, Flammable { is_burning: false, heat: 0.0 });
```

## Implementation Details

### Archetype Placement Flow

1. **Calculate TypeIds**: `[TypeId::of::<T1>(), TypeId::of::<T2>(), ...]`
2. **Hash to ArchetypeId**: `ArchetypeId::from_types(&type_ids)`
3. **Ensure archetype exists**: Create if first time seeing this component combo
4. **Ensure storage exists**: Add `ComponentVec<T>` for each component type
5. **Add entity**: `archetype.push_entity(entity)` returns index
6. **Push components**: `storage.push(component)` for each type
7. **Update metadata**: Set entity's archetype ID and index

All steps happen in `__spawn_with_components()` helper, called by `__spawn1/2/3/4()`.

### Macro Expansion Example

```rust
spawn!(world, Position { x: 0.0 }, Velocity { x: 1.0 })
```

Expands to:

```rust
world.__spawn2(Position { x: 0.0 }, Velocity { x: 1.0 })
```

Which calls:

```rust
fn __spawn2<T1, T2>(&mut self, c1: T1, c2: T2) -> Entity {
    let entity = self.spawn(); // Just allocate entity handle
    let type_ids = vec![TypeId::of::<T1>(), TypeId::of::<T2>()];
    
    self.__spawn_with_components(entity, type_ids, |archetype| {
        // Ensure storage
        if !archetype.components.contains_key(&TypeId::of::<T1>()) {
            archetype.add_storage::<T1>();
        }
        if !archetype.components.contains_key(&TypeId::of::<T2>()) {
            archetype.add_storage::<T2>();
        }
        // Add components
        archetype.get_storage_mut::<T1>().unwrap().push(c1);
        archetype.get_storage_mut::<T2>().unwrap().push(c2);
    });
    
    entity
}
```

## File Sizes

| File | Before | After | Change |
|------|--------|-------|--------|
| world.rs | 419 lines | 370 lines | **-49 lines** |
| macros.rs | 49 lines | 48 lines | -1 line |
| **Total** | **721 lines** | **674 lines** | **-47 lines** |

## Performance Impact

✅ **Faster entity spawning**: One archetype lookup instead of N (where N = component count)  
✅ **Better cache locality**: Entities never move after placement  
✅ **Zero runtime overhead**: All specialization via generics (monomorphization)

## Testing

✅ `cargo test -p latch_core` - All tests pass  
✅ `cargo build --example poc2_moving_triangles` - Builds successfully  
✅ Zero compilation warnings  

## Next Steps

With the static component model fully enforced:
1. ✅ ECS modularization (DONE)
2. ✅ Static component enforcement (DONE)
3. ⏭️ On-screen metrics overlay
4. ⏭️ Rendering performance investigation
5. ⏭️ PoC 3: Rust ↔ TypeScript FFI
