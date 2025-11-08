# Static Component Model

**Last Updated:** November 8, 2025

## Policy

**Components cannot be added or removed after entity creation.**

This is a hard constraint enforced at the API level. The `add_component()` and `remove_component()` methods are hidden from public API and only accessible internally to `spawn!()` macro and `EntityBuilder`.

## Rationale

### Performance

1. **Archetype migrations are expensive**
   - Adding a component changes the entity's archetype
   - Requires allocating new storage and copying all components
   - O(n) operation where n = number of components on the entity

2. **Cache coherency**
   - Dynamic composition scatters entities across archetypes
   - Queries become slower as they iterate more sparse data structures
   - SIMD operations work best on contiguous, homogeneous data

3. **Predictable layout**
   - Static composition means archetype set is known at compile time
   - Memory budgets can be estimated accurately
   - No surprise allocations during gameplay

### Design Benefits

1. **Forces good component design**
   - Components should represent capabilities, not states
   - State lives in component fields, not component presence
   - Example: `Burnable { is_burning: bool }` not `Burning` component

2. **Simplifies reasoning**
   - Entity "shape" never changes
   - No edge cases around mid-frame component addition
   - Systems can assume stable archetype membership

3. **Aligns with determinism**
   - Component layout affects iteration order
   - Dynamic changes would make replay harder to validate
   - Static entities → stable simulation

## Usage

### ✅ Good Patterns

```rust
// Component with state field
struct Burnable {
    is_burning: bool,
    fire_damage_per_tick: f32,
}

// Spawn with all components
let entity = spawn!(world,
    Position::default(),
    Health { current: 100, max: 100 },
    Burnable { is_burning: false, fire_damage_per_tick: 5.0 }
);

// Toggle state via component field
world.get_mut::<Burnable>(entity).unwrap().is_burning = true;

// Or using builder pattern
let entity = world.entity()
    .with(Position::default())
    .with(Health { current: 100, max: 100 })
    .with(Burnable { is_burning: false, fire_damage_per_tick: 5.0 })
    .build();
```

### ❌ Bad Patterns

```rust
// DON'T: Try to add components after spawn
let entity = spawn!(world, Position::default());
world.add_component(entity, Health { current: 100, max: 100 }); // ❌ Hidden from API

// DON'T: Represent state as component presence
struct Burning;  // ❌ Bad - requires add/remove for state changes
```

## Edge Cases

### Optional Components

If a component is truly optional, use `Option<T>` within another component:

```rust
struct CombatState {
    target: Option<Entity>,  // Some when in combat, None otherwise
}
```

Or use sentinel values:

```rust
struct Target {
    entity: Entity,  // Use Entity::NONE or similar sentinel
}

impl Default for Target {
    fn default() -> Self {
        Self { entity: Entity::NONE }
    }
}
```

### Component Archetypes

If entities genuinely need different "shapes" (e.g., Player vs NPC vs Projectile), spawn them as separate archetypes:

```rust
// Player archetype
let player = spawn!(world,
    Position::default(),
    Health::default(),
    Inventory::default(),
    PlayerController::default()
);

// NPC archetype
let npc = spawn!(world,
    Position::default(),
    Health::default(),
    AIController::default()
);

// Projectile archetype  
let projectile = spawn!(world,
    Position::default(),
    Velocity::default(),
    Damage { amount: 50 }
);
```

### Temporary Effects

For temporary effects (buffs, debuffs, status ailments), use a container component:

```rust
struct StatusEffects {
    effects: Vec<StatusEffect>,  // Add/remove from this Vec, not from entity
}

struct StatusEffect {
    kind: StatusKind,
    duration_remaining: f32,
}

enum StatusKind {
    Burning { damage_per_tick: f32 },
    Frozen { movement_penalty: f32 },
    Poisoned { damage_per_tick: f32, tick_interval: f32 },
}
```

## Implementation Details

### API Enforcement

- `World::add_component()` marked `#[doc(hidden)] pub` (only for macro access)
- `World::add_component2/3()` marked `#[doc(hidden)] pub` (only for macro access)
- No `remove_component()` method exists
- Public API only exposes:
  - `spawn!()` macro
  - `World::entity()` → `EntityBuilder`
  - `EntityBuilder::with()` → `EntityBuilder`
  - `EntityBuilder::build()` → `Entity`

### Macro Implementation

The `spawn!()` macro uses `add_component*()` methods internally, but these are hidden from user code:

```rust
#[macro_export]
macro_rules! spawn {
    // Single component
    ($world:expr, $c1:expr) => {{
        let entity = $world.spawn();
        $world.add_component(entity, $c1);  // ← Hidden method
        entity
    }};
    // ... more cases
}
```

## Future Considerations

### Prefab/Template System

If we add a prefab system, it should also only support static composition:

```rust
// Define template with fixed components
let player_template = Template::new()
    .with::<Position>()
    .with::<Health>()
    .with::<Inventory>();

// Spawn from template (still static)
let player = world.spawn_from_template(&player_template, |builder| {
    builder
        .set(Position::default())
        .set(Health { current: 100, max: 100 })
        .set(Inventory::empty())
});
```

### Editor Support

The editor should:
1. Show entity archetype at spawn time
2. Warn if developer tries to "add component" to existing entity
3. Suggest using component fields for dynamic behavior
4. Provide archetype browser (group entities by component signature)

## References

- ECS implementation: `crates/latch_core/src/ecs.rs`
- Usage example: `crates/latch_runtime/examples/poc2_moving_triangles.rs`
- Related: `docs/architecture.md` (section on determinism)
