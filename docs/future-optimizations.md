# Future Optimization Notes

**Last Updated:** November 8, 2025

This document tracks potential performance optimizations that are NOT priorities for Phase 0, but may be valuable in later development phases.

## Low Priority (Phase 1+)

### Rust Runtime Overhead

**Context:** With 100k entities, PoC 2 shows:
- **Total RAM:** ~477 MB
- **ECS data:** ~13 MB (entities + components)
- **VRAM estimate:** ~7.5 MB (vertex buffer)
- **Unaccounted:** ~456 MB (wgpu internals, winit, Rust runtime, OS buffers)

**Observations:**
- Most overhead comes from rendering stack (wgpu, winit, window surfaces)
- We plan to replace wgpu with our own IR layer eventually
- Rust runtime itself is relatively small (~10-30 MB)

**Potential Actions (very late in development):**
1. Profile with custom allocator to identify large allocations
2. Consider `no_std` for headless server builds (removes stdlib overhead)
3. Use `jemalloc` or `mimalloc` instead of system allocator
4. Strip debug symbols and unused dependencies in release builds
5. LTO (Link-Time Optimization) and aggressive optimization flags

**Expected Gains:** 10-50 MB reduction (2-10% of total)

**Priority:** **VERY LOW** - Only consider after Phase 2+ when we have our own rendering IR and need to squeeze every MB for console/mobile targets.

---

## Why These Are Low Priority

### 1. **Premature Optimization**
We're in Phase 0 (proof-of-concept). Focus on:
- Architecture validation
- Feature completeness
- Developer experience

NOT on squeezing out megabytes of RAM.

### 2. **wgpu Is Temporary**
Current memory breakdown:
- wgpu + winit: ~50-100 MB
- Window surfaces (Retina): ~15-30 MB
- Vertex staging buffers: ~12 MB

**Total wgpu stack: ~100-150 MB (60-80% of overhead)**

Once we implement our own rendering IR (Phase 1+), this goes away. Optimizing wgpu usage now would be wasted effort.

### 3. **477 MB Is Not A Problem**
For desktop targets (Phase 0):
- Modern PCs have 8-32 GB RAM
- 477 MB for 100k entities is reasonable
- Most games use 1-4 GB RAM

For mobile/console (Phase 2+):
- We'll have tighter budgets (512 MB - 2 GB)
- Will need aggressive optimization then
- But not now

### 4. **Measurement Uncertainty**
The `ps` command reports RSS (Resident Set Size), which includes:
- Shared libraries (counted multiple times)
- Memory mapped files
- OS kernel buffers
- Page tables

Actual "owned" memory is likely lower. We'd need more precise instrumentation (custom allocator, Valgrind/Instruments) to know what's truly needed.

---

## When To Revisit

Reconsider these optimizations if:

1. **Shipping to consoles** - Strict memory budgets (512 MB - 2 GB total)
2. **Mobile targets** - iOS/Android have tighter limits
3. **Web (WASM)** - Browser heap limits (1-4 GB)
4. **Running on "toaster" hardware** - Our Quake 3 performance target
5. **MMO server** - Need to pack 10k+ entities per node

Until then, focus on:
- Feature completeness
- Developer ergonomics
- Architectural soundness

---

## Related Documents

- `docs/poc2-gpu-buffer-overflow.md` - VRAM budget and vertex buffer sizing
- `docs/architecture.md` - Overall engine architecture
- `.github/copilot-instructions.md` - Performance targets and constraints

---

## Measurement Baseline (PoC 2, 100k entities)

```
=== Performance Metrics ===
FPS: 68.8 (14.54 ms avg, 7.06-29.52 ms range)
RAM: 476.80 MB | VRAM: 7.50 MB (estimate)
Entities: 100000 (1 archetypes, 13.16 MB components)
System Timings:
  render: 9.42 ms (64.8%)
  physics: 4.84 ms (33.3%)
```

**Breakdown:**
- ECS data: 13.16 MB (2.8% of RAM)
- Vertex buffer (VRAM): 7.50 MB
- Rendering stack overhead: ~50-100 MB (wgpu internals)
- Window surfaces (Retina): ~15-30 MB
- Rust runtime + libs: ~10-30 MB
- OS/kernel: ~50-100 MB
- **Total: 477 MB**

**Target for Phase 2 (ship builds):**
- Remove wgpu: -100 MB
- Remove winit (custom window): -20 MB
- Optimize Rust runtime: -10 MB
- **Estimated: ~350 MB** for 100k entities

Still plenty of headroom for consoles (512 MB - 2 GB budgets).
