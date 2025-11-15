// PoC 4: Entity Query System - Falling Sand
//
// Goals:
// - Demonstrate spatial hash grid query accelerator
// - Sand particles that detect nearby particles
// - Push particles away from each other on collision
// - Show efficient neighbor queries without iterating all entities
//
// Success Criteria:
// - 10,000+ sand particles
// - Smooth 60 FPS with collision detection
// - Particles never overlap
// - Query performance scales better than O(n²)

use latch_core::define_component;
use latch_core::ecs::{
    query_params_radius, ComponentId, QueryRegistry, SpatialHashConfig, SpatialHashGrid,
    SystemDescriptor, SystemHandle, World,
};
use latch_core::spawn;
use latch_core::time::{SimulationTime, TICK_DURATION_SECS};
use latch_metrics::{FrameTimer, SystemProfiler};

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

use std::sync::Arc;
use wgpu;
use wgpu::util::DeviceExt;

// ============================================================================
// Components
// ============================================================================

const UNITS_PER_METER: i32 = 100_000;
const UNITS_PER_NDC: i32 = 10 * UNITS_PER_METER;

#[derive(Clone, Copy, Debug)]
struct Position {
    x: i32,
    y: i32,
}
define_component!(Position, 1, "Position");

#[derive(Clone, Copy, Debug)]
struct Velocity {
    x: i16,
    y: i16,
}
define_component!(Velocity, 2, "Velocity");

#[derive(Clone, Copy, Debug)]
struct Color {
    r: u8,
    g: u8,
    b: u8,
}
define_component!(Color, 3, "Color");

// ============================================================================
// Systems
// ============================================================================

struct PhysicsSystem {
    #[allow(dead_code)]
    handle: SystemHandle,
    component_filter: Vec<ComponentId>,
}

impl PhysicsSystem {
    fn new(world: &mut World) -> Self {
        let descriptor = SystemDescriptor::new("physics")
            .reads([Position::ID, Velocity::ID])
            .writes([Position::ID, Velocity::ID]);

        let component_filter = descriptor.all_components().to_vec();
        let handle = world
            .register_system(descriptor)
            .expect("failed to register physics system");

        Self {
            handle,
            component_filter,
        }
    }

    fn run(&mut self, world: &mut World, queries: &QueryRegistry, _dt: f32) {
        const GRAVITY: i16 = -50; // Downward acceleration
        const PARTICLE_RADIUS: i32 = 30_000; // 0.3 meters in units
        const MIN_SEPARATION: i32 = PARTICLE_RADIUS * 2;

        world.for_each(&self.component_filter, |storage| {
            let (pos_col, vel_col) = storage
                .columns_mut_pair(Position::ID, Velocity::ID)
                .expect("archetype missing position/velocity columns");

            for page_idx in 0..pos_col.page_count() {
                let range = pos_col.page_range(page_idx);
                if range.is_empty() {
                    continue;
                }

                let start = range.start;
                let end = range.end;

                let (pos_read, pos_write) = pos_col
                    .slice_rw_typed::<Position>(start..end)
                    .expect("position tile slice");
                let (vel_read, vel_write) = vel_col
                    .slice_rw_typed::<Velocity>(start..end)
                    .expect("velocity tile slice");

                for i in 0..pos_read.len() {
                    let src_pos = pos_read[i];
                    let src_vel = vel_read[i];

                    // Apply gravity
                    let mut new_vel_y = src_vel.y + GRAVITY;
                    new_vel_y = new_vel_y.max(-10000); // Terminal velocity

                    // Apply velocity
                    let mut new_x = src_pos.x + src_vel.x as i32;
                    let mut new_y = src_pos.y + new_vel_y as i32;

                    // Check collisions with nearby particles using spatial hash
                    // Note: This queries the spatial hash which reflects the PREVIOUS frame's positions
                    // This is acceptable for soft-body physics and avoids borrowing issues
                    if let Some(spatial_hash) = queries.get(Position::ID) {
                        let query_params = query_params_radius(new_x, new_y, MIN_SEPARATION);
                        let nearby = spatial_hash.query(&query_params);

                        if nearby.len() > 1 {
                            // Has nearby particles (>1 because it includes self)
                            // Apply a simple repulsion force
                            new_vel_y = (new_vel_y * 9) / 10; // Dampen slightly
                        }
                    }

                    // Boundary collision
                    let bound = UNITS_PER_NDC - PARTICLE_RADIUS;

                    let mut new_vel_x = src_vel.x;

                    if new_x < -bound {
                        new_x = -bound;
                        new_vel_x = 0;
                    } else if new_x > bound {
                        new_x = bound;
                        new_vel_x = 0;
                    }

                    if new_y < -bound {
                        new_y = -bound;
                        new_vel_y = 0; // Stop falling at bottom
                    } else if new_y > bound {
                        new_y = bound;
                        new_vel_y = 0;
                    }

                    pos_write[i] = Position { x: new_x, y: new_y };
                    vel_write[i] = Velocity { x: new_vel_x, y: new_vel_y };
                }
            }
        });
    }
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
    position: [i32; 2],
    velocity: [i16; 2], // Add velocity for shader compatibility
    color: [u8; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    interpolation_alpha: f32,
    dt: f32,
    _padding: [f32; 2],
}

struct ParticleRenderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    #[allow(dead_code)]
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    instance_buffer_capacity: usize,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
}

