# PoC 2: GPU Buffer Overflow Incident

**Date:** November 8, 2025  
**Issue:** Runtime crash when scaling from 100 to 1,000 triangles  
**Root Cause:** Static GPU buffer allocation

## The Crash

```
wgpu error: Validation Error

Caused by:
  In Queue::write_buffer
    Copy of 0..60000 would end up overrunning the bounds of the Destination buffer of size 6000
```

## Why This Failed

### The Math

**Buffer size:**
- Allocated: `sizeof(Vertex) * 3 * 100` = `20 * 3 * 100` = **6,000 bytes**
- Each `Vertex` = 2 floats (position) + 3 floats (color) = 20 bytes
- Each triangle = 3 vertices

**Data written:**
- Actual: `sizeof(Vertex) * 3 * 1000` = **60,000 bytes**
- Tried to write 10× more data than buffer capacity

**Result:** wgpu validation caught the overflow and panicked.

## Why Rust Didn't Prevent This

This is **not a Rust safety violation**. Here's the distinction:

### ✅ What Rust's Type System Guarantees

Rust prevents **memory unsafety** within its own type system:

```rust
// Compile error: Index out of bounds (if constant)
let arr = [1, 2, 3];
let x = arr[10]; // ❌ Compile error

// Runtime panic: Dynamic index out of bounds
let idx = user_input();
let x = arr[idx]; // ⚠️ Runtime bounds check → panic

// Memory safety: Ownership prevents use-after-free
let vec = vec![1, 2, 3];
drop(vec);
let x = vec[0]; // ❌ Compile error: use after move
```

### ⚠️ What Rust Cannot Guarantee

**External API contracts** (GPU, OS, hardware) require runtime validation:

```rust
// GPU buffer is an opaque handle
let buffer = device.create_buffer(&desc); // Returns opaque GPU handle

// Buffer size is tracked by GPU driver, not Rust type system
queue.write_buffer(&buffer, 0, &data); // Runtime validation needed
```

**Why?**

1. **Buffer size is dynamic** - Determined at runtime via API call
2. **GPU memory is opaque** - Managed by driver, not Rust's allocator  
3. **Cross-process boundary** - GPU driver is separate process/kernel module
4. **Hardware limits vary** - Different GPUs have different VRAM/capabilities

Rust's borrow checker can't track GPU state because:
- GPU memory lives outside Rust's heap
- Buffer handles are just integers (opaque IDs)
- Actual memory is managed by OS/driver

## Why `panic!` Instead of `Result`?

`wgpu` distinguishes between **programmer errors** vs. **runtime errors**:

### Programmer Errors → `panic!`

These are **bugs** that should be caught during development:

```rust
queue.write_buffer(&buffer, 0, &too_much_data); // Buffer overflow
pipeline.draw(0..999999999); // Invalid draw call
```

**Rationale:** Like array indexing, these are logic errors. The program is in an invalid state and should crash immediately to prevent data corruption.

### Runtime Errors → `Result`

These are **expected failures** that can happen in production:

```rust
device.create_buffer(&desc)?; // Out of VRAM
adapter.request_device()?;    // GPU lost/disconnected
surface.get_current_texture()?; // Window minimized
```

**Rationale:** These failures are recoverable - retry, fallback, or gracefully degrade.

### Standard Library Precedent

This matches Rust's standard library:

```rust
vec[100];           // panic! (programmer error)
File::open(path)?;  // Result (runtime error)

vec.get(100);       // Option (safe alternative)
queue.write_buffer_checked(); // Could exist but doesn't (redundant validation)
```

## The Fix

Changed from **static allocation** to **dynamic growth**:

### Before (Static - Fragile)

```rust
let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
    size: (std::mem::size_of::<Vertex>() * 3 * 100) as u64, // Hard-coded limit
    // ...
});
```

**Problem:** Crashes if entity count exceeds 100.

### After (Dynamic - Robust)

```rust
struct TriangleRenderer {
    vertex_buffer: wgpu::Buffer,
    vertex_buffer_capacity: usize, // Track current capacity
}

fn render(&mut self, world: &World) {
    let mut vertices = Vec::new();
    // ... build vertex data ...
    
    let triangle_count = vertices.len() / 3;
    if triangle_count > self.vertex_buffer_capacity {
        // Grow buffer by doubling (like Vec)
        let new_capacity = triangle_count.next_power_of_two();
        
        self.vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            size: (std::mem::size_of::<Vertex>() * 3 * new_capacity) as u64,
            // ...
        });
        self.vertex_buffer_capacity = new_capacity;
    }
    
    queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
}
```

