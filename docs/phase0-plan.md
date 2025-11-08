# Phase 0: Technical Validation

This phase validates the hardest technical risks before committing to full implementation.

## Goals

Prove that the core architectural decisions are sound through focused proof-of-concepts.

## PoCs

### PoC 1: Windowing + Simple Rendering ⭐ START HERE

**Goal**: Prove cross-platform window creation and basic GPU rendering works.

**Success Criteria**:
- Same Rust code creates a window on macOS, Windows, Linux
- Display a colored triangle using wgpu
- Window responds to close events

**Tech Stack**:
- `winit` for windowing
- `wgpu` for rendering abstraction

**Time Estimate**: 3-5 days

**Deliverable**: Working example in `crates/latch_runtime/examples/poc1_triangle.rs`

---

### PoC 2: Minimal ECS + Determinism

**Goal**: Prove deterministic simulation with visual feedback.

**Success Criteria**:
- Simple ECS with position/velocity components
- Fixed 60 Hz timestep
- Record inputs, replay them → identical results after 1000 frames
- Render moving triangles to visually verify

**Tech Stack**:
- Custom minimal ECS or `hecs` (evaluate both)
- PoC 1's rendering

**Time Estimate**: 5-7 days

**Deliverable**: Example showing deterministic replay with visual verification

---

### PoC 3: Rust ↔ TypeScript FFI

**Goal**: Prove TypeScript can safely interact with Rust ECS.

**Success Criteria**:
- TS can spawn entities, set position components
- Hot reload a `.js` file without restarting
- Measure FFI overhead (target: <10μs per call)

**Tech Stack**:
- `rquickjs` for QuickJS bindings
- PoC 2's ECS
- Handle-based API design

**Time Estimate**: 3-4 days

**Deliverable**: Example with hot-reloadable TS controlling entities

---

### PoC 4: Distributed Authority

**Goal**: Prove self-organizing server architecture works.

**Success Criteria**:
- Two server processes discover each other via gossip
- Leader election via Raft
- Leader assigns Cell A → Server 1, Cell B → Server 2
- Client connects, moves between cells, observes handoff

**Tech Stack**:
- SWIM/Serf-style gossip (consider `swim-rs` or custom)
- Raft consensus (consider `raft-rs` or custom)
- `quinn` for QUIC networking

**Time Estimate**: 7-10 days (hardest PoC)

**Deliverable**: Two-server demo with observable cell handoff

---

## After Phase 0

Create `docs/phase0-results.md` summarizing:
- What worked, what didn't
- Performance measurements
- Architectural decisions locked in
- Scope adjustments for Phase 1
