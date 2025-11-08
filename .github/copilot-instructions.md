Latch Engine — Core Architecture Guide

A Rust-first, TypeScript-scripted, cross-platform game engine focused on fast iteration, deterministic simulation, and universal compatibility.

## Core Principles

- **Cross-platform dev tools**: Windows, macOS, Linux with no compromises
- **Ultra-fast iteration**: Hot reload, instant script turnaround
- **Ship anywhere**: Desktop, mobile, web, consoles from one codebase
- **60 FPS on toasters**: Auto-scaling + fallbacks for low-spec hardware
- **Deterministic by default**: Fixed tick, seeded RNG, replay-friendly
- **Git built-in**: Git + LFS + GCM integrated into editor
- **MMO-ready**: Same code for solo → MMO via distributed authority
- **Rust + TypeScript**: Engine in Rust, gameplay in TS (WASM), Rust for hot paths
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

- **Dev mode**: TypeScript → QuickJS (instant hot reload)
- **Ship mode**: TypeScript → WASM via AssemblyScript (identical FFI)
- **Future**: Custom TS→SSA compiler for native/WASM
- **FFI contract**: Scripts get opaque handles, never raw pointers. Access via views or copy-out structs.

### Memory Model

- Rust owns all memory
- Dev (QuickJS): GC allowed, per-frame budget tracked
- Ship (WASM): Linear memory, arena allocators provided
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

- Entity: 8-byte handle with generation
- ComponentId: u32 (stable across Rust/TypeScript, not TypeId)
- Archetype: SoA storage for entities with identical component sets
- Object pooling per archetype (free list prevents fragmentation)
- Parallel iteration safe (SoA enables &mut [T] per column)

## Networking (See: `.github/instructions/networking.instructions.md`)

**Server-authoritative rollback networking** with distributed world authority.

- Simulation: 60 Hz fixed tick, 2-tick input buffer, 8-tick rollback
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
- **Mods**: Desktop-only WASM plugins (sandboxed, capability-gated)

## Build & Packaging

- **Dev builds**: Dynamic linking, hot reload, QuickJS, shader cache
- **Ship builds**: Static linking, LTO, baked pipelines, WASM scripts
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

- Script sandbox: WASM with capability tokens
- Mod sandbox: Desktop-only, WASM, no dlopen
- Telemetry: Explicit opt-in, PII-safe
- Server: mTLS between nodes, server-authoritative, state-hash validation

## Quality Gates

- **Compatibility enforcement**: Editor hides disallowed features, CI fails on violations
- **Performance budgets**: Per-frame allocations, draw calls, texture memory tracked
- **Testing**: Golden replay tests, soak tests for GC, platform adapter smoke tests

## Opinionated Defaults

| Setting | Value |
|---------|-------|
| Tick rate | 60 Hz fixed |
| Input buffer | 2 ticks (~33 ms) |
| Rollback window | 8 ticks (~133 ms) |
| Prediction | Always on |
| Authority | Server (even single-player) |
| Persistence | SQLite (Postgres optional) |
| Networking | UDP/QUIC + reliable channels |
| Security | mTLS cluster, server auth |
| Modding | Desktop-only, sandboxed WASM |
| Background work | ~1.5 ms/frame budget |

## Glossary

- **Tier S/L/M**: Software / Legacy GPU / Modern GPU
- **Effect IR**: Backend-neutral rendering compiled to shaders + Rust functions
- **Cells/Bubbles**: Spatial authority regions / temporary combat instances
- **PVS**: Potentially Visible Set (Quake-style pre-baked visibility)
- **ComponentId**: u32 identifier (not Rust's TypeId, stable across languages)
