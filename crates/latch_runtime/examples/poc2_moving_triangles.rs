// PoC 2: Minimal ECS + Determinism
//
// Goals:
// - Spawn entities with Position + Velocity components
// - Fixed 60Hz physics system
// - Render triangles at entity positions
// - Record inputs and prove deterministic replay
//
// Success Criteria:
// - 100 moving triangles at 60 FPS
// - Same inputs → same positions after 1000 frames
// - Visual confirmation of replay matching original

use latch_core::ecs::{World, Component};
use latch_core::define_component;
use latch_core::time::{SimulationTime, InputRecorder, TickInput, TICK_DURATION_SECS};
use latch_core::spawn;
use latch_metrics::{FrameTimer, SystemProfiler};

use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

use wgpu;
use wgpu::util::DeviceExt;
use std::sync::Arc;

// ============================================================================
// Render Timings
// ============================================================================

#[derive(Default, Clone, Copy)]
struct RenderTimings {
    acquire_texture_us: u64,
    build_instances_us: u64,
    upload_instances_us: u64,
    update_uniforms_us: u64,
    encode_commands_us: u64,
    submit_us: u64,
    present_us: u64,
}

// ============================================================================
// Components
// ============================================================================

// Integer-based physics with fixed-point precision
// - 1 game unit = 10 micrometers (0.00001 meters)
// - Position: i32 → range ±21,474,836 units = ±214.7 meters
// - Velocity: i16 → range ±32,767 units/tick = ±327mm/tick = 19.6 m/s max
// - NDC mapping: 1.0 NDC = 1,000,000 units = 10 meters

const UNITS_PER_METER: i32 = 100_000; // 10 micrometers precision
const UNITS_PER_NDC: i32 = 10 * UNITS_PER_METER; // 1 NDC = 10 meters

#[derive(Clone, Copy, Debug)]
struct Position {
    x: i32,  // Fixed-point: units (10 µm precision)
    y: i32,
}
define_component!(Position, 1, "Position");

#[derive(Clone, Copy, Debug)]
struct Velocity {
    x: i16,  // Fixed-point: units per tick (10 µm precision)
    y: i16,
}
define_component!(Velocity, 2, "Velocity");

#[derive(Clone, Copy, Debug)]
struct Color {
    r: u8,   // 0-255
    g: u8,
    b: u8,
}
define_component!(Color, 3, "Color");

// ============================================================================
// Systems
// ============================================================================

fn physics_system(world: &mut World, _dt: f32) {
    use rayon::prelude::*;
    use latch_core::{columns, columns_mut};
    
    // Integer-based deterministic physics with double-buffering!
    // - Read from "current" buffer (stable state from last tick)
    // - Write to "next" buffer (new state for next tick)
    // - No floating-point, no drift, perfect replay.
    
    world.par_for_each(&[Position::ID, Velocity::ID], |storage| {
        // Read from "current" buffer, write to "next" buffer
        let (pos_read, vel_read) = columns!(storage, Position, Velocity);
        let (pos_write, vel_write) = columns_mut!(storage, Position, Velocity);
        
        // Parallel iteration: read old state, write new state
        pos_write.par_iter_mut()
            .zip(vel_write.par_iter_mut())
            .zip(pos_read.par_iter())
            .zip(vel_read.par_iter())
            .for_each(|(((pos_out, vel_out), pos_in), vel_in)| {
                // Update position (pure integer arithmetic!)
                pos_out.x = pos_in.x + vel_in.x as i32;
                pos_out.y = pos_in.y + vel_in.y as i32;
                
                // Copy velocity for next iteration
                vel_out.x = vel_in.x;
                vel_out.y = vel_in.y;
                
                // Bounce off edges (NDC bounds: ±1,000,000 units = ±10 meters)
                let bound = UNITS_PER_NDC;
                if pos_out.x < -bound || pos_out.x > bound {
                    pos_out.x = pos_out.x.clamp(-bound, bound);
                    vel_out.x = vel_out.x.saturating_neg(); // Handle i16::MIN overflow
                }
                if pos_out.y < -bound || pos_out.y > bound {
                    pos_out.y = pos_out.y.clamp(-bound, bound);
                    vel_out.y = vel_out.y.saturating_neg(); // Handle i16::MIN overflow
                }
            });
    });
}

