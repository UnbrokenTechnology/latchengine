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

use latch_core::ecs::World;
use latch_core::time::{SimulationTime, InputRecorder, TickInput, TICK_DURATION_SECS};
use latch_core::spawn;

use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

use wgpu;
use std::sync::Arc;

// ============================================================================
// Components
// ============================================================================

#[derive(Clone, Copy, Debug)]
struct Position {
    x: f32,
    y: f32,
}

#[derive(Clone, Copy, Debug)]
struct Velocity {
    x: f32,
    y: f32,
}

#[derive(Clone, Copy, Debug)]
struct Color {
    r: f32,
    g: f32,
    b: f32,
}

// ============================================================================
// Systems
// ============================================================================

fn physics_system(world: &mut World, dt: f32) {
    // Apply velocity to position (collect first to avoid borrow checker issues)
    let updates: Vec<_> = world
        .query2::<Position, Velocity>()
        .map(|(e, p, v)| {
            let mut new_x = p.x + v.x * dt;
            let mut new_y = p.y + v.y * dt;
            
            // Calculate bounce
            let mut bounce_x = false;
            let mut bounce_y = false;
            
            if new_x < -1.0 || new_x > 1.0 {
                bounce_x = true;
                new_x = new_x.clamp(-1.0, 1.0);
            }
            if new_y < -1.0 || new_y > 1.0 {
                bounce_y = true;
                new_y = new_y.clamp(-1.0, 1.0);
            }
            
            (e, new_x, new_y, bounce_x, bounce_y)
        })
        .collect();
    
    // Apply updates
    for (entity, new_x, new_y, bounce_x, bounce_y) in updates {
        if let Some(pos) = world.get_component_mut::<Position>(entity) {
            pos.x = new_x;
            pos.y = new_y;
        }
        
        if bounce_x || bounce_y {
            if let Some(vel) = world.get_component_mut::<Velocity>(entity) {
                if bounce_x {
                    vel.x = -vel.x;
                }
                if bounce_y {
                    vel.y = -vel.y;
                }
            }
        }
    }
}

// ============================================================================
// Renderer
// ============================================================================

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 3],
}

struct TriangleRenderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    #[allow(dead_code)] // Stored for potential future use
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
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
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Triangle Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/triangle.wgsl").into()),
        });
        
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x3],
                }],
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
        
        // Create initial vertex buffer (will be updated each frame)
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Vertex Buffer"),
            size: (std::mem::size_of::<Vertex>() * 3 * 100) as u64, // 100 triangles max
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
        }
    }
    
    fn render(&mut self, world: &World) -> Result<(), wgpu::SurfaceError> {
        // Build vertex buffer from all entities with Position + Color
        let mut vertices = Vec::new();
        
        for (_entity, pos, color) in world.query2::<Position, Color>() {
            let size = 0.02; // Triangle size
            vertices.extend_from_slice(&[
                Vertex {
                    position: [pos.x, pos.y + size],
                    color: [color.r, color.g, color.b],
                },
                Vertex {
                    position: [pos.x - size, pos.y - size],
                    color: [color.r, color.g, color.b],
                },
                Vertex {
                    position: [pos.x + size, pos.y - size],
                    color: [color.r, color.g, color.b],
                },
            ]);
        }
        
        // Upload vertices to GPU
        if !vertices.is_empty() {
            self.queue.write_buffer(
                &self.vertex_buffer,
                0,
                bytemuck::cast_slice(&vertices),
            );
        }
        
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        
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
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.draw(0..(vertices.len() as u32), 0..1);
        }
        
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        
        Ok(())
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
        for i in 0..100 {
            let angle = (i as f32 / 100.0) * 2.0 * PI;
            let radius = 0.5 + (i as f32 / 100.0) * 0.3;
            
            let pos = Position {
                x: angle.cos() * radius,
                y: angle.sin() * radius,
            };
            
            let vel = Velocity {
                x: (angle + PI / 2.0).cos() * 0.2,
                y: (angle + PI / 2.0).sin() * 0.2,
            };
            
            let color = Color {
                r: (i as f32 / 100.0),
                g: (1.0 - i as f32 / 100.0),
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
        }
    }
    
    fn tick(&mut self) {
        // Record input
        let input = TickInput {
            tick: self.time.tick_count(),
            mouse_x: self.mouse_pos.0,
            mouse_y: self.mouse_pos.1,
            mouse_pressed: self.mouse_pressed,
        };
        self.recorder.record(input);
        
        // Run physics
        physics_system(&mut self.world, TICK_DURATION_SECS);
        
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
                    match renderer.render(&self.world) {
                        Ok(_) => {}
                        Err(wgpu::SurfaceError::Lost) => {
                            // Reconfigure surface
                        }
                        Err(wgpu::SurfaceError::OutOfMemory) => {
                            event_loop.exit();
                        }
                        Err(e) => eprintln!("Render error: {:?}", e),
                    }
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
