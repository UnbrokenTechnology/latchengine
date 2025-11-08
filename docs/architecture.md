# Latch Engine Architecture

## Overview

Latch is a **server-authoritative, deterministic, cross-platform** game engine with distributed world authority.

## Core Principles

1. **Server Always Runs**: Even in single-player, a server process manages game state
2. **Deterministic Simulation**: Fixed 60 Hz tick, seeded RNG, replay-friendly
3. **Distributed Authority**: Multiple servers can own different spatial regions (cells)
4. **Handle-Based Scripting**: Scripts use opaque handles, never raw pointers
5. **Hot Reload First**: Dev iteration speed is paramount

## System Layers

```
┌────────────────────────────────────────┐
│  Game Scripts (TypeScript/WASM)        │
└────────────┬───────────────────────────┘
             │ FFI Layer (handles only)
             ↓
┌────────────────────────────────────────┐
│  Services Layer                        │
│  (Save, Settings, Input, Telemetry)    │
└────────────┬───────────────────────────┘
             │
             ↓
┌────────────────────────────────────────┐
│  Rendering (client-side only)          │
│  - Backend abstraction                 │
│  - Strategy selection                  │
│  - Auto-scaling                        │
└────────────────────────────────────────┘

┌────────────────────────────────────────┐
│  Network Authority Layer               │
│  - Cell assignment                     │
│  - Gossip/Raft                         │
│  - Client-server sync                  │
└────────────┬───────────────────────────┘
             │
             ↓
┌────────────────────────────────────────┐
│  Core Simulation                       │
│  - ECS (authority-aware)               │
│  - Jobs scheduler                      │
│  - Fixed timestep                      │
│  - Deterministic math                  │
└────────────────────────────────────────┘
```

## Key Concepts

### Cells

World is divided into fixed spatial regions (~192m grid). Each cell has one authoritative server at a time.

### Combat Bubbles

Temporary high-frequency simulation regions spawned during combat. Merge back after disengagement.

### Determinism

- Fixed 60 Hz tick
- Seeded RNG (no `rand::random()`)
- No wall-clock reads in gameplay
- Stable iteration order (ECS)
- Platform-stable math (controlled SIMD)

### Rollback Networking

- Client predicts immediately
- Server runs authoritative sim
- Client rolls back and resimulates on correction
- 2-tick input buffer, 8-tick rollback window

## Crate Responsibilities

| Crate | Purpose |
|-------|---------|
| `latch_core` | ECS, jobs, time, deterministic math |
| `latch_render` | Rendering backends and abstraction |
| `latch_net` | Authority, cells, gossip, replication |
| `latch_script` | QuickJS/WASM runtime, FFI layer |
| `latch_asset` | Asset pipeline, loaders |
| `latch_audio` | Audio playback, mixing |
| `latch_services` | Platform abstraction (saves, input, etc) |
| `latch_runtime` | Minimal game player binary |
| `latch_editor` | Editor GUI application |

## Threading Model

- **Main thread**: Graphics/audio APIs (platform requirement)
- **Sim thread**: Fixed 60 Hz tick, ECS updates
- **Job threads**: Work-stealing pool for parallel systems
- **Net thread**: I/O, replication

## Memory Model

- **Rust owns all memory**: Scripts get handles
- **Component access**: Bounded views or copy-out structs
- **Dev (QuickJS)**: GC allowed, budget-tracked
- **Ship (WASM)**: Linear memory, arena allocators

## Security

- mTLS between servers
- Server-authoritative (no client trust)
- WASM sandbox for mods (desktop only)
- Script capability tokens
