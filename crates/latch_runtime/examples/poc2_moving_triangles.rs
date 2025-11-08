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

#[derive(Clone, Copy, Debug)]
struct Position {
    x: f32,
    y: f32,
}
define_component!(Position, 1, "Position");

#[derive(Clone, Copy, Debug)]
struct Velocity {
    x: f32,
    y: f32,
}
define_component!(Velocity, 2, "Velocity");

#[derive(Clone, Copy, Debug)]
struct Color {
    r: f32,
    g: f32,
    b: f32,
}
define_component!(Color, 3, "Color");

// ============================================================================
// Systems
// ============================================================================

fn physics_system(world: &mut World, dt: f32) {
    use rayon::prelude::*;
    use latch_core::columns_mut;
    
    // Beautiful ergonomic API with TRUE arbitrary component support!
    //
    // The macro uses Rust's repetition patterns ($(...)*) to handle any number:
    //
    // 1 component:  let pos = columns_mut!(storage, Position);
    // 2 components: let (p, v) = columns_mut!(storage, Position, Velocity);
    // 3 components: let (p, v, c) = columns_mut!(storage, Position, Velocity, Color);
    // 5 components: let (a, b, c, d, e) = columns_mut!(storage, A, B, C, D, E);
    // 10 components: let (a, b, c, d, e, f, g, h, i, j) = columns_mut!(storage, ...);
    //
    // No hard-coded limit! The macro expands to handle however many you provide.
    
    world.par_for_each(&[Position::ID, Velocity::ID], |storage| {
        // Get both component slices - zero allocation, zero HashMap lookups!
        let (positions, velocities) = columns_mut!(storage, Position, Velocity);
        
        // Parallel iteration over component arrays
        positions.par_iter_mut()
            .zip(velocities.par_iter_mut())
            .for_each(|(pos, vel)| {
                // Update position
                pos.x += vel.x * dt;
                pos.y += vel.y * dt;
                
                // Bounce off edges
                if pos.x < -1.0 || pos.x > 1.0 {
                    pos.x = pos.x.clamp(-1.0, 1.0);
                    vel.x = -vel.x;
                }
                if pos.y < -1.0 || pos.y > 1.0 {
                    pos.y = pos.y.clamp(-1.0, 1.0);
                    vel.y = -vel.y;
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

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct InstanceData {
    position: [f32; 2],
    velocity: [f32; 2],
    color: [f32; 3],
    _padding: [f32; 2], // Align to 16 bytes
}

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
    instance_buffer: wgpu::Buffer,
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
                    // Instance buffer (position + velocity + color)
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<InstanceData>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![
                            1 => Float32x2,  // position
                            2 => Float32x2,  // velocity
                            3 => Float32x3   // color
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
        
        // Create dynamic instance buffer (will grow as needed)
        let initial_capacity = 1024;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Instance Buffer"),
            size: (std::mem::size_of::<InstanceData>() * initial_capacity) as u64,
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
            instance_buffer,
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
        
        // Only rebuild and upload instance data when physics ticks (not every render frame!)
        if tick != self.last_physics_tick {
            self.last_physics_tick = tick;
            uploaded = true;
            
            let build_start = std::time::Instant::now();
            
            // Build instance data from all entities with Position + Velocity + Color
            let mut instances = Vec::new();
            
            // Get archetypes that have all three components
            let position_archs = world.archetypes_with(Position::ID);
            let velocity_archs = world.archetypes_with(Velocity::ID);
            let color_archs = world.archetypes_with(Color::ID);
            
            for &arch_id in position_archs {
                if !velocity_archs.contains(&arch_id) || !color_archs.contains(&arch_id) {
                    continue;
                }
                
                if let (Some(positions), Some(velocities), Some(colors)) = (
                    world.column::<Position>(arch_id),
                    world.column::<Velocity>(arch_id),
                    world.column::<Color>(arch_id),
                ) {
                    instances.reserve(positions.len());
                    
                    for i in 0..positions.len() {
                        instances.push(InstanceData {
                            position: [positions[i].x, positions[i].y],
                            velocity: [velocities[i].x, velocities[i].y],
                            color: [colors[i].r, colors[i].g, colors[i].b],
                            _padding: [0.0, 0.0],
                        });
                    }
                }
            }
            
            instance_count = instances.len();
            self.last_instance_count = instance_count; // Store for non-physics frames
            
            timings.build_instances_us = build_start.elapsed().as_micros() as u64;
            
            // Resize instance buffer if needed
            if instance_count > self.instance_buffer_capacity {
                let new_capacity = instance_count.next_power_of_two();
                let bytes_per_instance = std::mem::size_of::<InstanceData>();
                println!("Resizing instance buffer: {} → {} instances ({} bytes/instance, {} KB total)", 
                    self.instance_buffer_capacity, new_capacity, bytes_per_instance, 
                    (new_capacity * bytes_per_instance) / 1024);
                
                self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Instance Buffer"),
                    size: (std::mem::size_of::<InstanceData>() * new_capacity) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.instance_buffer_capacity = new_capacity;
            }
            
            // Upload instance data to GPU (only on physics tick!)
            let upload_start = std::time::Instant::now();
            if !instances.is_empty() {
                self.queue.write_buffer(
                    &self.instance_buffer,
                    0,
                    bytemuck::cast_slice(&instances),
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
            render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
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
        
        // Spawn 100 triangles with random positions and velocities
        use std::f32::consts::PI;
        let num_triangles = 500000;
        let f_num_triangles = num_triangles as f32;
        for i in 0..num_triangles {
            let angle = (i as f32 / f_num_triangles) * 2.0 * PI;
            let radius = 0.5 + (i as f32 / f_num_triangles) * 0.3;

            let pos = Position {
                x: angle.cos() * radius,
                y: angle.sin() * radius,
            };
            
            let vel = Velocity {
                x: (angle + PI / 2.0).cos() * 0.2,
                y: (angle + PI / 2.0).sin() * 0.2,
            };
            
            let color = Color {
                r: ((i % 100) as f32 / 100.0),
                g: (1.0 - (i % 100) as f32 / 100.0),
                b: 0.5,
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
        
        // Run physics
        self.profiler.time_system("physics", || {
            physics_system(&mut self.world, TICK_DURATION_SECS);
        });
        
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
                                // Track bandwidth (only when actually uploaded)
                                if uploaded {
                                    let bytes_per_upload = instance_count * std::mem::size_of::<InstanceData>();
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
                    
                    // Bandwidth metrics
                    if self.frame_count > 0 {
                        let mb_uploaded = self.total_bytes_uploaded as f64 / (1024.0 * 1024.0);
                        let mb_per_sec = mb_uploaded / 2.0; // 2 second window
                        println!("GPU upload: {:.2} MB/s ({} bytes/instance, {} uploads)", 
                            mb_per_sec, std::mem::size_of::<InstanceData>(), self.frame_count);
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
