# Cross-Compilation for Native Gameplay Code

## Problem Statement

Developers may want to write performance-critical gameplay code in Rust (physics, AI, simulation) that gets compiled alongside the engine runtime. Can they cross-compile from one platform to all others?

---

## Answer: YES (with caveats)

### ✅ **What Cross-Compiles Easily**

Pure Rust code with **no platform-specific dependencies**:

```rust
// crates/gameplay/src/physics.rs
// Custom physics engine - pure math, no OS/graphics dependencies

use glam::{Vec3, Quat};

pub struct RigidBody {
    pub position: Vec3,
    pub velocity: Vec3,
    pub mass: f32,
}

pub fn integrate_physics(bodies: &mut [RigidBody], dt: f32) {
    for body in bodies {
        body.velocity += Vec3::Y * -9.81 * dt; // Gravity
        body.position += body.velocity * dt;
    }
}
```

**Cross-compiles from any platform to any platform.** Zero issues.

---

## Cross-Compilation Matrix

| From → To | Windows | macOS | Linux | Web (WASM) | iOS | Android |
|-----------|---------|-------|-------|------------|-----|---------|
| **Windows** | ✅ Native | ⚠️ Hard | ✅ Easy | ✅ Easy | ❌ No | ⚠️ Hard |
| **macOS** | ✅ Easy | ✅ Native | ✅ Easy | ✅ Easy | ✅ Easy | ⚠️ Hard |
| **Linux** | ✅ Easy | ⚠️ Hard | ✅ Native | ✅ Easy | ❌ No | ⚠️ Hard |