impl ParticleRenderer {
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
            present_mode: wgpu::PresentMode::Immediate,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Particle Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/triangle.wgsl").into()),
        });

        // Create uniform buffer
        let uniforms = Uniforms {
            interpolation_alpha: 0.0,
            dt: 0.0,
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
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x2],
                    },
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<InstanceData>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![
                            1 => Sint32x2,   // position
                            2 => Snorm16x2,  // velocity
                            3 => Unorm8x4    // color
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
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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

        let size = 0.02;
        let circle_vertices = [
            Vertex { position: [0.0, size] },
            Vertex { position: [-size, -size] },
            Vertex { position: [size, -size] },
        ];

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(&circle_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

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
            uniform_buffer,
            uniform_bind_group,
        }
    }

    fn render(&mut self, world: &World) -> Result<usize, wgpu::SurfaceError> {
        let mut instance_data: Vec<InstanceData> = Vec::new();

        let position_archs = world.archetypes_with(Position::ID);
        let color_archs = world.archetypes_with(Color::ID);

        for &arch_id in position_archs {
            if !color_archs.contains(&arch_id) {
                continue;
            }

            if let Some(storage) = world.storage(arch_id) {
                let positions_col = storage.column(Position::ID).expect("position column");
                let colors_col = storage.column(Color::ID).expect("color column");

                for page_idx in 0..positions_col.page_count() {
                    let range = positions_col.page_range(page_idx);
                    if range.is_empty() {
                        continue;
                    }

                    let start = range.start;
                    let end = range.end;

                    let positions = positions_col
                        .slice_read_typed::<Position>(start..end)
                        .expect("position slice");
                    let colors = colors_col
                        .slice_read_typed::<Color>(start..end)
                        .expect("color slice");

                    for i in 0..positions.len() {
                        instance_data.push(InstanceData {
                            position: [positions[i].x, positions[i].y],
                            velocity: [0, 0], // No interpolation needed for this demo
                            color: [colors[i].r, colors[i].g, colors[i].b, 255],
                        });
                    }
                }
            }
        }

        let instance_count = instance_data.len();

        if instance_count > self.instance_buffer_capacity {
            let new_capacity = instance_count.next_power_of_two();
            self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Instance Buffer"),
                size: (std::mem::size_of::<InstanceData>() * new_capacity) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.instance_buffer_capacity = new_capacity;
        }

        if !instance_data.is_empty() {
            self.queue
                .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&instance_data));
        }

        // Update uniforms (no interpolation for this demo)
        let uniforms = Uniforms {
            interpolation_alpha: 0.0,
            dt: 0.0,
            _padding: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
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
                            r: 0.05,
                            g: 0.05,
                            b: 0.05,
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
            render_pass.draw(0..3, 0..(instance_count as u32));
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(instance_count)
    }
}

