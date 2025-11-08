# Getting Started with Latch Engine Development

## ✅ Prerequisites (Already Installed)

- **Rust**: 1.90.0 ✓
- **Cargo**: 1.90.0 ✓  
- **Node.js**: v18.17.1 ✓

## ✅ Initial Setup Complete

The workspace is bootstrapped and ready for Phase 0 development.

### Workspace Structure

```
latchengine/
├── Cargo.toml              # Workspace configuration
├── crates/
│   ├── latch_core/         # ECS, jobs, time, math (foundation)
│   ├── latch_render/       # Rendering abstraction (winit, wgpu)
│   ├── latch_net/          # Network authority, cells, gossip
│   ├── latch_script/       # QuickJS/WASM runtime
│   ├── latch_asset/        # Asset pipeline
│   ├── latch_audio/        # Audio system
│   ├── latch_services/     # Platform services
│   ├── latch_runtime/      # Game player binary
│   └── latch_editor/       # Editor binary
└── docs/
    ├── phase0-plan.md      # Current phase roadmap
    └── architecture.md     # System design overview
```

## Quick Commands

```bash
# Build everything
cargo build

# Run the runtime
cargo run --bin latch

# Run the editor  
cargo run --bin latch-editor

# Run tests
cargo test --workspace

# Build in release mode
cargo build --release

# Check for errors without building
cargo check
```

## What's Next: Phase 0 PoCs

### 1. PoC 1: Window + Triangle (START HERE)

Create `crates/latch_runtime/examples/poc1_triangle.rs`:

```rust
use latch_render::window::{create_window, WindowConfig};
use winit::event_loop::EventLoop;

fn main() {
    let event_loop = EventLoop::new().unwrap();
    let window = create_window(&event_loop, WindowConfig::default());
    
    // TODO: Initialize wgpu, render a triangle
    // See: https://sotrh.github.io/learn-wgpu/
    
    event_loop.run(|event, target| {
        // Event handling
    });
}
```

**Goal**: See a colored triangle on screen.

**Resources**:
- [Learn wgpu Tutorial](https://sotrh.github.io/learn-wgpu/)
- `winit` already handles cross-platform windowing
- `wgpu` provides the rendering abstraction

### 2. PoC 2: Minimal ECS + Determinism

Add position/velocity components, fixed timestep, render moving triangles.

### 3. PoC 3: TypeScript FFI

Expose ECS to QuickJS, test hot reload.

### 4. PoC 4: Distributed Authority

Two servers, gossip discovery, cell handoff.

## Development Workflow Tips

### Fast Iteration

```bash
# Check without full build (faster)
cargo check

# Watch mode (requires cargo-watch)
cargo install cargo-watch
cargo watch -x check -x test
```

### Reducing Build Times

Already configured in `Cargo.toml`:
- Dependencies optimized even in dev mode
- Incremental compilation enabled
- Only changed crates rebuild

### Working with Examples

```bash
# Run a specific example
cargo run --example poc1_triangle

# List all examples
cargo run --example
```

## Common Issues

### wgpu/Metal Errors on macOS

If you see Metal shader compilation errors:
```bash
# Ensure Xcode Command Line Tools are installed
xcode-select --install
```

### QuickJS Binding Errors

The `rquickjs` crate requires `libclang`:
```bash
# macOS (via Homebrew)
brew install llvm

# Linux (Ubuntu/Debian)
sudo apt-get install libclang-dev
```

Already working for you since the build succeeded! ✓

## Documentation

- [docs/phase0-plan.md](docs/phase0-plan.md) - Current phase details
- [docs/architecture.md](docs/architecture.md) - System architecture
- [.github/copilot-instructions.md](.github/copilot-instructions.md) - Full project plan

## Help & Resources

- **Rust Game Development**: https://arewegameyet.rs/
- **wgpu Tutorial**: https://sotrh.github.io/learn-wgpu/
- **winit Docs**: https://docs.rs/winit/
- **Bevy Engine** (reference): https://github.com/bevyengine/bevy

---

**Status**: ✅ Ready to start PoC 1

**Next Step**: Create `crates/latch_runtime/examples/poc1_triangle.rs` and render your first triangle!
