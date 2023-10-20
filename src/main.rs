use wgpu::{Extent3d, SurfaceError, util::DeviceExt};

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
    velocity: [f32; 2],
    position: [f32; 2],
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
    rule1d_sqr: f32,
    rule2d_sqr: f32,
    rule1s: f32,
    rule2s: f32,
}

const BOIDS: u32 = 100;
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
            depth_compare: wgpu::CompareFunction::Greater, // 1.
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
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
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
                position: [0.05, -0.05],
            },
            Vertex {
                position: [-0.05, -0.05],
            },
            Vertex {
                position: [-0.05, 0.05],
            },
        ]),
        usage: wgpu::BufferUsages::VERTEX,
    });

    let mut aspect_ratio = size.width as f32 / size.width as f32;
    let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("camera buffer"),
        size: std::mem::size_of::<Camera>() as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    
    const BOID_BUFFER_SIZE: wgpu::BufferAddress = 
        (BOIDS as wgpu::BufferAddress) * (std::mem::size_of::<Boid>() as wgpu::BufferAddress);
    let alignment = device.limits().min_storage_buffer_offset_alignment as wgpu::BufferAddress;
    let aligned_boid_buffer_size = (BOID_BUFFER_SIZE + (alignment - 1)) & !(alignment - 1);
    let boid_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("2 boid position and velocity buffers"),
        size: 2 * aligned_boid_buffer_size,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });

    let sim_params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("simulation parameters buffer"),
        size: std::mem::size_of::<SimParams>() as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
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

    let mut dynamic_offsets = [0, aligned_boid_buffer_size as wgpu::DynamicOffset];
    let mut other_boid_buffer_turn = false;
    event_loop.run(move |event, _, control_flow| {
        use winit::{event_loop::*, event::*};

        match event {
            Event::RedrawRequested(..) => {
                queue.write_buffer(
                    &camera_buffer, 
                    0, 
                    bytemuck::cast_slice(&[Camera {
                        aspect_ratio,
                    }])
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
                    compute_pass.dispatch_workgroups(
                        if BOIDS % WORK_GROUP_SIZE == 0 { 
                            BOIDS / WORK_GROUP_SIZE
                        } else {
                            BOIDS / WORK_GROUP_SIZE + 1
                        },
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
                                load: wgpu::LoadOp::Clear(0.0),
                                store: true,
                            }),
                            stencil_ops: None,
                        }),
                    });

                    render_pass.set_pipeline(&render_pipeline);
                    render_pass.set_bind_group(0, &render_bind_group, &[]);
                    render_pass.set_vertex_buffer(0, sprite_buffer.slice(..));

                    render_pass.draw(0..3, 0..BOIDS);
                }

                queue.submit(std::iter::once(encoder.finish()));
                output.present();
            }
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                WindowEvent::Resized(size) => {
                    config.width = size.width;
                    config.height = size.height;
                    if size.width > 0 && size.height > 0 {
                        surface.configure(&device, &config);
                        (depth_texture, depth_texture_view) = create_depth_texture(&device, size.width, size.height);
                        aspect_ratio = size.width as f32 / size.height as f32;
                    }
                }
                _ => {}
            }
            Event::MainEventsCleared => {
                if config.width > 0 && config.height > 0 {
                    other_boid_buffer_turn = !other_boid_buffer_turn;
                    // swap dynamic offsets of compute bind groups
                    {
                        let tmp = dynamic_offsets[0];
                        dynamic_offsets[0] = dynamic_offsets[1];
                        dynamic_offsets[1] = tmp;
                    }
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