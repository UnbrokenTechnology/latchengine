# Software Rasterizer Design

## Philosophy: Quake 3 Performance Parity

**Goal**: Games that look like Quake 3 should run as fast as Quake 3 on equivalent hardware.

### Why This Matters

Modern "retro-style" games built in Unity/Unreal often suffer from:
- ❌ Long shader compilation times
- ❌ Graphical stutters (shader caching, asset streaming)
- ❌ VRAM bloat (uncompressed textures, wasteful formats)
- ❌ CPU overhead (inefficient culling, draw call batching)

**But Quake 3 (1999) ran on a CPU!** It achieved 60 FPS on a Pentium III (450 MHz) with:
- ✅ ~5,000-10,000 triangles per frame
- ✅ Frustum culling (always)
- ✅ PVS (Potentially Visible Set) - pre-baked room visibility
- ✅ Occlusion culling (portal-based)
- ✅ Efficient software rasterizer fallback
- ✅ SIMD optimization (3DNow! / SSE)

**Latch Engine targets this performance** for low-poly games.

---

## Architecture

### Dual Pipeline Model

```
┌─────────────────────────────────────────────────────┐
│                   Effect IR                         │
│         (High-level material description)           │
└───────────────────┬─────────────────────────────────┘
                    │
        ┌───────────┴───────────┐
        │                       │
        ▼                       ▼
┌───────────────┐       ┌──────────────────┐
│  GPU Backend  │       │ Software Backend │
│  (WGSL/GLSL)  │       │   (Rust/SIMD)    │
└───────────────┘       └──────────────────┘
        │                       │
        ▼                       ▼
   GPU Pipeline            CPU Rasterizer
   (Metal/D3D/            (SSE2/NEON
    Vulkan/GL)             optimized)
```

**Key principle**: Effect IR compiles to **both** shader code and Rust functions. Same visual output, different execution paths.

---

## Software Rasterizer Components

### 1. Vertex Processing

```rust
pub trait VertexShader {
    fn process(&self, input: &VertexInput) -> VertexOutput;
}

// Example: Simple MVP transform
struct MVPVertexShader {
    mvp_matrix: Mat4,
}

impl VertexShader for MVPVertexShader {
    fn process(&self, input: &VertexInput) -> VertexOutput {
        VertexOutput {
            position: self.mvp_matrix * input.position.extend(1.0),
            color: input.color,
            texcoord: input.texcoord,
        }
    }
}
```

### 2. Triangle Rasterization

**Two strategies**:

#### A. Scanline Rasterization (Quake-style)
- Iterate row-by-row
- Calculate edge intersections
- Fill horizontal spans
- **Fast for large triangles**

```rust
fn rasterize_scanline(v0, v1, v2, framebuffer) {
    let (min_y, max_y) = compute_y_bounds(v0, v1, v2);
    
    for y in min_y..max_y {
        let (x_start, x_end) = compute_span(y, v0, v1, v2);
        
        for x in x_start..x_end {
            let bary = barycentric(x, y, v0, v1, v2);
            let color = interpolate(v0.color, v1.color, v2.color, bary);
            framebuffer[y * width + x] = fragment_shader(color);
        }
    }
}
```

#### B. Tile-Based Rasterization (Modern)
- Divide screen into 16x16 tiles
- Test which triangles overlap each tile
- Rasterize triangle only in overlapping tiles
- **Better cache locality, SIMD friendly**

```rust
const TILE_SIZE: usize = 16;

fn rasterize_tiled(triangles, framebuffer) {
    let tiles = divide_screen_into_tiles();
    
    for tile in tiles {
        let overlapping = triangles.iter()
            .filter(|tri| tri.overlaps_tile(tile))
            .collect();
        
        for tri in overlapping {
            rasterize_triangle_in_tile(tri, tile, framebuffer);
        }
    }
}
```

**Latch will use**: Hybrid approach (scanline for large tris, tiled for small batches)

### 3. Fragment Processing

```rust
pub trait FragmentShader {
    fn shade(&self, input: &FragmentInput) -> Color;
}

// Example: Textured + lit
struct TexturedFragmentShader {
    texture: Texture,
    light_dir: Vec3,
}

impl FragmentShader for TexturedFragmentShader {
    fn shade(&self, input: &FragmentInput) -> Color {
        let tex_color = self.texture.sample(input.texcoord);
        let lighting = input.normal.dot(self.light_dir).max(0.0);
        tex_color * lighting
    }
}
```