// ============================================================================
// Renderer
// ============================================================================

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
}

// Split instance data into STATIC (uploaded once) and DYNAMIC (uploaded every tick)

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct InstanceStatic {
    color: [u8; 4],          // 4 bytes - RGB + padding (u8: 0-255)
}
// Total: 4 bytes per instance - uploaded ONCE at startup!

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct InstanceDynamic {
    position: [i32; 2],      // 8 bytes - integer game units (10 µm precision)
    velocity: [i16; 2],      // 4 bytes - MUST update when bouncing!
}
// Total: 12 bytes per instance - uploaded every physics tick (60 Hz)
// 5M triangles = 60 MB per upload
// GPU converts i32 → NDC in shader (zero CPU conversion cost!)

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    interpolation_alpha: f32,
    dt: f32,
    _padding: [f32; 2], // Align to 16 bytes
}

struct TriangleRenderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    #[allow(dead_code)]
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    instance_static_buffer: wgpu::Buffer,   // Velocity + Color (uploaded ONCE)
    instance_dynamic_buffer: wgpu::Buffer,  // Position (uploaded every tick)
    instance_buffer_capacity: usize,
    last_instance_count: usize, // Track actual instances uploaded
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    last_physics_tick: u64,
}

impl TriangleRenderer {
    async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        
        let surface = instance.create_surface(window.clone()).unwrap();
        
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();
        
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await
            .unwrap();
        
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps.formats[0];
        
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Immediate, // No V-Sync (Mailbox not supported on macOS)
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Triangle Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/triangle.wgsl").into()),
        });
        
        // Create uniform buffer for interpolation data
        let uniforms = Uniforms {
            interpolation_alpha: 0.0,
            dt: TICK_DURATION_SECS,
            _padding: [0.0, 0.0],
        };
        
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Uniform Buffer"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        
        // Create bind group layout for uniforms
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Uniform Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        
        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Uniform Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[
                    // Vertex buffer (base triangle shape)
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x2],
                    },
                    // Instance buffer 1: DYNAMIC data (position + velocity - updated every tick)
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<InstanceDynamic>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![
                            1 => Sint32x2,   // position (i32x2 game units - converted in shader)
                            2 => Snorm16x2,  // velocity (i16x2 - changes on bounce!)
                        ],
                    },
                    // Instance buffer 2: STATIC data (color only - uploaded once)
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<InstanceStatic>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![
                            3 => Unorm8x4    // color (u8x4 normalized to 0.0-1.0)
                        ],
                    },
                ],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });
        
        // Create static vertex buffer with base triangle shape (centered at origin)
        let size = 0.02;
        let triangle_vertices = [
            Vertex { position: [0.0, size] },       // Top
            Vertex { position: [-size, -size] },    // Bottom left
            Vertex { position: [size, -size] },     // Bottom right
        ];
        
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(&triangle_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        
        // Create instance buffers (will grow as needed)
        let initial_capacity = 1024;
        
        // Static buffer: Velocity + Color (uploaded ONCE)
        let instance_static_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Instance Static Buffer"),
            size: (std::mem::size_of::<InstanceStatic>() * initial_capacity) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        
        // Dynamic buffer: Position (uploaded every tick)
        let instance_dynamic_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Instance Dynamic Buffer"),
            size: (std::mem::size_of::<InstanceDynamic>() * initial_capacity) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        
        Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            vertex_buffer,
            instance_static_buffer,
            instance_dynamic_buffer,
            instance_buffer_capacity: initial_capacity,
            last_instance_count: 0,
            uniform_buffer,
            uniform_bind_group,
            last_physics_tick: 0,
        }
    }
    
    fn render(&mut self, world: &World, tick: u64, interpolation_alpha: f32) -> Result<(bool, usize, RenderTimings), wgpu::SurfaceError> {
        let mut timings = RenderTimings::default();
        
        let instance_count;
        let mut uploaded = false;
        
        // Only rebuild and upload DYNAMIC data when physics ticks (not every render frame!)
        if tick != self.last_physics_tick {
            self.last_physics_tick = tick;
            uploaded = true;
            
            let build_start = std::time::Instant::now();
            
            // Micro-benchmark: timing each phase
            let mut bench_reserve_us = 0u64;
            let mut bench_copy_us = 0u64;
            
            // Build position + velocity (DYNAMIC) and color (STATIC)
            let mut dynamic_data: Vec<InstanceDynamic> = Vec::new();
            let mut static_data_built = self.last_instance_count > 0;
            let mut static_data: Vec<InstanceStatic> = Vec::new();
            
            // PHASE 1: Query archetypes
            let query_start = std::time::Instant::now();
            let position_archs = world.archetypes_with(Position::ID);
            let velocity_archs = world.archetypes_with(Velocity::ID);
            let color_archs = world.archetypes_with(Color::ID);
            let bench_query_us = query_start.elapsed().as_micros() as u64;
            
            // PHASE 2 & 3: Reserve + Copy loop
            for &arch_id in position_archs {
                if !velocity_archs.contains(&arch_id) || !color_archs.contains(&arch_id) {
                    continue;
                }
                
                if let (Some(positions), Some(velocities), Some(colors)) = (
                    world.column::<Position>(arch_id),
                    world.column::<Velocity>(arch_id),
                    world.column::<Color>(arch_id),
                ) {
                    // PHASE 2: Reserve space
                    let reserve_start = std::time::Instant::now();
                    let count = positions.len();
                    dynamic_data.reserve(count);
                    if !static_data_built {
                        static_data.reserve(count);
                    }
                    bench_reserve_us += reserve_start.elapsed().as_micros() as u64;
                    
                    // PHASE 3: Copy data using unsafe direct memory operations
                    // This eliminates Vec::push bounds checks (5M × 2 = 10M checks!)
                    let copy_start = std::time::Instant::now();
                    
                    unsafe {
                        // Get raw pointers to the end of our vectors
                        let dynamic_ptr = dynamic_data.as_mut_ptr().add(dynamic_data.len());
                        let dynamic_start_len = dynamic_data.len();
                        
                        // Copy dynamic data (position + velocity)
                        for i in 0..count {
                            std::ptr::write(
                                dynamic_ptr.add(i),
                                InstanceDynamic {
                                    position: [positions[i].x, positions[i].y],
                                    velocity: [velocities[i].x, velocities[i].y],
                                }
                            );
                        }
                        dynamic_data.set_len(dynamic_start_len + count);
                        
                        // Copy static data (color only, first time)
                        if !static_data_built {
                            let static_ptr = static_data.as_mut_ptr().add(static_data.len());
                            let static_start_len = static_data.len();
                            
                            for i in 0..count {
                                std::ptr::write(
                                    static_ptr.add(i),
                                    InstanceStatic {
                                        color: [colors[i].r, colors[i].g, colors[i].b, 0],
                                    }
                                );
                            }
                            static_data.set_len(static_start_len + count);
                        }
                    }
                    
                    bench_copy_us += copy_start.elapsed().as_micros() as u64;
                }
            }
            
            instance_count = dynamic_data.len();
            self.last_instance_count = instance_count;
            
            timings.build_instances_us = build_start.elapsed().as_micros() as u64;
            
            // Print micro-benchmark results every 60 ticks (~1 second)
            if tick % 60 == 0 {
                println!("build_instances breakdown ({}K instances):", instance_count / 1000);
                println!("  Query:   {:6} µs ({:5.1}%)", bench_query_us, 
                    (bench_query_us as f64 / timings.build_instances_us as f64) * 100.0);
                println!("  Reserve: {:6} µs ({:5.1}%)", bench_reserve_us,
                    (bench_reserve_us as f64 / timings.build_instances_us as f64) * 100.0);
                println!("  Copy:    {:6} µs ({:5.1}%)", bench_copy_us,
                    (bench_copy_us as f64 / timings.build_instances_us as f64) * 100.0);
                println!("  TOTAL:   {:6} µs", timings.build_instances_us);
            }
            
            // Resize buffers if needed
            if instance_count > self.instance_buffer_capacity {
                let new_capacity = instance_count.next_power_of_two();
                println!("Resizing instance buffers: {} → {} instances", 
                    self.instance_buffer_capacity, new_capacity);
                println!("  Static:  {} KB ({} bytes/instance)", 
                    (new_capacity * std::mem::size_of::<InstanceStatic>()) / 1024,
                    std::mem::size_of::<InstanceStatic>());
                println!("  Dynamic: {} KB ({} bytes/instance)", 
                    (new_capacity * std::mem::size_of::<InstanceDynamic>()) / 1024,
                    std::mem::size_of::<InstanceDynamic>());
                
                self.instance_static_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Instance Static Buffer"),
                    size: (std::mem::size_of::<InstanceStatic>() * new_capacity) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                
                self.instance_dynamic_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Instance Dynamic Buffer"),
                    size: (std::mem::size_of::<InstanceDynamic>() * new_capacity) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                
                self.instance_buffer_capacity = new_capacity;
                static_data_built = false; // Force rebuild of static data with new buffer
            }
            
            // Upload instance data to GPU
            let upload_start = std::time::Instant::now();
            
            // Upload STATIC data (first tick only or after resize)
            if !static_data_built && !static_data.is_empty() {
                self.queue.write_buffer(
                    &self.instance_static_buffer,
                    0,
                    bytemuck::cast_slice(&static_data),
                );
            }
            
            // Upload DYNAMIC data (every physics tick!)
            if !dynamic_data.is_empty() {
                self.queue.write_buffer(
                    &self.instance_dynamic_buffer,
                    0,
                    bytemuck::cast_slice(&dynamic_data),
                );
            }
            
            timings.upload_instances_us = upload_start.elapsed().as_micros() as u64;
        } else {
            // Not a physics tick - use the last uploaded instance count
            instance_count = self.last_instance_count;
        }
        
        // Update uniforms every frame with interpolation alpha
        let uniform_start = std::time::Instant::now();
        let uniforms = Uniforms {
            interpolation_alpha,
            dt: TICK_DURATION_SECS,
            _padding: [0.0, 0.0],
        };
        self.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[uniforms]),
        );
        timings.update_uniforms_us = uniform_start.elapsed().as_micros() as u64;
        
        let acquire_start = std::time::Instant::now();
        let output = self.surface.get_current_texture()?;
        timings.acquire_texture_us = acquire_start.elapsed().as_micros() as u64;
        
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        
        let encode_start = std::time::Instant::now();
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });
        
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.1,
                            b: 0.1,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            
            render_pass.set_pipeline(&self.pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_vertex_buffer(1, self.instance_dynamic_buffer.slice(..));  // Position
            render_pass.set_vertex_buffer(2, self.instance_static_buffer.slice(..));   // Velocity + Color
            render_pass.draw(0..3, 0..(instance_count as u32)); // 3 vertices, N instances
        }
        
        timings.encode_commands_us = encode_start.elapsed().as_micros() as u64;
        
        let submit_start = std::time::Instant::now();
        self.queue.submit(std::iter::once(encoder.finish()));
        timings.submit_us = submit_start.elapsed().as_micros() as u64;
        
        let present_start = std::time::Instant::now();
        output.present();
        timings.present_us = present_start.elapsed().as_micros() as u64;
        
        Ok((uploaded, instance_count, timings))
    }
}

