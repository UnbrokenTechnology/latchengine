---
applyTo: "crates/latch_render/**"
---

# Rendering System Design

## Core Principle: Universal Floor

**Every feature works on every target**, from modern GPU to CPU-only fallback.

Single authoring surface → runtime strategy selection based on capability probes.

Always target 60+ FPS (higher is fine, but never lower).

Because we have an always-on server (see [networking instructions](networking.instructions.md)), we automatically decouple simulation from rendering. Simulation should never be blocked by rendering, and rendering should never cause simulation to drop below 50 Ticks-Per-Second (TPS).

## Architecture

```
Effect IR (backend-neutral)
    ↓
Runtime probes device caps
    ↓
Selects strategy: GPU backend OR software rasterizer
    ↓
Renders with automatically chosen implementation
```

## Capability Probes

On startup:
1. Detect device/API (D3D9/11, GL2.1/3.3, Metal, WebGL, software)
2. Query caps (MRT, SIMD, texture formats, uniform limits)
3. Bind strategies from ranked table per feature
4. Select asset formats to fit VRAM/caps
5. Auto-scale to maintain 60+ FPS

## Strategy Examples

| Feature | GPU Strategy | Software Fallback |
|---------|--------------|-------------------|
| Instancing | GPU instancing | CPU batching |
| MRT post-FX | Single-pass MRT | Multi-pass blending |
| Skinning | GPU matrices | CPU skinning + upload |
| Shadows | Depth map | Projected blob/static texture |
| Particles | GPU compute/VBO | CPU batches |

It's not important that the fallback matches the GPU output pixel-for-pixel, just that it looks visually similar.

All strategies → visually similar results, only performance varies.

## Auto-Scaler

Monitors: frame time, VRAM, draw calls
Controls: LOD bias, shadow map size, MSAA, particle density, post-FX scale

Editor warns when CPU-raster worst-case would miss 60 FPS.

## Software Rasterizer (Quake 3 Target)

**Goal**: Games that look like Quake 3 should run as fast as Quake 3.

### Performance Target

- 10,000 triangles/frame @ 60 FPS on low-spec CPUs (e.g. 2000-era hardware)
- Frustum culling (always on)
- PVS (Potentially Visible Set) for static scenes
- SIMD optimization (SSE2/NEON)

### Visibility Culling

**Frustum culling** (every frame):
```rust
if frustum.cull_sphere(entity.pos, entity.radius) { skip; }
```

**PVS** (pre-baked):
- Build time: Ray cast through portals to compute room visibility. Only need to cast at vertices.
- Runtime: O(1) lookup of visible rooms from current room
- Reduces rendered geometry by ~70% (Quake 3 stats)

### SIMD Paths

Detect CPU features and use best available:
- SSE2 (x86/x64)
- NEON (ARM)
- Portable SIMD fallback

Process 4 vertices/pixels in parallel where possible.

## Dynamic Buffer Growth

GPU buffers must grow with entity count:

```rust
if triangle_count > vertex_buffer_capacity {
    let new_capacity = triangle_count.next_power_of_two();
    self.vertex_buffer = device.create_buffer(new_capacity * vertex_size);
    self.vertex_buffer_capacity = new_capacity;
}
```

Logarithmic reallocations, amortized O(1) like Vec.

## Backends

- D3D9/11 (Windows)
- GL 2.1→3.3 (cross-platform)
- Metal 2+ (macOS/iOS)
- WebGL 1/2 (Web)
- Console SDKs (NVN/GNM via vendor integrations)
- Software rasterizer (always available)

## Effect IR (Future)

Author once in backend-neutral IR → compile to:
- WGSL/GLSL/HLSL shaders (GPU)
- Rust functions (software rasterizer)

Runtime selects compiled variant matching current strategy.

## Editor Debug Tools

- Strategy overlay ("High GPU", "Web", "CPU-only")
- Low-spec simulation toggles
- Per-strategy timings
- Auto-scaler logs and budget warnings
