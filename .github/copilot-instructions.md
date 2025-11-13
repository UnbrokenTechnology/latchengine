Latch Engine — Core Architecture Guide

A Rust-first, scripting language compatiable (Lua, JavaScript, etc), cross-platform game engine focused on fast iteration, deterministic simulation, and universal compatibility.

## Core Principles

- **Cross-platform dev tools**: Windows, macOS, Linux with no compromises
- **Ultra-fast iteration**: Hot reload, instant script turnaround
- **Ship anywhere**: Desktop, mobile, web, consoles from one codebase
- **60+ FPS on toasters**: Auto-scaling + fallbacks for low-spec hardware
- **Deterministic by default**: Fixed tick, seeded RNG, replay-friendly
- **Git built-in**: Git + LFS + GCM integrated into editor
- **MMO-ready**: Same code for solo → MMO via distributed authority
- **Rust + Scripts**: Engine in Rust, gameplay in scripts, Rust for hot paths
- **Quake 3 performance**: Low-poly games run as fast on equivalent hardware, including CPU fallback

**Non-goals**: Cutting-edge GPU features (ray tracing, custom shaders), translation layers (Wine)

## Architecture

### Crate Responsibilities

| Crate | Purpose |
|-------|---------|
| `latch_core` | ECS (archetype-based, static components), jobs, fixed timestep, deterministic math |
| `latch_render` | Backend abstraction, strategy selection, auto-scaler, window management |
| `latch_net` | Authority (cells/bubbles), gossip/Raft, rollback networking, replication |
| `latch_script` | QuickJS/WASM runtime, FFI layer (handle-based API) |
| `latch_asset` | glTF/KTX2 import, asset pipeline, converters |
| `latch_audio` | Playback, mixing, spatial audio |
| `latch_services` | Platform abstraction (save, settings, input, telemetry, achievements) |
| `latch_runtime` | Minimal game player binary |
| `latch_editor` | Editor GUI (egui/ImGui) |

### Scripting Model

- **Dev mode**: Scripts → interpreter (instant hot reload)
- **Ship mode**: Scripts → compiled (identical FFI)
- **FFI contract**: As best as each language will allow, scripts should be given direct access to memory to avoid unnecessary copying. For example, if using JavaScript/TypeScript, we can use `ArrayBuffer` and `DataView` to read/write component data directly from Rust's memory.

### Memory Model

- Rust owns all memory
- Dev (interpreted): GC allowed, per-frame budget tracked
- Ship (compiled): Linear memory, arena allocators provided
- Determinism: Fixed timestep, seeded RNG, no wall-clock/IO in gameplay

## Rendering (See: `.github/instructions/rendering.instructions.md`)

**Universal floor**: Every feature works on all targets (GPU or CPU fallback).

- Capability probes → strategy selection → auto-scaling
- GPU backends: D3D9/11, GL2.1-3.3, Metal 2+, WebGL 1/2, console SDKs
- Software rasterizer: Quake 3 target (5-10k tris @ 60 FPS, SIMD optimized)
- Effect IR: Backend-neutral → compiles to shaders + Rust functions
- Visibility: Frustum culling (always), PVS for static scenes, occlusion queries (GPU)

## ECS (See: `.github/instructions/ecs.instructions.md`)

**Static component model**: Components cannot be added/removed after entity creation.

- Entity: 8-byte handle (ID) with generation
- ComponentId: u32 (stable across Rust/TypeScript, not TypeId)
- Archetype: SoA storage for entities with identical component sets
- Object pooling per archetype (free list prevents fragmentation)
- Parallel iteration safe (SoA enables &mut [T] per column)

## Networking (See: `.github/instructions/networking.instructions.md`)

**Server-authoritative rollback networking** with distributed world authority.

- Simulation: 50 Hz fixed tick, 2-tick input buffer, 8-tick rollback
- Authority: Cells (spatial regions) + Combat Bubbles (temporary combat instances)
- Replication: Delta compression, interest management, lag compensation
- Self-organizing servers: Gossip (SWIM) + Raft leader election + consistent hashing
- Determinism: Seeded RNG, no wall-clock, stable iteration, platform-stable math

## Asset Pipeline

- **Gold standard**: glTF 2.0 + KTX2 textures → engine binaries
- **Import-only**: FBX/COLLADA/OBJ via sandboxed converters
- **Audio**: WAV/FLAC → Ogg/ADPCM
- **Blender**: First-party add-on with validation and one-click export
- **Deterministic**: Out-of-process converters, hashed manifests, incremental re-import

## Services Layer

