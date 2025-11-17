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
use latch_core::ecs::query::{reset_spatial_hash_metrics, spatial_hash_metrics_snapshot};
use latch_core::ecs::{
    ComponentId, EntityId, QueryRegistry, RelationBuffer, RelationType, SpatialHashConfig,
    SpatialHashGrid, SystemDescriptor, SystemHandle, World,
};
use latch_core::spawn;
use latch_core::time::SimulationTime;
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
const UNITS_PER_NDC: i32 = 2 * UNITS_PER_METER;
const PARTICLE_RADIUS: i32 = 1000; // 5 mm radius particles
const PARTICLE_DIAMETER: i32 = PARTICLE_RADIUS * 2;
const FLOOR_Y: i32 = -UNITS_PER_NDC / 2; // keep pile within view
const DEBUG_ENTITY_ID: Option<EntityId> = None;
const DEBUG_NEIGHBOR_LIMIT: usize = 8;
const COLLISION_RELATION: RelationType = RelationType::new(1);
const AXIS_JITTER_EPSILON: f32 = 0.000_1;
const AXIS_JITTER_PUSH: f32 = 1.0;
const COLLISION_LINEAR_DAMPING: f32 = 0.96;
const COLLISION_TANGENT_FRICTION: f32 = 0.2;

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

struct MovementSystem {
    #[allow(dead_code)]
    handle: SystemHandle,
    component_filter: Vec<ComponentId>,
}

impl MovementSystem {
    fn new(world: &mut World) -> Self {
        let descriptor = SystemDescriptor::new("movement")
            .reads([Position::ID, Velocity::ID])
            .writes([Position::ID, Velocity::ID]);

        let component_filter = descriptor.all_components().to_vec();
        let handle = world
            .register_system(descriptor)
            .expect("failed to register movement system");

        Self {
            handle,
            component_filter,
        }
    }

    fn run(&mut self, world: &mut World) {
        const GRAVITY: i16 = -50; // Downward acceleration

        world.for_each(&self.component_filter, |storage| {
            let page_count = {
                let column = storage
                    .column(Position::ID)
                    .expect("position column missing");
                column.page_count()
            };
            for page_idx in 0..page_count {
                let range = {
                    let column = storage
                        .column(Position::ID)
                        .expect("position column missing");
                    column.page_range(page_idx)
                };
                if range.is_empty() {
                    continue;
                }

                let (pos_col, vel_col) = storage
                    .columns_mut_pair(Position::ID, Velocity::ID)
                    .expect("archetype missing position/velocity columns");

                let (pos_read, pos_write) = pos_col
                    .slice_rw_typed::<Position>(range.clone())
                    .expect("position tile slice");
                let (vel_read, vel_write) = vel_col
                    .slice_rw_typed::<Velocity>(range.clone())
                    .expect("velocity tile slice");

                for i in 0..pos_read.len() {
                    let src_pos = pos_read[i];
                    let src_vel = vel_read[i];

                    let mut pos_x = src_pos.x as f32;
                    let mut pos_y = src_pos.y as f32;
                    let mut vel_x = src_vel.x as f32;
                    let mut vel_y_f = src_vel.y as f32;

                    vel_y_f = (vel_y_f + GRAVITY as f32).max(-10_000.0);
                    vel_y_f = vel_y_f.clamp(
                        -(PARTICLE_DIAMETER - 1) as f32,
                        (PARTICLE_DIAMETER - 1) as f32,
                    );

                    pos_x += vel_x;
                    pos_y += vel_y_f;

                    let horizontal_bound = (UNITS_PER_NDC - PARTICLE_RADIUS) as f32;
                    let floor = (FLOOR_Y + PARTICLE_RADIUS) as f32;

                    if pos_x < -horizontal_bound {
                        pos_x = -horizontal_bound;
                        vel_x = 0.0;
                    } else if pos_x > horizontal_bound {
                        pos_x = horizontal_bound;
                        vel_x = 0.0;
                    }

                    if pos_y < floor {
                        let penetration = floor - pos_y;
                        pos_y = floor;
                        if penetration > 0.0 && vel_y_f < 0.0 {
                            vel_y_f = 0.0;
                        }
                    }

                    let new_x = pos_x.round() as i32;
                    let new_y = pos_y.round() as i32;
                    let new_vel_x = vel_x.round() as i32;
                    let new_vel_y = vel_y_f.round() as i32;

                    pos_write[i] = Position { x: new_x, y: new_y };
                    vel_write[i] = Velocity {
                        x: new_vel_x.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
                        y: new_vel_y.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
                    };
                }
            }
        });
    }
}