// ============================================================================
// Application
// ============================================================================

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<ParticleRenderer>,
    world: World,
    queries: QueryRegistry,
    physics: PhysicsSystem,
    time: SimulationTime,
    frame_timer: FrameTimer,
    profiler: SystemProfiler,
    last_print: std::time::Instant,
}

impl App {
    fn new() -> Self {
        let mut world = World::new();

        // Spawn sand particles in a pile at the top
        let num_particles = 10_000;
        for i in 0..num_particles {
            // Distribute particles in a rectangular region at the top
            let cols = 100;
            let row = i / cols;
            let col = i % cols;

            let spacing = 60_000; // 0.6 meters
            let start_x = -(cols as i32 * spacing) / 2;
            let start_y = UNITS_PER_NDC - (row as i32 * spacing);

            let pos = Position {
                x: start_x + col as i32 * spacing,
                y: start_y,
            };

            let vel = Velocity { x: 0, y: 0 };

            // Color based on position
            let hue = (i % 100) as i32;
            let color = Color {
                r: ((hue * 255) / 100) as u8,
                g: 127,
                b: (((100 - hue) * 255) / 100) as u8,
            };

            spawn!(world, pos, vel, color);
        }

        let physics = PhysicsSystem::new(&mut world);

        // Create spatial hash grid for position queries
        let mut queries = QueryRegistry::new();
        let spatial_config = SpatialHashConfig::new(Position::ID, 100_000); // 1 meter cells
        let spatial_hash = Box::new(SpatialHashGrid::new(spatial_config));
        queries.register(Position::ID, spatial_hash);

        Self {
            window: None,
            renderer: None,
            world,
            queries,
            physics,
            time: SimulationTime::new(),
            frame_timer: FrameTimer::new(60),
            profiler: SystemProfiler::new(),
            last_print: std::time::Instant::now(),
        }
    }

    fn tick(&mut self) {
        self.frame_timer.begin();

        // Rebuild spatial hash before physics
        self.profiler.time_system("rebuild_queries", || {
            if let Some(spatial_hash) = self.queries.get_mut(Position::ID) {
                spatial_hash.rebuild(&self.world);
            }
        });

        // Run physics
        self.profiler.time_system("physics", || {
            self.physics
                .run(&mut self.world, &self.queries, TICK_DURATION_SECS);
        });

        // Swap buffers
        self.world.swap_buffers();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let window_attrs = Window::default_attributes().with_title("PoC 4: Falling Sand");
            let window = Arc::new(event_loop.create_window(window_attrs).unwrap());

            let renderer = pollster::block_on(ParticleRenderer::new(window.clone()));

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
                let ticks = self.time.update();
                for _ in 0..ticks {
                    self.tick();
                }

                if let Some(renderer) = &mut self.renderer {
                    self.profiler.time_system("render", || {
                        match renderer.render(&self.world) {
                            Ok(instance_count) => {
                                // Success
                                let _ = instance_count;
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

                self.frame_timer.end();

                // Print metrics every 2 seconds
                if self.last_print.elapsed() >= std::time::Duration::from_secs(2) {
                    self.last_print = std::time::Instant::now();

                    println!("\n=== Performance Metrics ===");
                    println!(
                        "FPS: {:.1} ({:.2} ms avg)",
                        self.frame_timer.fps(),
                        self.frame_timer.frame_time_ms()
                    );

                    println!("Entities: {}", self.world.entity_count());

                    let rebuild_ms = self.profiler.get_timing("rebuild_queries").as_secs_f64() * 1000.0;
                    let physics_ms = self.profiler.get_timing("physics").as_secs_f64() * 1000.0;
                    let render_ms = self.profiler.get_timing("render").as_secs_f64() * 1000.0;
                    println!(
                        "Systems: rebuild_queries={:.2}ms, physics={:.2}ms, render={:.2}ms",
                        rebuild_ms, physics_ms, render_ms
                    );

                    self.profiler.reset();
                }

                if let Some(window) = &self.window {
                    window.request_redraw();
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