---

## Visibility Culling (Critical for Performance)

### 1. Frustum Culling (Always On)

```rust
pub struct Frustum {
    planes: [Plane; 6], // left, right, top, bottom, near, far
}

impl Frustum {
    pub fn cull_sphere(&self, center: Vec3, radius: f32) -> bool {
        for plane in &self.planes {
            if plane.distance_to(center) < -radius {
                return true; // Outside frustum
            }
        }
        false
    }
    
    pub fn cull_aabb(&self, min: Vec3, max: Vec3) -> bool {
        // Test AABB against all 6 planes
        // (implementation details...)
    }
}
```

**Usage**:
```rust
for entity in entities {
    if frustum.cull_sphere(entity.position, entity.radius) {
        continue; // Skip this entity
    }
    render(entity);
}
```

### 2. Occlusion Culling (GPU Path)

**GPU**: Use occlusion queries
```rust
// Render bounding boxes to depth buffer
// Query how many pixels passed
if pixels_visible == 0 {
    skip_object();
}
```

**Software fallback**: Portal-based or PVS

### 3. PVS (Potentially Visible Set) - Quake Method

**Concept**: Pre-compute which rooms are visible from each room.

#### Build Time (Map Compiler)

```rust
pub struct PVSBuilder {
    rooms: Vec<Room>,
}

impl PVSBuilder {
    pub fn compute_pvs(&self) -> PVSData {
        let mut pvs = PVSData::new(self.rooms.len());
        
        for room_a in &self.rooms {
            for room_b in &self.rooms {
                if self.can_see_room(room_a, room_b) {
                    pvs.mark_visible(room_a.id, room_b.id);
                }
            }
        }
        
        pvs.compress(); // Run-length encoding
        pvs
    }
    
    fn can_see_room(&self, from: &Room, to: &Room) -> bool {
        // Cast rays through portals
        // If any ray reaches target room, mark visible
        for portal in &from.portals {
            if self.raycast_through_portal(portal, to) {
                return true;
            }
        }
        false
    }
}
```

#### Runtime (Game Engine)

```rust
pub struct PVSData {
    visibility_matrix: CompressedBitset, // rooms x rooms
}

impl PVSData {
    pub fn get_visible_rooms(&self, current_room: RoomId) -> &[RoomId] {
        &self.visibility_matrix[current_room]
    }
}

// In render loop:
let current_room = world.get_room_containing(camera.position);
let visible_rooms = pvs.get_visible_rooms(current_room);

for room_id in visible_rooms {
    for entity in world.get_entities_in_room(room_id) {
        if frustum.cull(entity) { continue; }
        render(entity);
    }
}
```

**Memory**: Quake 3 used ~10KB for PVS data per map. Highly compressed.

---

## SIMD Optimization

### SSE2 (x86/x64)

```rust
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

#[target_feature(enable = "sse2")]
unsafe fn transform_vertices_sse2(
    vertices: &[Vertex],
    mvp: &Mat4,
    output: &mut [Vec4],
) {
    // Process 4 vertices at once
    for chunk in vertices.chunks_exact(4) {
        let x = _mm_set_ps(chunk[3].x, chunk[2].x, chunk[1].x, chunk[0].x);
        let y = _mm_set_ps(chunk[3].y, chunk[2].y, chunk[1].y, chunk[0].y);
        let z = _mm_set_ps(chunk[3].z, chunk[2].z, chunk[1].z, chunk[0].z);
        
        // Matrix multiply (4 vertices in parallel)
        // ... SIMD math ...
    }
}
```

### NEON (ARM)

```rust
#[cfg(target_arch = "aarch64")]
use std::arch::aarch64::*;

#[target_feature(enable = "neon")]
unsafe fn transform_vertices_neon(/* ... */) {
    // Similar to SSE2, using NEON intrinsics
}
```

**Portable fallback**: Use `std::simd` (portable SIMD) when stabilized, or `packed_simd` crate.

---

## Performance Targets

### Low-Poly Scene (Quake 3 style)

| Hardware | Triangles/Frame | Target FPS | Strategy |
|----------|-----------------|------------|----------|
| Pentium III 450 MHz | 5,000 | 60 FPS | Software + PVS |
| Modern laptop (no GPU) | 20,000 | 60 FPS | Software + SIMD |
| Raspberry Pi 4 | 10,000 | 30-60 FPS | Software + NEON |
| Any GPU (even old) | 50,000+ | 60 FPS | GPU path |