Platform abstraction with single API:
- **Save**: Slots + cloud sync (Steam/PSN/Switch/filesystem)
- **Telemetry**: Privacy-gated, buffered, offline queue
- **Entitlements/Achievements**: Unified API for Steam/Xbox/PSN/Epic
- **Settings**: Audio/video/input with per-platform persistence
- **Input**: Device abstraction, remapping, recording for replays
- **Net**: Sockets/HTTP with probes and relay fallbacks
- **Mods**: Desktop-only plugins (sandboxed, capability-gated)

## Build & Packaging

- **Dev builds**: Dynamic linking, hot reload, interpreter, shader cache
- **Ship builds**: Static linking, LTO, baked pipelines, compiled scripts
- **Packaging**: Pre-built runtimes from CI → developer bundles scripts + assets → distributable
- **CI**: GitHub Actions for runtime binaries, compat checker, replay tests, budget enforcement

## Editor & Tooling

- **Editor**: Native Rust (egui/ImGui), scene graph, asset browser, profiler, replay controls
- **VS Code**: Portable Code-OSS + Extension Pack (LSP, debugger, Git integration)
- **Git**: Bundled Git + LFS + GCM, one-click OAuth, auto LFS patterns, asset-aware diffs

## Console & Platform Support

- No SDK redistribution
- Console bridges: Build presets and adapters activate when developer installs SDK locally
- Services layer hides platform restrictions (one mental model)

## Security

- Script sandbox with capability tokens
- Mod sandbox: Desktop-only, script only, no dlopen
- Telemetry: Explicit opt-in, PII-safe
- Server: mTLS between nodes, server-authoritative, state-hash validation

## Quality Gates

- **Compatibility enforcement**: Editor hides disallowed features, CI fails on violations
- **Performance budgets**: Per-frame allocations, draw calls, texture memory tracked
- **Testing**: Golden replay tests, soak tests for GC, platform adapter smoke tests

## Default Components

We will provide a set of built-in components for common use cases, such as:
- `Position`: x, y, z coordinates
- `Rotation`: quaternion or Euler angles
- `Scale`: uniform or non-uniform scaling
- `Velocity`: linear velocity
- `AngularVelocity`: rotational speed
- `Mesh`: reference to a mesh asset
- `Material`: reference to a material asset
- `Renderable`: combines `Mesh` and `Material`
- `Camera`: view/projection parameters
- `Light`: type, color, intensity
- `Collider`: shape and physics properties
- `Rigidbody`: mass, velocity, forces

Note: We separate `Position` from `Rotation` to optimize the memory footprint and performance of systems that only need one of those components. This is a common pattern in ECS design to allow for more efficient queries and updates.

Similarly we separate `Velocity` and `AngularVelocity` for the same reason. This allows systems that only need to update linear velocity to avoid touching the angular velocity data, and vice versa.

## Note on Positions

Our opinionated engine will put the following constraints on position data:
- World coordinates are in "UNITS" (i32) to allow for large worlds without floating point precision issues.
- The default world scale is 1 UNIT = 10 micrometers, which allows for animations and movements with imperceptible precision (0.01 mm is less than the width of a human hair) while still supporting large worlds (up to 43 kilometers in each direction, or ~1600 square kilometers).
- The `Position` component will store coordinates as `i32` values, and systems that need to convert to world units can do so by multiplying by the scale factor (10 micrometers per UNIT).

The world scale can be reinterpreted as necessary for different game types. For example, a space game might use 1 UNIT = 1 meter, while a city builder might use 1 UNIT = 1 centimeter. The key is that the engine will provide integer-based positions.

## Opinionated Defaults

| Setting | Value |
|---------|-------|
| Tick rate | 50 Hz fixed |
| Input buffer | 2 ticks (40 ms) |
| Rollback window | 8 ticks (160 ms) |
| Prediction | Always on |
| Authority | Server (even single-player) |
| Persistence | SQLite (Postgres optional) |
| Networking | UDP/QUIC + reliable channels |
| Security | mTLS cluster, server auth |
| Modding | Desktop-only, sandboxed plugins |
| Background work | ~1.5 ms/frame budget |

## Glossary

- **Tier S/L/M**: Software / Legacy GPU / Modern GPU
- **Effect IR**: Backend-neutral rendering compiled to shaders + Rust functions
- **Cells/Bubbles**: Spatial authority regions / temporary combat instances
- **PVS**: Potentially Visible Set (Quake-style pre-baked visibility)
- **ComponentId**: u32 identifier (not Rust's TypeId, stable across languages)