// ============================================================================
// Application
// ============================================================================

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<TriangleRenderer>,
    world: World,
    time: SimulationTime,
    recorder: InputRecorder,
    mouse_pos: (f32, f32),
    mouse_pressed: bool,
    mode: Mode,
    frame_timer: FrameTimer,
    profiler: SystemProfiler,
    last_print: std::time::Instant,
    frame_count: u64,
    total_bytes_uploaded: u64,
    render_timings: RenderTimings,
    render_frame_count: u64,
}

enum Mode {
    Recording,
    Replaying,
}

impl App {
    fn new() -> Self {
        let mut world = World::new();
        
        // Spawn triangles with random positions and velocities
        use std::f32::consts::PI;
        let num_triangles = 5_000_000; // 5 MILLION TRIANGLES!
        let f_num_triangles = num_triangles as f32;
        for i in 0..num_triangles {
            let angle = (i as f32 / f_num_triangles) * 2.0 * PI;
            
            // Distribute triangles across the visible area
            // radius: 0.3-0.9 NDC = 3-9 meters = 300,000-900,000 units
            let radius_ndc = 0.3 + (i as f32 / f_num_triangles) * 0.6;
            let radius_units = (radius_ndc * UNITS_PER_NDC as f32) as i32;

            // Position in game units (integer from the start!)
            let pos = Position {
                x: ((angle.cos() * radius_units as f32) as i32),
                y: ((angle.sin() * radius_units as f32) as i32),
            };
            
            // Velocity: tangent to circle, moderate speed
            // Target: ~3.0 meters/sec = 3.0 * 100,000 units/sec = 300,000 units/sec
            // At 60 Hz: 300,000 / 60 = 5,000 units/tick (well within i16 range ±32,767)
            let speed_units_per_tick = 5000;
            let vel = Velocity {
                x: ((angle + PI / 2.0).cos() * speed_units_per_tick as f32) as i16,
                y: ((angle + PI / 2.0).sin() * speed_units_per_tick as f32) as i16,
            };
            
            let color = Color {
                r: ((i % 100) * 255 / 100) as u8,
                g: (255 - (i % 100) * 255 / 100) as u8,
                b: 128,
            };
            
            spawn!(world, pos, vel, color);
        }
        
        let mut recorder = InputRecorder::new();
        recorder.start_recording();
        
        Self {
            window: None,
            renderer: None,
            world,
            time: SimulationTime::new(),
            recorder,
            mouse_pos: (0.0, 0.0),
            mouse_pressed: false,
            mode: Mode::Recording,
            frame_timer: FrameTimer::new(60),
            profiler: SystemProfiler::new(),
            last_print: std::time::Instant::now(),
            frame_count: 0,
            total_bytes_uploaded: 0,
            render_timings: RenderTimings::default(),
            render_frame_count: 0,
        }
    }
    