struct CollisionSystem {
    #[allow(dead_code)]
    handle: SystemHandle,
    component_filter: Vec<ComponentId>,
    iterations: usize,
}

impl CollisionSystem {
    fn new(world: &mut World, iterations: usize) -> Self {
        let descriptor = SystemDescriptor::new("collision")
            .reads([Position::ID, Velocity::ID])
            .writes([Position::ID, Velocity::ID]);

        let component_filter = descriptor.all_components().to_vec();
        let handle = world
            .register_system(descriptor)
            .expect("failed to register collision system");

        Self {
            handle,
            component_filter,
            iterations: iterations.max(1),
        }
    }

    fn run(&mut self, world: &mut World, relations: &RelationBuffer) {
        world.for_each(&self.component_filter, |storage| {
            let page_count = {
                let column = storage
                    .column(Position::ID)
                    .expect("position column missing");
                column.page_count()
            };
            let mut entity_ids: Vec<EntityId> = Vec::new();

            for page_idx in 0..page_count {
                let range = {
                    let column = storage
                        .column(Position::ID)
                        .expect("position column missing");
                    column.page_range(page_idx)
                };
                if range.is_empty() {
                    continue;
                }

                entity_ids.clear();
                entity_ids.extend_from_slice(
                    storage
                        .entity_ids_slice(range.clone())
                        .expect("entity id slice"),
                );

                let (pos_col, vel_col) = storage
                    .columns_mut_pair(Position::ID, Velocity::ID)
                    .expect("archetype missing position/velocity columns");

                let pos_write = pos_col
                    .slice_write_typed::<Position>(range.clone())
                    .expect("position tile slice");
                let vel_write = vel_col
                    .slice_write_typed::<Velocity>(range.clone())
                    .expect("velocity tile slice");

                for _ in 0..self.iterations {
                    for i in 0..pos_write.len() {
                        let entity_id = entity_ids[i];
                        let jitter_sign = if entity_id % 2 == 0 { -1.0 } else { 1.0 };
                        let debug_this_entity = DEBUG_ENTITY_ID
                            .map(|target| target == entity_id)
                            .unwrap_or(false);
                        let neighbors = relations.relations_for_entity_id(entity_id);

                        let mut pos_x = pos_write[i].x as f32;
                        let mut pos_y = pos_write[i].y as f32;
                        let mut vel_x = vel_write[i].x as f32;
                        let mut vel_y_f = vel_write[i].y as f32;

                        if debug_this_entity {
                            println!(
                                "[dbg] collision iter entity={} pos=({}, {}), vel=({}, {}) neighbors={}",
                                entity_id,
                                pos_write[i].x,
                                pos_write[i].y,
                                vel_write[i].x,
                                vel_write[i].y,
                                neighbors.len()
                            );
                        }

                        for (idx, relation) in neighbors.iter().enumerate() {
                            let delta = match relation.delta {
                                Some(delta) => delta,
                                None => continue,
                            };
                            let dist_sq = (delta.dx.saturating_mul(delta.dx))
                                .saturating_add(delta.dy.saturating_mul(delta.dy));
                            if dist_sq <= 1 {
                                continue;
                            }
                            let dist = (dist_sq as f32).sqrt();
                            let penetration = (PARTICLE_DIAMETER as f32) - dist;
                            if penetration <= 1.0 {
                                continue;
                            }

                            let normal_x = (delta.dx as f32) / dist;
                            let normal_y = (delta.dy as f32) / dist;
                            let correction = penetration * 0.5;
                            pos_x += normal_x * correction;
                            pos_y += normal_y * correction;

                            if normal_x.abs() <= AXIS_JITTER_EPSILON {
                                pos_x += jitter_sign * AXIS_JITTER_PUSH;
                            } else if normal_y.abs() <= AXIS_JITTER_EPSILON {
                                pos_y += jitter_sign * AXIS_JITTER_PUSH;
                            }

                            let rel_vel = vel_x * normal_x + vel_y_f * normal_y;
                            if rel_vel < 0.0 {
                                vel_x -= normal_x * rel_vel;
                                vel_y_f -= normal_y * rel_vel;
                            }

                            let tangent_x = -normal_y;
                            let tangent_y = normal_x;
                            let rel_tangent = vel_x * tangent_x + vel_y_f * tangent_y;
                            if rel_tangent.abs() > f32::EPSILON {
                                let friction_impulse = rel_tangent * COLLISION_TANGENT_FRICTION;
                                vel_x -= tangent_x * friction_impulse;
                                vel_y_f -= tangent_y * friction_impulse;
                            }

                            if debug_this_entity && idx < DEBUG_NEIGHBOR_LIMIT {
                                println!(
                                    "  -> neighbor={} delta=({}, {}), pen={:.2}, normal=({:.2}, {:.2})",
                                    relation.other.index(),
                                    delta.dx,
                                    delta.dy,
                                    penetration,
                                    normal_x,
                                    normal_y
                                );
                            }
                        }

                        let horizontal_bound = (UNITS_PER_NDC - PARTICLE_RADIUS) as f32;
                        let floor = (FLOOR_Y + PARTICLE_RADIUS) as f32;

                        if pos_x < -horizontal_bound {
                            pos_x = -horizontal_bound;
                            vel_x = 0.0;
                        } else if pos_x > horizontal_bound {
                            pos_x = horizontal_bound;
                            vel_x = 0.0;
                        }

                        if pos_y < floor {
                            let penetration = floor - pos_y;
                            pos_y = floor;
                            if penetration > 0.0 && vel_y_f < 0.0 {
                                vel_y_f = 0.0;
                            }
                        }

                        vel_x *= COLLISION_LINEAR_DAMPING;
                        vel_y_f *= COLLISION_LINEAR_DAMPING;

                        pos_write[i] = Position {
                            x: pos_x.round() as i32,
                            y: pos_y.round() as i32,
                        };
                        vel_write[i] = Velocity {
                            x: vel_x
                                .round()
                                .clamp(i16::MIN as f32, i16::MAX as f32) as i16,
                            y: vel_y_f
                                .round()
                                .clamp(i16::MIN as f32, i16::MAX as f32) as i16,
                        };
                    }
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
    uv: [f32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct InstanceData {
    position: [i32; 2],
    velocity: [i16; 2], // Add velocity for shader compatibility
    color: [u8; 4],
    radius: f32,
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
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/sand_circle.wgsl").into()),
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
                        attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2],
                    },
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<InstanceData>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![
                            2 => Sint32x2,   // position
                            3 => Snorm16x2,  // velocity
                            4 => Unorm8x4,   // color
                            5 => Float32     // radius
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

        let quad_vertices = [
            Vertex {
                position: [-1.0, -1.0],
                uv: [-1.0, -1.0],
            },
            Vertex {
                position: [1.0, -1.0],
                uv: [1.0, -1.0],
            },
            Vertex {
                position: [1.0, 1.0],
                uv: [1.0, 1.0],
            },
            Vertex {
                position: [-1.0, -1.0],
                uv: [-1.0, -1.0],
            },
            Vertex {
                position: [1.0, 1.0],
                uv: [1.0, 1.0],
            },
            Vertex {
                position: [-1.0, 1.0],
                uv: [-1.0, 1.0],
            },
        ];

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(&quad_vertices),
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
                    let velocities = storage
                        .column(Velocity::ID)
                        .ok()
                        .and_then(|col| col.slice_read_typed::<Velocity>(start..end).ok());

                    for i in 0..positions.len() {
                        let velocity = velocities
                            .as_ref()
                            .map(|slice| slice[i])
                            .unwrap_or(Velocity { x: 0, y: 0 });
                        instance_data.push(InstanceData {
                            position: [positions[i].x, positions[i].y],
                            velocity: [velocity.x, velocity.y],
                            color: [colors[i].r, colors[i].g, colors[i].b, 255],
                            radius: PARTICLE_RADIUS as f32,
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
            self.queue.write_buffer(
                &self.instance_buffer,
                0,
                bytemuck::cast_slice(&instance_data),
            );
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
            render_pass.draw(0..6, 0..(instance_count as u32));
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
    relation_buffer: RelationBuffer,
    movement: MovementSystem,
    collision: CollisionSystem,
    time: SimulationTime,
    frame_timer: FrameTimer,
    profiler: SystemProfiler,
    last_print: std::time::Instant,
}

impl App {
    fn new() -> Self {
        let mut world = World::new();

        // Spawn sand particles starting near the floor so rows build upward
        // Allow rows to extend above the camera so sand continues pouring in
        let num_particles = 10_000;
        for i in 0..num_particles {
            // Distribute particles in a rectangular column rising from the floor
            let cols = 50; // Spread them out horizontally
            let row = i / cols;
            let col = i % cols;

            let spacing = PARTICLE_DIAMETER * 2; // keep roughly one particle gap
            let start_x = -(cols as i32 * spacing) / 2;
            let start_y = (FLOOR_Y + PARTICLE_RADIUS) + (row as i32 * spacing);

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

        let movement = MovementSystem::new(&mut world);
        let collision = CollisionSystem::new(&mut world, 10);

        let mut queries = QueryRegistry::new();
        let spatial_config = SpatialHashConfig::new(
            Position::ID,
            PARTICLE_RADIUS,   // tighter cells reduce bucket occupancy
            PARTICLE_DIAMETER, // keep full contact radius
            COLLISION_RELATION,
        );
        let spatial_hash = Box::new(SpatialHashGrid::new(spatial_config));
        queries.register(spatial_hash);

        let relation_buffer = RelationBuffer::new(2048, 256);

        Self {
            window: None,
            renderer: None,
            world,
            queries,
            relation_buffer,
            movement,
            collision,
            time: SimulationTime::new(),
            frame_timer: FrameTimer::new(60),
            profiler: SystemProfiler::new(),
            last_print: std::time::Instant::now(),
        }
    }

    fn tick(&mut self) {
        self.frame_timer.begin();

        self.profiler.time_system("movement", || {
            self.movement.run(&mut self.world);
        });

        // Movement writes into the next buffer; swap to expose it for query rebuilds.
        self.world.swap_buffers();

        self.profiler.time_system("rebuild_queries", || {
            self.relation_buffer.clear();
            self.queries
                .rebuild_all(&self.world, &mut self.relation_buffer);
        });

        // Swap back so collision can iterate over the next buffer while reading deltas from the rebuild.
        self.world.swap_buffers();

        self.profiler.time_system("collision", || {
            self.collision.run(&mut self.world, &self.relation_buffer);
        });

        // Commit collision corrections
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

                    let rebuild_ms =
                        self.profiler.get_timing("rebuild_queries").as_secs_f64() * 1000.0;
                    let movement_ms = self.profiler.get_timing("movement").as_secs_f64() * 1000.0;
                    let collision_ms = self.profiler.get_timing("collision").as_secs_f64() * 1000.0;
                    let render_ms = self.profiler.get_timing("render").as_secs_f64() * 1000.0;
                    println!(
                        "Systems: movement={:.2}ms, collision={:.2}ms, rebuild_queries={:.2}ms, render={:.2}ms",
                        movement_ms, collision_ms, rebuild_ms, render_ms
                    );

                    let hash_metrics = spatial_hash_metrics_snapshot();
                    if hash_metrics.total_calls > 0 {
                        let avg_total_ms = (hash_metrics.total_ns as f64)
                            / (hash_metrics.total_calls as f64)
                            / 1_000_000.0;
                        let avg_emit_ns = if hash_metrics.emit_calls > 0 {
                            (hash_metrics.emit_ns as f64) / (hash_metrics.emit_calls as f64)
                        } else {
                            0.0
                        };
                        let avg_entities = hash_metrics.entities / hash_metrics.total_calls;
                        let avg_relations = hash_metrics.relations / hash_metrics.total_calls;
                        let avg_lookups = hash_metrics.bucket_lookups / hash_metrics.total_calls;
                        let hit_rate = if hash_metrics.bucket_lookups > 0 {
                            (hash_metrics.bucket_hits as f64) / (hash_metrics.bucket_lookups as f64)
                                * 100.0
                        } else {
                            0.0
                        };
                        println!(
                            "SpatialHash: avg_total={:.2}ms, emit_avg={:.2}ns, entities/tick={}, relations/tick={}, lookups/tick={}, hit_rate={:.1}%",
                            avg_total_ms,
                            avg_emit_ns,
                            avg_entities,
                            avg_relations,
                            avg_lookups,
                            hit_rate
                        );
                    }

                    reset_spatial_hash_metrics();

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
