# Dependency Analysis & Justification

This document explains every external dependency and whether we should keep, replace, or remove it.

## Current Dependencies (Phase 0)

### ✅ **Essential - Keep Long-Term**

#### **winit** - Windowing
- **What**: Cross-platform window creation and event handling
- **Why essential**: Writing platform-specific code for Win32, Cocoa, X11, Wayland would be ~10k lines. Not our differentiator.
- **Performance impact**: Zero. It's a thin wrapper around OS APIs.
- **Keep**: Yes, permanently.

#### **bytemuck** - Safe byte casting
- **What**: Zero-cost traits for casting types to byte slices (for GPU uploads)
- **Why essential**: Prevents undefined behavior when uploading data to GPU. Pure compile-time checks.
- **Performance impact**: Zero (no runtime code).
- **Keep**: Yes, permanently.

#### **rquickjs** - QuickJS bindings
- **What**: Rust bindings to QuickJS JavaScript engine
- **Why essential**: We need QuickJS for dev-time scripting. These bindings are well-maintained.
- **Could we write our own?**: Yes, but marginal benefit. Would be ~2000 lines of unsafe FFI code.
- **Keep**: Yes. Re-evaluate only if we hit specific performance issues.

#### **serde** + **thiserror** - Serialization and errors
- **What**: Industry-standard serialization framework and error derive macros
- **Why essential**: Network protocol needs serialization. Both are compile-time (proc macros).
- **Performance impact**: Zero (compile-time code generation).
- **Keep**: Yes, permanently.

#### **tracing** + **tracing-subscriber** - Logging/profiling
- **What**: Structured logging and profiling framework
- **Why essential**: Debugging, performance analysis. Compile-time filtered (zero cost when disabled).
- **Performance impact**: ~10ns per log point when enabled, 0ns when disabled.
- **Keep**: Yes, permanently.

#### **quinn** - QUIC networking
- **What**: Modern UDP-based protocol with reliability, congestion control, encryption
- **Why essential**: Hard to implement better ourselves. Handles NAT traversal, packet loss, etc.
- **Could we write our own?**: Not advisable. QUIC is complex (~20k lines). Would take months.
- **Keep**: Yes. This is a solved problem; don't reinvent.

---

### ⚠️ **Evaluate - May Replace**

#### **wgpu** - GPU abstraction
- **Status**: Keep for Phase 0 PoCs
- **Long-term plan**: Replace with custom backends for D3D9, OpenGL 2.1, software rasterizer (per project plan)
- **Reason**: wgpu targets modern APIs (D3D11+, Vulkan, Metal). We need older API support.
- **Timeline**: Replace in Phase 2 (rendering system implementation)

#### **glam** - Math library
- **Status**: Keep for now
- **Concern**: Uses `f32` by default. We may need fixed-point for determinism.
- **Evaluate**: After PoC 2 (determinism testing). If floats work deterministically, keep it.
- **Alternative**: Write custom fixed-point math or use `fixed` crate.

#### **tokio** - Async runtime
- **Status**: **Consider removing or making optional**
- **Current use**: Network I/O in `latch_net`
- **Concern**: Do we need async? Our sim is fixed-tick. May only need for I/O thread.
- **Action**: Make optional. See if we can use `std::thread` + channels instead.
- **Size**: Large dependency (~100 crates transitive)

#### **serde_json** - JSON serialization
- **Status**: **Consider removing**
- **Current use**: Config files (maybe)
- **Alternative**: Binary formats (bincode, rmp) are faster and smaller
- **Action**: Remove if we don't need human-readable configs. Use TOML for user-facing, binary for internal.

#### **dashmap** - Concurrent HashMap
- **Status**: Keep for Phase 0
- **Evaluate**: After PoC 4 (distributed authority). May not need lock-free structures if we use message passing.
- **Alternative**: `std::sync::RwLock<HashMap>` is simpler if contention is low.

---

### ❌ **Remove After Phase 0**

#### **hecs** - ECS reference implementation
- **Status**: Optional feature, only for comparison
- **Plan**: Remove once we write our own authority-aware ECS
- **Timeline**: After PoC 2

#### **anyhow** - Error convenience
- **Status**: Keep for development/examples
- **Plan**: Don't use in ship builds. Use `thiserror` or custom errors for released code.
- **Reason**: `anyhow` adds small runtime overhead. Fine for dev, not for ship.

---

## Dependency Count Analysis

**Total unique crates after `cargo build`**: 223 crates

**Direct dependencies we declared**: 13
- latch_* (7 internal)
- winit, wgpu, bytemuck, rquickjs, glam, serde (+ json), tokio, tracing (+ subscriber), anyhow, thiserror, quinn, dashmap, hecs

**Why 223?** Transitive dependencies:
- `wgpu` pulls ~80 crates (shaderc, naga, gpu-alloc, platform backends)
- `tokio` pulls ~40 crates (mio, parking_lot, etc)
- `rquickjs` pulls ~20 crates (bindgen, libloading)
- `quinn` pulls ~15 crates (rustls, ring, etc)

**Action items to reduce**:
1. Make `tokio` optional → saves ~40 crates
2. Remove `hecs` after Phase 0 → saves ~5 crates
3. Consider `wgpu` → custom backends (Phase 2) → saves ~80 crates eventually

---

## Performance Philosophy

From your project plan:
> "Runtime performance: 60 FPS minimum on low-spec hardware"

**Our approach**:
1. **Don't reinvent solved problems**: winit, quinn, serde are industry-proven
2. **Zero-cost abstractions preferred**: bytemuck, thiserror are compile-time only
3. **Profile before replacing**: Keep dependencies until we measure they're a problem
4. **Custom code for core loops**: ECS, rendering, deterministic math will be ours

**When to write our own**:
- ✅ ECS (authority-aware, specific to our needs)
- ✅ Rendering backends (need D3D9/GL2.1 support wgpu doesn't provide)
- ✅ Deterministic math (if floats prove non-deterministic)
- ❌ Windowing (winit is perfect)
- ❌ QUIC (quinn is battle-tested)
- ❌ Serialization framework (serde is zero-cost)

---

## Action Plan

### Immediate (Now)
- ✅ Fix all warnings (done)
- ✅ Document dependencies (this file)

### Phase 0
- [ ] Evaluate if we actually need `tokio` (make optional or remove)
- [ ] Remove `serde_json` if not needed
- [ ] Keep `glam` but test determinism in PoC 2

### Phase 1+
- [ ] Remove `hecs` once we have our own ECS
- [ ] Remove `wgpu` once we have custom backends (Phase 2)
- [ ] Profile all hot paths; replace dependencies only if they show up

---

## Dependency Decision Matrix

| Dependency | LOC to replace | Time to replace | Performance gain | Decision |
|------------|----------------|-----------------|------------------|----------|
| winit | ~10,000 | 2-3 months | None | **Keep** |
| bytemuck | ~500 | 1 week | None | **Keep** |
| rquickjs | ~2,000 | 2-3 weeks | Marginal | **Keep** |
| serde | ~5,000 | 1-2 months | None (zero-cost) | **Keep** |
| quinn | ~20,000 | 3-6 months | Unknown | **Keep** |
| glam | ~3,000 | 2-4 weeks | Maybe (fixed-point) | **Evaluate** |
| wgpu | ~15,000 | 2-3 months | Yes (custom backends needed) | **Replace Phase 2** |
| tokio | N/A | N/A | Yes (may not need async) | **Make optional** |

---

**Summary**: Our dependency choices are sound. Most are either zero-cost (compile-time) or solving problems that would take months to reimplement. We'll profile and replace strategically.