    fn tick(&mut self) {
        // Begin frame timing
        self.frame_timer.begin();
        
        // Record input
        let input = TickInput {
            tick: self.time.tick_count(),
            mouse_x: self.mouse_pos.0,
            mouse_y: self.mouse_pos.1,
            mouse_pressed: self.mouse_pressed,
        };
        self.recorder.record(input);
        
        // Run physics (writes to "next" buffer)
        self.profiler.time_system("physics", || {
            physics_system(&mut self.world, TICK_DURATION_SECS);
        });
        
        // Swap buffers: make "next" become "current" for the next tick
        // This ensures deterministic parallel updates
        self.world.swap_buffers();
        
        // Check if we've recorded 1000 frames
        if matches!(self.mode, Mode::Recording) && self.time.tick_count() >= 1000 {
            println!("✅ Recorded 1000 frames. Starting replay...");
            self.recorder.stop_recording();
            self.mode = Mode::Replaying;
            
            // Reset simulation
            self.time.reset();
            
            // TODO: Save world state to compare after replay
            
            self.recorder.start_playback();
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let window_attrs = Window::default_attributes().with_title("PoC 2: Moving Triangles");
            let window = Arc::new(event_loop.create_window(window_attrs).unwrap());
            
            let renderer = pollster::block_on(TriangleRenderer::new(window.clone()));
            
            self.window = Some(window);
            self.renderer = Some(renderer);
        }
    }
    
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                // Update simulation
                let ticks = self.time.update();
                for _ in 0..ticks {
                    self.tick();
                }
                
