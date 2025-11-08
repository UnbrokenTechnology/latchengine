# PoC 1: Results

**Date**: November 7, 2025  
**Status**: ✅ **SUCCESS**

## Objectives

- [x] Window opens on macOS
- [x] Colored triangle renders using wgpu
- [x] GPU backend automatically detected
- [x] Clean shutdown on window close

## Results

### Hardware Detection

```
GPU Adapter: AdapterInfo {
  name: "Apple M1 Max",
  vendor: 0,
  device: 0,
  device_type: IntegratedGpu,
  driver: "",
  driver_info: "",
  backend: Metal
}
```

**✅ Metal backend automatically selected** - validates cross-platform abstraction works.

### Build Performance

- **Initial build**: ~58s (includes wgpu and all dependencies)
- **Incremental rebuild**: ~0.58s (after code changes)
- **Final binary**: `target/debug/examples/poc1_triangle`

### Code Quality

- **Zero warnings** during compilation
- **Clean architecture**: Window management separated from rendering
- **Proper lifetime handling**: Used `Arc<Window>` for `'static` surface requirement

## Technical Details

### Window Creation

- Used `winit` 0.30's new `ApplicationHandler` trait
- Window created in `resumed` event (required by winit 0.30+)
- Clean event loop with `about_to_wait` for continuous rendering

### Rendering Pipeline

- **Shader**: WGSL (WebGPU Shading Language)
- **Primitive**: Triangle list (3 vertices, no vertex buffer)
- **Colors**: Red, Green, Blue vertices (interpolated)
- **Clear color**: Dark blue background (0.1, 0.2, 0.3)

### wgpu Configuration

- **Backends**: All available (Metal selected on macOS)
- **Power preference**: Default
- **Present mode**: Immediate (first available)
- **Surface format**: sRGB preferred

## Cross-Platform Validation

### macOS ✅

- **Tested**: Yes (Apple M1 Max, Metal backend)
- **Result**: Working perfectly

### Windows ⏸️

- **Tested**: No (requires Windows machine)
- **Expected**: Should work with DirectX 11/12 backend

### Linux ⏸️

- **Tested**: No (requires Linux machine)
- **Expected**: Should work with Vulkan or OpenGL backend

## Performance Notes

### Rendering

- **Frame rendering**: Immediate mode, continuous redraw
- **No performance measurement yet** (Phase 0 is validation only)
- **Async initialization**: Using `pollster::block_on` for simplicity

### Memory

- No memory profiling in this PoC
- Surface owns window via `Arc` - proper lifetime management

## What We Validated

1. ✅ **Cross-platform windowing works** (`winit` handles OS differences)
2. ✅ **GPU abstraction works** (`wgpu` selected Metal automatically)
3. ✅ **Build system is correct** (dependencies properly scoped)
4. ✅ **Zero-warning builds** (code quality maintained)
5. ✅ **Window lifecycle** (open, render, close all working)

## Blockers / Issues

**None.** PoC 1 completed successfully on first attempt after fixing lifetime issues.

## Next Steps

**Ready for PoC 2**: Minimal ECS + Determinism

With rendering validated, we can now:
1. Create simple ECS with position/velocity components
2. Render moving triangles (visual feedback for determinism)
3. Implement fixed timestep
4. Add input recording/replay
5. Prove determinism: same inputs → same positions after 1000 frames

## Code Location

- **Example**: `crates/latch_runtime/examples/poc1_triangle.rs`
- **Window utilities**: `crates/latch_render/src/window.rs`

## Run Command

```bash
cargo run --example poc1_triangle
```

## Success Criteria Met

✅ All criteria satisfied:
- Window opens
- Triangle renders with colors
- Clean shutdown
- Zero warnings
- Cross-platform code (validated on macOS, portable to Win/Linux)

---

**PoC 1 Status: COMPLETE** 🎉