### Budget Breakdown (16.6ms frame @ 60 FPS)

```
Software rasterizer (Pentium III target):
- Frustum culling:    0.2ms  (1000 objects)
- PVS lookup:         0.1ms  (constant time)
- Vertex transform:   2.0ms  (5000 vertices)
- Rasterization:     10.0ms  (5000 tris, ~200 pixels each)
- Fragment shading:   3.0ms  (simple texture + lighting)
- Frame present:      0.5ms
Total:               15.8ms  ✅ Under budget
```

---

## Implementation Phases

### Phase 0-1 (Current)
- ✅ GPU path only (wgpu)
- ⏸️ Software rasterizer deferred

### Phase 2 (Rendering System)
- [ ] Implement basic software rasterizer
  - Scanline triangle fill
  - Flat shading
  - Frustum culling
- [ ] Benchmark: 5k triangles @ 60 FPS on modern CPU
- [ ] Effect IR: Compile to Rust functions

### Phase 3 (Optimization)
- [ ] SIMD optimization (SSE2/NEON)
- [ ] Tile-based rasterization
- [ ] Texture mapping
- [ ] PVS system (build tool + runtime)
- [ ] Benchmark: Match Quake 3 performance

### Phase 4 (Polish)
- [ ] Occlusion queries (GPU path)
- [ ] Portal rendering
- [ ] LOD system integration
- [ ] Profiling tools (show bottlenecks)

---

## Reference Material

### Quake 3 Renderer Analysis

**Key techniques Latch will adopt**:
1. **PVS**: Pre-baked visibility (build-time ray casting)
2. **Frustum culling**: Every frame, every object
3. **Portal rendering**: Indoor scenes split by portals
4. **Batching**: Merge draw calls for same material
5. **Texture atlases**: Reduce texture switches
6. **LOD**: Far objects use lower poly models

**Quake 3 stats** (1999):
- 5,000-10,000 tris/frame (typical map)
- 60 FPS on Pentium III 450 MHz (software mode)
- 125 FPS on Pentium III 450 MHz + Voodoo 3 (GPU)
- PVS reduced rendered tris by ~70% (from 30k potential to 10k actual)

### Resources

- **Quake 3 Source Code**: https://github.com/id-Software/Quake-III-Arena
- **Michael Abrash's Graphics Programming Black Book**: Chapter on Quake/Doom rendering
- **Scratchapixel**: Software rasterization tutorial
- **Fabian Giesen**: Software rasterizer series

---

## Design Decisions

### Why Not Just Require a GPU?

1. **Headless servers**: MMO servers don't have GPUs
2. **Embedded devices**: Raspberry Pi, industrial systems
3. **Broken drivers**: GPU driver bugs are common
4. **Testing**: Automated tests shouldn't need GPU access
5. **Accessibility**: Not everyone has modern hardware

### Why Quake 3 as Benchmark?

1. **Proven**: Ran on real hardware, not theoretical
2. **Sufficient fidelity**: Low-poly aesthetic is popular (PSX-style games)
3. **Attainable**: Modern CPUs are 100x faster than Pentium III
4. **Well-documented**: Source code available, techniques known

### Why Not Just Use SwiftShader/WARP?

**SwiftShader** (Chrome's software renderer):
- ✅ Full Vulkan compliance
- ❌ Bloated (~50 MB binary)
- ❌ JIT compiler (forbidden on iOS/consoles)
- ❌ Targets modern shaders, slow for simple scenes

**WARP** (Windows software renderer):
- ✅ Full DirectX 11 compliance
- ❌ Windows-only
- ❌ Large runtime dependency
- ❌ Targets modern shaders

**Latch software rasterizer**:
- ✅ Cross-platform (Rust + SIMD)
- ✅ Tiny (~100 KB binary)
- ✅ No JIT (AOT compiled)
- ✅ Optimized for low-poly scenes
- ✅ Compiles from same Effect IR as GPU path

---

## Summary

The software rasterizer is **not a fallback for modern games**. It's a **first-class citizen for retro-style games**.

**Philosophy**: If your game looks like Quake 3, it should run like Quake 3.

**Implementation**: Dual-compile Effect IR to GPU shaders and CPU functions. Use Quake's PVS + frustum culling to keep triangle counts low. SIMD optimization for transform/rasterization.

**Target**: 60 FPS on 5-10k tris/frame, even on CPUs without dedicated GPUs.
