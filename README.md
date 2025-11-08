# Latch Engine

A Rust-first, TypeScript-scripted, cross-platform game engine focused on fast dev iteration, deterministic simulation, and "works everywhere" shipping.

## Project Structure

```
latchengine/
├── crates/
│   ├── latch_core/       # ECS, jobs, time, deterministic math
│   ├── latch_render/     # Cross-platform rendering (Metal/D3D/GL/Software)
│   ├── latch_net/        # Distributed authority, cells, networking
│   ├── latch_script/     # TypeScript/JS execution (QuickJS/WASM)
│   ├── latch_asset/      # Asset pipeline (glTF, KTX2)
│   ├── latch_audio/      # Audio system
│   ├── latch_services/   # Platform abstraction layer
│   ├── latch_runtime/    # Game runtime binary
│   └── latch_editor/     # Editor application
└── docs/                 # Documentation
```

## Quick Start

### Prerequisites

- **Rust**: 1.80+ (you have 1.90 ✓)
- **Node.js**: 18+ (you have v18.17.1 ✓)

### Build & Run

```bash
# Build all crates
cargo build

# Run the runtime
cargo run --bin latch

# Run the editor
cargo run --bin latch-editor

# Run tests
cargo test --workspace
```

## Development Workflow

### Phase 0: Validation (Current)

We're validating core technical risks through proof-of-concepts:

1. **PoC 1**: Cross-platform windowing + triangle rendering
2. **PoC 2**: Minimal ECS + deterministic simulation
3. **PoC 3**: Rust ↔ TypeScript FFI
4. **PoC 4**: Distributed server authority

See [docs/phase0-plan.md](docs/phase0-plan.md) for details.

## Architecture Decisions

- **Multi-crate workspace**: Fast incremental compilation, clear boundaries
- **Server-always-runs**: Same code for single-player, co-op, and MMO
- **Fixed 60 Hz simulation**: Deterministic, replay-friendly
- **Handle-based scripting**: Scripts never hold raw pointers
- **Hot reload**: QuickJS for dev, WASM for shipping

## License

MIT OR Apache-2.0
