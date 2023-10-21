use std::time::Instant;

use bytemuck::offset_of;
use wgpu::*;
use wgpu::{Extent3d, util::DeviceExt};
mod input;

fn main() {
    env_logger::init();
    pollster::block_on(run());
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
struct Vertex {
    position: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
struct Boid {
    position: [f32; 2],
    velocity: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
struct Camera {
    aspect_ratio: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
struct SimParams {
    dt: f32,
    rule1_d2: f32,
    rule2_d2: f32,
    rule3_d2: f32,
    rule1_w: f32,
    rule2_w: f32,
    rule3_w: f32,
}

const BOIDS: u32 = 1000;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const VERTEX_ATTRIBS: &[wgpu::VertexAttribute] = &wgpu::vertex_attr_array![0 => Float32x2];
const BOID_ATTRIBS: &[wgpu::VertexAttribute] = &wgpu::vertex_attr_array![1 => Float32x2, 2 => Float32x2];

async fn run() {
    use winit::*;

    let event_loop = event_loop::EventLoop::new();
    let window = window::Window::new(&event_loop).unwrap();

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());

    let surface = unsafe { instance.create_surface(&window) }.unwrap();
    let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions::default()).await.unwrap();

    let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default(), None).await.unwrap();
    device.limits().min_storage_buffer_offset_alignment;
    let surface_caps = surface.get_capabilities(&adapter);
    // Shader code in this tutorial assumes an sRGB surface texture. Using a different
    // one will result all the colors coming out darker. If you want to support non
    // sRGB surfaces, you'll need to account for that when drawing to the frame.
    let surface_format = surface_caps.formats.iter()
        .copied()
        .find(|f| f.is_srgb())            
        .unwrap_or(surface_caps.formats[0]);
    let size = window.inner_size();
    let mut config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: surface_format,
        width: size.width,
        height: size.height,
        present_mode: surface_caps.present_modes[0],
        alpha_mode: surface_caps.alpha_modes[0],
        view_formats: vec![],
    };
    surface.configure(&device, &config);

    let (mut depth_texture, mut depth_texture_view) = create_depth_texture(&device, size.width, size.height);

    let render_bind_group_layout =
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        entries: &[
            wgpu::BindGroupLayoutEntry { // camera bind group
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
        label: Some("bind_group_layout"),
    });

    let sprite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("sprite.wgsl").into()),
    });

    let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Render Pipeline Layout"),
        bind_group_layouts: &[&render_bind_group_layout],
        push_constant_ranges: &[],
    });

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Render Pipeline"),
        layout: Some(&render_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &sprite_shader,
            entry_point: "vs_main", // 1.
            buffers: &[
                wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: VERTEX_ATTRIBS,
                },
                wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Boid>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: BOID_ATTRIBS,
                },
            ], // 2.
        },
        fragment: Some(wgpu::FragmentState { // 3.
            module: &sprite_shader,
            entry_point: "fs_main",
            targets: &[Some(wgpu::ColorTargetState { // 4.
                format: config.format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList, // 1.
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw, // 2.
            cull_mode: Some(wgpu::Face::Back),
            // Setting this to anything other than Fill requires Features::NON_FILL_POLYGON_MODE
            polygon_mode: wgpu::PolygonMode::Fill,
            // Requires Features::DEPTH_CLIP_CONTROL
            unclipped_depth: false,
            // Requires Features::CONSERVATIVE_RASTERIZATION
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less, // 1.
            stencil: wgpu::StencilState::default(), // 2.
            bias: wgpu::DepthBiasState::default(),
        }), // 1.
        multisample: wgpu::MultisampleState {
            count: 1, // 2.
            mask: !0, // 3.
            alpha_to_coverage_enabled: false, // 4.
        },
        multiview: None, // 5.
    });

    let sim_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("boid simulation shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("sim.wgsl").into()),
    });

    let compute_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Boid bind group layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: true,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: true,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }
        ],
    });

    let compute_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("compute pipeline layout"),
        bind_group_layouts: &[&compute_bind_group_layout],
        push_constant_ranges: &[],
    });
    let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("compute pipeline"),
        layout: Some(&compute_pipeline_layout),
        module: &sim_shader,
        entry_point: "main",
    });

    let sprite_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("sprite buffer"),
        contents: bytemuck::cast_slice(&[
            Vertex {
                position: [0.02, 0.0],
            },
            Vertex {
                position: [-0.01, -0.01],
            },
            Vertex {
                position: [-0.01, 0.01],
            },
        ]),
        usage: wgpu::BufferUsages::VERTEX,
    });

    let mut aspect_ratio = size.width as f32 / size.height as f32;
    let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("camera buffer"),
        contents: bytemuck::cast_slice(&[
            Camera {
                aspect_ratio,
            }
        ]),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    
    const BOID_BUFFER_SIZE: wgpu::BufferAddress = 
        (BOIDS as wgpu::BufferAddress) * (std::mem::size_of::<Boid>() as wgpu::BufferAddress);
    let alignment = device.limits().min_storage_buffer_offset_alignment as wgpu::BufferAddress;
    let aligned_boid_buffer_size = (BOID_BUFFER_SIZE + (alignment - 1)) & !(alignment - 1);
    let boid_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("2 boid position and velocity buffers"),
        size: 2 * aligned_boid_buffer_size,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::STORAGE,
        mapped_at_creation: true,
    });
    {   
        use rand::prelude::*;
        let mut rng = rand::thread_rng();
        let init_boids = (0..BOIDS).map(|_| Boid {
            position: [rng.gen::<f32>() * 2.0 - 1.0, rng.gen::<f32>() * 2.0 - 1.0],
            velocity: [rng.gen::<f32>() - 0.5, rng.gen::<f32>() - 0.5],
        }).collect::<Vec<_>>();

        boid_buffer
            .slice(..BOID_BUFFER_SIZE)
            .get_mapped_range_mut()
            .copy_from_slice(bytemuck::cast_slice(&init_boids));
    }
    boid_buffer.unmap();
    let sim_params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("simulation parameters buffer"),
        contents: bytemuck::cast_slice(&[
            SimParams {
                dt: 0.01,
                rule1_d2: 0.1 * 0.1,
                rule2_d2: 0.025 * 0.025,
                rule3_d2: 0.025 * 0.025,
                rule1_w: 0.05,
                rule2_w: 0.2,
                rule3_w: 0.01,
            }
        ]),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    

    let render_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("camera bind group"),
        layout: &render_bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: camera_buffer.as_entire_binding(),
        }],
    });

    let compute_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor{
        label: Some("boid bind group"),
        layout: &compute_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &boid_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(BOID_BUFFER_SIZE),
                }),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &boid_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(BOID_BUFFER_SIZE),
                }),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: sim_params_buffer.as_entire_binding(),
            }
        ],
    });

    let instant = std::time::Instant::now();
    let mut last_frame_time = instant.elapsed().as_secs_f32();
    let mut delta_frame_time = 0.0;
    let mut time_rendered = 0.0;
    let mut frames = 0;

    let sim_speed = 1.0;
    let mut rule1_d = 0.1;
    let mut rule2_d = 0.025;
    let mut rule3_d = 0.025;
    let mut rule1_w = 0.05;
    let mut rule2_w = 0.5;
    let mut rule3_w = 0.2;
    let mut dynamic_offsets = [0, aligned_boid_buffer_size as wgpu::DynamicOffset];
    let mut write_to_other_boid_buffer = true;

    let mut input = input::InputState::new();
    
    event_loop.run(move |event: event::Event<'_, ()>, _, control_flow| {
        use winit::{event_loop::*, event::*};

        match event {
            Event::RedrawRequested(..) => {
                if config.width == 0 || config.height == 0 {
                    return;
                }

                frames += 1;
                let frame_time = instant.elapsed().as_secs_f32();
                delta_frame_time = frame_time - last_frame_time;
                last_frame_time = frame_time;
                time_rendered += delta_frame_time;

                window.set_title(&format!("fps: {}, average fps: {}, time rendered: {}", 
                    (1.0 / delta_frame_time) as u32,
                    (frames as f32 / time_rendered) as u32,
                    time_rendered,
                ));

                // perhaps need to synchronize so this write happens before simulation
                queue.write_buffer(
                    &sim_params_buffer, 
                    0,
                    bytemuck::cast_slice(&[
                        SimParams {
                            dt: delta_frame_time * sim_speed,
                            rule1_d2: rule1_d * rule1_d,
                            rule2_d2: rule2_d * rule2_d,
                            rule3_d2: rule3_d * rule3_d,
                            rule1_w,
                            rule2_w,
                            rule3_w,
                        }
                    ]), 
                );

                let output = surface.get_current_texture().unwrap();
                let output_view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("command block")
                });

                {
                    let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("my computepass"),
                    });

                    compute_pass.set_pipeline(&compute_pipeline);
                    compute_pass.set_bind_group(
                        0,
                        &compute_bind_group,
                        &dynamic_offsets,
                    );

                    const WORK_GROUP_SIZE: u32 = 64;
                    const WORK_GROUPS: u32 = if BOIDS % WORK_GROUP_SIZE == 0 { 
                        BOIDS / WORK_GROUP_SIZE
                    } else {
                        BOIDS / WORK_GROUP_SIZE + 1
                    };
                    compute_pass.dispatch_workgroups(
                        WORK_GROUPS,
                        1,
                        1,
                    );
                }

                {
                    let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("my renderpass"),
                        color_attachments: &[
                            Some(wgpu::RenderPassColorAttachment {
                                view: &output_view,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(wgpu::Color{
                                        r: 0.01,
                                        g: 0.0,
                                        b: 0.02,
                                        a: 1.0,
                                    }),
                                    store: true,
                                },
                            }),
                        ],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: &depth_texture_view,
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(1.0),
                                store: true,
                            }),
                            stencil_ops: None,
                        }),
                    });

                    render_pass.set_pipeline(&render_pipeline);
                    render_pass.set_bind_group(0, &render_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, sprite_buffer.slice(..));
                    render_pass.set_vertex_buffer(1, boid_buffer.slice(
                        if write_to_other_boid_buffer {
                            0..BOID_BUFFER_SIZE
                        } else {
                            aligned_boid_buffer_size..aligned_boid_buffer_size + BOID_BUFFER_SIZE
                        }
                    ));

                    render_pass.draw(0..3, 0..BOIDS);
                }

                
                queue.submit(std::iter::once(encoder.finish()));
                output.present();

                // swap dynamic offsets of compute bind groups
                {
                    let tmp = dynamic_offsets[0];
                    dynamic_offsets[0] = dynamic_offsets[1];
                    dynamic_offsets[1] = tmp;
                }
                write_to_other_boid_buffer = !write_to_other_boid_buffer;
            }
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                WindowEvent::Resized(size) => {
                    if config.width == 0 && config.height == 0 {
                        last_frame_time = instant.elapsed().as_secs_f32();
                    }

                    config.width = size.width;
                    config.height = size.height;
                    if size.width > 0 && size.height > 0 {

                        surface.configure(&device, &config);
                        (depth_texture, depth_texture_view) = create_depth_texture(&device, size.width, size.height);
                        aspect_ratio = size.width as f32 / size.height as f32;
                        queue.write_buffer(
                            &camera_buffer, 
                            0,
                            bytemuck::cast_slice(&[
                                Camera {
                                    aspect_ratio,
                                }
                            ]) 
                        );
                    }
                }
                _ => {}
            }
            Event::DeviceEvent {event, ..} => match event {
                DeviceEvent::Key(KeyboardInput {
                    virtual_keycode: Some(virtual_keycode),
                    state,
                    ..
                }) => {
                    input.set_key_pressed(virtual_keycode, state == ElementState::Pressed);
                }
                _ => {}
            }
            Event::MainEventsCleared => {
                use VirtualKeyCode::*;

                if config.width > 0 && config.height > 0 {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    });
}

fn create_depth_texture(device: &wgpu::Device, width: u32, height: u32) -> (wgpu::Texture, wgpu::TextureView) {  
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth texture"),
        size: Extent3d {
            width: width,
            height: height,
            depth_or_array_layers: 1,
        },
        format: DEPTH_FORMAT,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });  

    let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    (texture, texture_view)
}