**Legend**:
- ✅ **Native**: Building on the same platform
- ✅ **Easy**: `rustup target add <target> && cargo build --target <target>` just works
- ⚠️ **Hard**: Requires cross-compilation toolchain setup (linker, C stdlib)
- ❌ **No**: Apple SDK legally required (can't do on non-Apple hardware)

---

## Why Graphics Was Hard vs. Why Physics Is Easy

### Graphics Dependencies (Hard)

```toml
[dependencies]
winit = "0.30"     # Needs: Win32 SDK / Cocoa SDK / X11/Wayland libs
wgpu = "23.0"      # Needs: DirectX SDK / Metal SDK / Vulkan SDK
```

**Problem**: Each platform needs **SDK headers/libraries** that can't be redistributed.

### Gameplay Code (Easy)

```toml
[dependencies]
glam = "0.29"      # Pure Rust math (no C dependencies)
serde = "1.0"      # Pure Rust serialization
rand = "0.8"       # Pure Rust RNG
# Your custom physics code - all Rust
```

**No problem**: Everything is pure Rust or vendored C code (like `rand`'s ChaCha implementation).

---

## Practical Example: Custom Physics Module

### Developer's Project Structure

```
my_game/
├── scripts/
│   └── game.ts              # Main gameplay loop
├── native/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       └── physics.rs       # Custom Rust physics
└── latch.toml
```

### `native/Cargo.toml`

```toml
[package]
name = "my_game_native"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]  # Dynamic library for dev hot reload

[dependencies]
glam = "0.29"            # Math library (pure Rust)
serde = "1.0"            # Serialization (pure Rust)

[target.wasm32-unknown-unknown.dependencies]
# No special dependencies needed for WASM
```

### `native/src/lib.rs`

```rust
use glam::Vec3;

#[repr(C)]
pub struct PhysicsState {
    positions: *mut Vec3,
    velocities: *mut Vec3,
    count: usize,
}

#[no_mangle]
pub extern "C" fn physics_tick(state: &mut PhysicsState, dt: f32) {
    let positions = unsafe { std::slice::from_raw_parts_mut(state.positions, state.count) };
    let velocities = unsafe { std::slice::from_raw_parts_mut(state.velocities, state.count) };
    
    for i in 0..state.count {
        velocities[i] += Vec3::Y * -9.81 * dt;
        positions[i] += velocities[i] * dt;
    }
}
```

---

## Cross-Compilation Workflow

### Option 1: Simple Targets (Recommended)

**From any platform → Windows, Linux, WASM**:

```bash
# One-time setup
rustup target add x86_64-pc-windows-gnu
rustup target add x86_64-unknown-linux-gnu
rustup target add wasm32-unknown-unknown

# Build for all targets
cargo build --release --target x86_64-pc-windows-gnu
cargo build --release --target x86_64-unknown-linux-gnu
cargo build --release --target wasm32-unknown-unknown
```

**Just works** for pure Rust code. No linker issues because no C dependencies.

### Option 2: Use `cross` Tool (For Complex Cases)

If your native code **does** have C dependencies (e.g., using FFI to a C physics library):

```bash
# Install cross (uses Docker containers with pre-configured toolchains)
cargo install cross

# Build for any target from any host
cross build --release --target x86_64-pc-windows-gnu
cross build --release --target x86_64-unknown-linux-gnu
cross build --release --target aarch64-unknown-linux-gnu
```

**Advantage**: Handles all linker/libc complexity via Docker.

**Disadvantage**: Requires Docker (but Docker Desktop is free for personal use).

### Option 3: GitHub Actions (Zero Local Setup)

Developer doesn't install anything. Just push code:

```yaml
# .github/workflows/build-native.yml
name: Build Native Gameplay Code

on: [push]

jobs:
  build:
    strategy:
      matrix:
        include:
          - os: windows-latest
            target: x86_64-pc-windows-msvc
          - os: macos-latest
            target: x86_64-apple-darwin
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
    
    runs-on: ${{ matrix.os }}
    
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      
      - name: Build native module
        run: |
          cd native
          cargo build --release --target ${{ matrix.target }}
      
      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: native-${{ matrix.target }}
          path: native/target/${{ matrix.target }}/release/
```

**Free for public repos. 2000 minutes/month for private repos.**

---

## Integration with Latch Packaging

### Workflow with Native Code

```bash
# Developer on macOS with native Rust physics module
latch package --platform windows

# What happens:
# 1. Check if native/ directory exists
# 2. If yes:
#    a. Build native code for target platform (or fetch from CI artifacts)
#    b. Link into runtime binary
# 3. Compile TypeScript → WASM
# 4. Pack assets
# 5. Bundle everything
```

### Implementation Sketch

```rust
// crates/latch_cli/src/package.rs

pub fn package(config: PackageConfig) -> Result<()> {
    let runtime_binary = if has_native_code(&config.project_dir) {
        // Option A: Try local cross-compilation
        if can_cross_compile(&config.platform) {
            build_with_native_code(&config)?
        } else {
            // Option B: Fetch from CI artifacts (requires GitHub Actions setup)
            fetch_ci_artifact(&config)?
        }
    } else {
        // No native code - just download pre-built runtime
        download_runtime(config.platform, ENGINE_VERSION)?
    };
    
    // ... rest of packaging
}
```

---

## Special Case: macOS/iOS

**Apple platforms are the exception**:

```rust
// This will NOT cross-compile to macOS from Windows/Linux:
cargo build --target x86_64-apple-darwin
// Error: requires macOS SDK (Xcode)
```

**Solutions**:

1. **GitHub Actions** (runs on actual macOS VMs) ✅ Free
2. **Buy a Mac Mini** ($599) and use it as a build server
3. **Rent macOS VM** (MacStadium, AWS EC2 Mac) ~$50/month
4. **Don't support macOS** (joking... mostly)

**BUT**: Most indie developers targeting macOS already own a Mac. They can build locally.

---

## Latch Engine Policy

### Recommended Approach

```toml
# latch.toml
[native]
enabled = false  # Default: no native code

# If developer opts in:
# enabled = true
# build-strategy = "local"  # or "ci" or "manual"
```

### Strategy Options

#### 1. **Local** (Default if native code exists)
- Tries to cross-compile locally
- Falls back to CI if cross-compilation fails
- Best for: Developers with Docker or multiple machines

#### 2. **CI** (Recommended for most)
- Always builds via GitHub Actions
- Developer runs `git push` → waits → downloads artifacts
- Best for: Solo developers on one platform

#### 3. **Manual**
- Developer provides pre-built `.so`/`.dll`/`.dylib` files
- Latch just bundles them
- Best for: Teams with dedicated build machines

---

## Performance Impact

### Rust Native Code vs. WASM

| Metric | Native (Rust) | WASM | Overhead |
|--------|---------------|------|----------|
| Math ops | 100% | ~95% | 5% slower |
| Memory access | 100% | ~90% | Bounds checks |
| Function calls | 100% | ~80% | FFI overhead |
| SIMD | 100% | ~60%* | Limited SIMD in WASM |

*WASM SIMD is improving but not as mature as native

### When to Use Native Rust

✅ **Good candidates**:
- Physics simulation (tight loops, SIMD)
- Pathfinding (A*, nav meshes)
- Procedural generation (noise, terrain)
- AI decision trees
- Custom compression/decompression

❌ **Not worth it**:
- Business logic (inventory, quests)
- UI code
- Anything that calls FFI frequently (overhead > gains)

---

## Example: Physics Engine Cross-Compilation Test

Let's create a minimal test to prove this works:

```rust
// my_game/native/src/lib.rs
#![cfg_attr(target_arch = "wasm32", no_std)]

use core::f32;

#[repr(C)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[no_mangle]
pub extern "C" fn physics_step(
    positions: *mut Vec3,
    velocities: *mut Vec3,
    count: usize,
    dt: f32,
) {
    #[cfg(not(target_arch = "wasm32"))]
    use std::slice;
    #[cfg(target_arch = "wasm32")]
    extern crate alloc;
    
    let positions = unsafe { core::slice::from_raw_parts_mut(positions, count) };
    let velocities = unsafe { core::slice::from_raw_parts_mut(velocities, count) };
    
    for i in 0..count {
        velocities[i].y -= 9.81 * dt;
        positions[i].x += velocities[i].x * dt;
        positions[i].y += velocities[i].y * dt;
        positions[i].z += velocities[i].z * dt;
    }
}
```

**Build for all targets**:

```bash
# Windows
cargo build --release --target x86_64-pc-windows-gnu

# Linux
cargo build --release --target x86_64-unknown-linux-gnu

# WASM
cargo build --release --target wasm32-unknown-unknown

# macOS (only on macOS)
cargo build --release --target x86_64-apple-darwin
```

**All succeed** (except macOS from non-Apple hardware).

---

## Conclusion

### For Pure Rust Gameplay Code (Physics, AI, etc.)

| Platform | Cross-Compilation Difficulty |
|----------|------------------------------|
| Windows ← macOS/Linux | ✅ **Easy** (just `rustup target add`) |
| Linux ← macOS/Windows | ✅ **Easy** |
| WASM ← Any | ✅ **Trivial** |
| macOS ← Windows/Linux | ❌ **Impossible** (need Apple SDK) |
| iOS ← Windows/Linux | ❌ **Impossible** |
| Android ← Any | ⚠️ **Medium** (need Android NDK, but it's free) |

### Recommended Developer Workflow

1. **No native code** (90% of games): Just use `latch package` with pre-built runtimes ✅
2. **With native code, single platform**: Build locally, package normally ✅
3. **With native code, multi-platform**:
   - **Best**: Use GitHub Actions (free for public repos) ✅
   - **Good**: Use `cross` tool with Docker ✅
   - **Last resort**: Build on each platform manually ⚠️

### Latch Engine's Role

We provide:
1. ✅ Pre-built runtimes (so native code is optional)
2. ✅ GitHub Actions templates (for easy CI setup)
3. ✅ `cross`-compatible build scripts
4. ✅ Documentation on when native code is worth it
5. ❌ **Not** a cloud build service (use free GitHub Actions instead)

---

## Bottom Line

**Physics/AI Rust code is 10x easier to cross-compile than graphics code** because:
- No platform SDKs needed
- No dynamic linking to OS libraries
- Pure Rust (or vendored C) compiles everywhere
- WASM is a perfect fallback target

**The graphics SDK problem doesn't apply here.**