**Benefits:**
- ✅ Handles any entity count
- ✅ Grows exponentially (amortized O(1) like `Vec`)
- ✅ Only reallocates when needed
- ✅ Prints debug message when resizing

## Performance Implications

### Growth Strategy

Using `next_power_of_two()` ensures logarithmic reallocations:

```
Entities: 100 → Buffer: 128 triangles (6,144 bytes)
Entities: 200 → Buffer: 256 triangles (12,288 bytes) [resize!]
Entities: 500 → Buffer: 512 triangles (24,576 bytes) [resize!]
Entities: 1,000 → Buffer: 1,024 triangles (49,152 bytes) [resize!]
Entities: 2,000 → Buffer: 2,048 triangles (98,304 bytes) [resize!]
Entities: 10,000 → Buffer: 16,384 triangles (786,432 bytes) [resize!]
```

**Reallocation count:** O(log n) where n = max entity count

### Frame Time Impact

- **Buffer creation:** ~0.1-1ms depending on size
- **Happens rarely:** Only when crossing power-of-two threshold
- **Amortized cost:** Negligible over application lifetime

### Alternative Strategies

1. **Pre-allocate maximum:**
   ```rust
   const MAX_TRIANGLES: usize = 100_000;
   size: (std::mem::size_of::<Vertex>() * 3 * MAX_TRIANGLES) as u64,
   ```
   - Pro: Zero runtime allocations
   - Con: Wastes VRAM for small scenes

2. **Pool of buffers:**
   ```rust
   let buffers = vec![
       create_buffer(1024),
       create_buffer(4096),
       create_buffer(16384),
   ];
   ```
   - Pro: No buffer creation during gameplay
   - Con: Complex to manage, still wastes VRAM

3. **Dynamic with hysteresis:**
   ```rust
   if triangle_count > capacity {
       resize(triangle_count * 2); // Grow aggressively
   } else if triangle_count < capacity / 4 {
       resize(capacity / 2); // Shrink conservatively
   }
   ```
   - Pro: Reclaims VRAM when entity count drops
   - Con: More complex, potential thrashing

**Chosen:** Simple exponential growth (matches Rust's `Vec`).

## Lessons Learned

### 1. External APIs Have Runtime Contracts

Rust guarantees memory safety **within the type system**, but can't validate:
- GPU buffer sizes
- File sizes
- Network packet sizes  
- OS resource limits

These require **runtime checks** or **defensive programming**.

### 2. Panics Are Not Always Bad

`panic!` is appropriate for:
- **Invariant violations** (logic bugs)
- **Unrecoverable errors** (data corruption)
- **Development-time issues** (should be fixed before shipping)

`Result` is appropriate for:
- **Expected failures** (file not found, network timeout)
- **Recoverable errors** (can retry or fallback)
- **Production issues** (user error, environmental)

### 3. Capacity Planning Matters

Even with dynamic allocation, consider:
- **Upper bounds:** Is 1 million entities reasonable?
- **Growth strategy:** Linear vs. exponential vs. pre-allocated
- **Memory budget:** Does doubling fit in VRAM?
- **Frame budget:** Can you afford reallocation spikes?

### 4. GPU Validation Is Your Friend

wgpu's validation layers catch:
- Buffer overflows
- Invalid pipeline state
- Mismatched bind groups
- Out-of-bounds draws

**Enable in development,** disable in production for performance:

```rust
let device = adapter.request_device(&wgpu::DeviceDescriptor {
    #[cfg(debug_assertions)]
    required_features: wgpu::Features::DEBUG_ADAPTER,
    // ...
}).await?;
```

## References

- **Code:** `crates/latch_runtime/examples/poc2_moving_triangles.rs` (lines 119, 243-270)
- **wgpu docs:** https://docs.rs/wgpu/latest/wgpu/
- **Rust panic vs Result:** https://doc.rust-lang.org/book/ch09-00-error-handling.html

## Action Items

- [ ] Add `MAX_ENTITIES` constant and enforce during spawn
- [ ] Add telemetry for buffer resizing (count, peak size)
- [ ] Document GPU memory budget in engine docs
- [ ] Consider pre-allocation strategy for production builds
- [ ] Add stress test: spawn/despawn entities to test reallocation