                // Render
                if let Some(renderer) = &mut self.renderer {
                    self.profiler.time_system("render", || {
                        match renderer.render(&self.world, self.time.tick_count(), self.time.interpolation_alpha()) {
                            Ok((uploaded, instance_count, timings)) => {
                                // Track bandwidth (only DYNAMIC data uploaded every tick)
                                if uploaded {
                                    let bytes_per_upload = instance_count * std::mem::size_of::<InstanceDynamic>();
                                    self.total_bytes_uploaded += bytes_per_upload as u64;
                                    self.frame_count += 1;
                                }
                                
                                // Accumulate timings
                                self.render_timings.acquire_texture_us += timings.acquire_texture_us;
                                self.render_timings.build_instances_us += timings.build_instances_us;
                                self.render_timings.upload_instances_us += timings.upload_instances_us;
                                self.render_timings.update_uniforms_us += timings.update_uniforms_us;
                                self.render_timings.encode_commands_us += timings.encode_commands_us;
                                self.render_timings.submit_us += timings.submit_us;
                                self.render_timings.present_us += timings.present_us;
                                self.render_frame_count += 1;
                            }
                            Err(wgpu::SurfaceError::Lost) => {
                                // Reconfigure surface
                            }
                            Err(wgpu::SurfaceError::OutOfMemory) => {
                                event_loop.exit();
                            }
                            Err(e) => eprintln!("Render error: {:?}", e),
                        }
                    });
                }
                
