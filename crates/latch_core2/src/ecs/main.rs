// main.rs
mod component;
mod archetype;
mod entity;
mod world;

use component::{Component, ComponentMeta, register_component};
use entity::EntityBuilder;
use world::World;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct Stats { str_: u16, dex: u16, int_: u16 }
define_component!(Stats, 1, "Stats");

fn health_bytes(hp: f32, max_hp: f32) -> Vec<u8> {
    // Health layout (TS-defined):
    // struct Health { hp: f32, max_hp: f32 }  // total size = 8, align = 4
    let mut v = Vec::with_capacity(8);
    v.extend_from_slice(&hp.to_ne_bytes());
    v.extend_from_slice(&max_hp.to_ne_bytes());
    v
}

fn main() {
    // Register the TS-defined Health component layout (as if parsed from schema).
    // id = 2, name = "Health", size = 8 (hp: f32, max_hp: f32), align = 4
    register_component(ComponentMeta {
        id: 2,
        name: "Health".into(),
        size: 8,
        align: 4,
    });

    // Create the world and spawn entities whose archetype is [Stats, Health]
    let mut world = World::new();

    let e1 = world.spawn(
        EntityBuilder::new()
            .with(Stats { str_: 10, dex: 8, int_: 12 })
            // TS-defined Health supplied as raw bytes (hp=100.0, max_hp=120.0)
            .with_raw(2, health_bytes(100.0, 120.0), /*expected_size=*/8)
    );

    let e2 = world.spawn(
        EntityBuilder::new()
            .with(Stats { str_: 20, dex: 5, int_: 6 })
            // TS-defined Health (hp=50.0, max_hp=80.0)
            .with_raw(2, health_bytes(50.0, 80.0), 8)
    );

    // Example “system”: parallel over Stats only (Health is TS-defined, so we won’t type it here)
    world.for_each::<Stats, _>(|s| {
        if s.str_ > 15 {
            // e.g., buff dex for high strength
            s.dex = s.dex.saturating_add(1);
        }
    });

    // Read all "Stats" components back to verify
    for slice in world.all_components::<Stats>() {
        for rb in slice {
            println!("Stats: {:?}", rb);
        }
    }

    // Read back our particular entities
    println!("e1: {:?}", e1);
    println!("e2: {:?}", e2);

    // And their Stats:
    if let Some(s) = world.get_component::<Stats>(e1) {
        println!("e1 Stats: {:?}", s);
    }
    if let Some(s) = world.get_component::<Stats>(e2) {
        println!("e2 Stats: {:?}", s);
    }

    // For fun, let's dump all components of e1 as raw bytes
    let comps = world.components_of_entity_raw(e1);
    for c in comps {
        println!("Component ID: {}, bytes: {:?}", c.cid, c.bytes);
    }

    // Note: Health is TS-defined. In Rust you’d normally expose the raw column bytes to JS/TS
    // and create a Float32Array view there. If you want to *inspect* in Rust for debugging,
    // you could add a helper in `World` to get raw bytes by component id and decode locally.
}