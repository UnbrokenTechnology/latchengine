---
applyTo: "crates/latch_net/**"
---

# Networking & Authority Model

## Core Principles

1. **Server always runs** - even single-player has embedded server
2. **Deterministic simulation** - 50 Hz fixed tick, seeded RNG
3. **Rollback networking** - client prediction + server correction
4. **Distributed authority** - multiple servers own different world regions

## Simulation Model

### Fixed Timestep

- **Tick rate**: 60 Hz (16.666 ms)
- **Render**: Variable, interpolates between ticks
- **Catch-up**: Max 1 extra substep, then time dilation
- **Input**: Sampled each render, applied on next tick

### Determinism Requirements

```rust
// ✅ ALLOWED
let pos = entity.position + velocity * FIXED_DT;
let rng = seeded_rng(entity.id, tick);

// ❌ FORBIDDEN
let time = std::time::SystemTime::now();  // Non-deterministic
let random = rand::random();  // Non-seeded
```

No wall-clock, stable iteration order, platform-stable math.

## Rollback Networking

### Defaults

- Input buffer: 2 ticks (~33 ms)
- Rollback window: 8 ticks (~133 ms)
- Authority: Server-authoritative
- Transport: UDP/QUIC with reliable channels

### Flow

1. Client predicts locally (instant feedback)
2. Client sends inputs to server
3. Server runs authoritative sim
4. Server sends state deltas to client
5. Client rolls back and resimulates
6. Client blends corrections (2-3 frames)

Average input-to-action latency: ~8 ms (half a tick).

## World Authority

### Cells

- Fixed spatial regions (~192 m grid, tunable)
- One cell = one authoritative server instance
- Handoff via snapshot → transfer → resume (no dual authority)

### Combat Bubbles

- Spawned on engagement
- 60 Hz with 8-tick rollback
- Covers active combatants (~2-16 players)
- Merges back after disengagement

### Client Flow

```
Inputs → Server → Authoritative Sim → Deltas → Client Rollback → Visual Blend
```

## Self-Organizing Servers

### One Binary, Multiple Roles

Single executable can be leader, worker, or relay (roles are logical).

### Discovery & Control

- **LAN**: UDP multicast/mDNS
- **WAN**: --join seed.host:7946
- **Membership**: Gossip protocol (SWIM-style)
- **Leader election**: Embedded Raft (no external etcd)
- **Scheduling**: Consistent hash ring

### Lifecycle

1. Node boots → joins gossip
2. Syncs ring state
3. Leader assigns cells/bubbles
4. Automatic reassignment on failure

### Deployment Modes

**Solo**: `node --solo` (leader + worker + relay, SQLite)
**Cluster**: `node --join seed` on each box
**Kubernetes**: Watches SRV/Lease, auto-joins (optional, never required)

### Security

- mTLS between nodes (rotating certs)
- Raft prevents split-brain
- Leader re-election ~2s
- Web admin UI (token auth, disable via flag)

## Replication

### State Serialization

- Per-archetype ECS pages
- Per-tick CRC for validation
- Delta compression for bandwidth

### Interest Management

- Spatial cells + team/party channels
- Auto-budget: entities/tick and bytes/tick
- Prioritize nearby/relevant entities

## Lag Compensation

Server maintains rewind buffer:
- Hit tests rewind to client's view timestamp
- Instant local feedback
- Silent reconciliation if prediction matches

## Default Settings (Recommended)

```rust
const TICK_RATE: u32 = 50;
const INPUT_BUFFER_TICKS: u32 = 2;
const ROLLBACK_WINDOW_TICKS: u32 = 8;
const CELL_SIZE_METERS: f32 = 192.0;
const BUBBLE_SIZE_PLAYERS: usize = 16;
const TARGET_RTT_MS: u32 = 40;
```

## MMO Scaling

- Cell size: 128-256 m with 2-cell hysteresis
- Bubble size: 2-16 players
- Target RTT: ≤40 ms (2 ticks)
- Workers auto-balance load via consistent hashing