                // End frame
                self.frame_timer.end();
                
                // Print metrics every 2 seconds
                if self.last_print.elapsed() >= std::time::Duration::from_secs(2) {
                    self.last_print = std::time::Instant::now();
                    
                    println!("\n=== Performance Metrics ===");
                    println!("FPS: {:.1} ({:.2} ms avg)", 
                        self.frame_timer.fps(), 
                        self.frame_timer.frame_time_ms());
                    
                    let (min_ms, max_ms) = self.frame_timer.frame_time_range_ms();
                    println!("Frame time range: {:.2}-{:.2} ms", min_ms, max_ms);
                    
                    println!("Entities: {} live, {} total",
                        self.world.live_entity_count(),
                        self.world.entity_count());
                    
                    // System timings
                    let physics_ms = self.profiler.get_timing("physics").as_secs_f64() * 1000.0;
                    let render_ms = self.profiler.get_timing("render").as_secs_f64() * 1000.0;
                    println!("Systems: physics={:.2}ms, render={:.2}ms", physics_ms, render_ms);
                    
                    // Bandwidth metrics (DYNAMIC position data only)
                    if self.frame_count > 0 {
                        let mb_uploaded = self.total_bytes_uploaded as f64 / (1024.0 * 1024.0);
                        let mb_per_sec = mb_uploaded / 2.0; // 2 second window
                        println!("GPU upload: {:.2} MB/s ({} bytes/instance DYNAMIC, {} uploads)", 
                            mb_per_sec, std::mem::size_of::<InstanceDynamic>(), self.frame_count);
                        self.total_bytes_uploaded = 0;
                        self.frame_count = 0;
                    }
                    
                    // Render timings breakdown
                    if self.render_frame_count > 0 {
                        let frames = self.render_frame_count as f64;
                        println!("\nRender Breakdown (avg µs per frame):");
                        println!("  acquire_texture: {:.1}", self.render_timings.acquire_texture_us as f64 / frames);
                        println!("  build_instances: {:.1}", self.render_timings.build_instances_us as f64 / frames);
                        println!("  upload_instances: {:.1}", self.render_timings.upload_instances_us as f64 / frames);
                        println!("  update_uniforms: {:.1}", self.render_timings.update_uniforms_us as f64 / frames);
                        println!("  encode_commands: {:.1}", self.render_timings.encode_commands_us as f64 / frames);
                        println!("  submit: {:.1}", self.render_timings.submit_us as f64 / frames);
                        println!("  present: {:.1}", self.render_timings.present_us as f64 / frames);
                        
                        self.render_timings = RenderTimings::default();
                        self.render_frame_count = 0;
                    }
                    
                    // Reset profiler for next period
                    self.profiler.reset();
                }
                
                // Request next frame
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if button == MouseButton::Left {
                    self.mouse_pressed = state == ElementState::Pressed;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(window) = &self.window {
                    let size = window.inner_size();
                    // Convert to NDC (-1 to 1)
                    self.mouse_pos.0 = (position.x as f32 / size.width as f32) * 2.0 - 1.0;
                    self.mouse_pos.1 = -((position.y as f32 / size.height as f32) * 2.0 - 1.0);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

// ============================================================================
// Main
// ============================================================================

fn main() {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}
