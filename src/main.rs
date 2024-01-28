use std::{mem::size_of, f32::consts::TAU, cmp::Ordering};

use bytemuck::{bytes_of};
use wgpu::*;
use winit::dpi::PhysicalSize;
use math::{Vector3, BiVector3, Vector2, Scale2, Rotor};

use crate::math::Scale3;

use {Extent3d, util::DeviceExt};
mod input;
mod math;
mod polygon;

fn main() {
    // env_logger::init();
    pollster::block_on(run());
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
struct Vertex {
    position: [f32; 3],
}

struct Camera {
    translation: Vector3,
    // vector rotated along xz plane from the z-axis by z_to_x
    forward: Vector3,
    /// angle from local-z to local-x axis
    z_to_x: f32,
    /// angle from local-xz plane to local-y
    xz_to_y: f32,
    /// camera's eye is near_z behind projection point,
    /// everything behind near_z is not rendered
    near_z: f32,
    far_z: f32,
    width: f32,
    height: f32,
}

struct Instance {
    translation: Vector3,
    rotation: math::Rotor,
    scale: math::Scale3,
}

struct Light {
    translation: Vector3,
    near_z: f32,
    width: f32,
    height: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
struct LightRaw {
    view: math::Affine3,
    near_z: f32,
    _padding: [u32; 3],
}

impl Light {
    pub fn compute_view(&self) -> math::Affine3 {
        *math::Affine3::IDENTITY
            .translate(&(-self.translation))
    }

    fn into_raw(&self, view: &math::Affine3) -> LightRaw {
        LightRaw {
            view: *view,
            near_z: self.near_z,
            _padding: Default::default(),
        }
    }
}

impl Instance {
    fn to_raw(&self) -> InstanceRaw {
        InstanceRaw {
            affine: math::Affine3::from(self.scale, self.rotation, self.translation)
        }
    }
}

impl Camera {
    fn update_forward(&mut self) {
        self.forward.z = self.z_to_x.cos();
        self.forward.x = self.z_to_x.sin();
    }
    fn compute_model(&self) -> math::Affine3 {
        let plane = self.forward.wedge(&Vector3::new(0.0, 1.0, 0.0));
        *math::Affine3::IDENTITY
            .rotate(self.z_to_x, &math::BiVector3::new(0.0, 0.0, 1.0))
            .rotate(self.xz_to_y, &plane)
            .translate(&self.translation)
    }

    fn to_raw(&self) -> CameraRaw {
        let plane = self.forward.wedge(&Vector3::new(0.0, 1.0, 0.0));

        CameraRaw {
            view: *math::Affine3::IDENTITY
                .translate(&(-self.translation))
                .rotate(-self.xz_to_y, &plane)
                .rotate(-self.z_to_x, &BiVector3::new(0.0, 0.0, 1.0))
                .scale(&Scale3::new(2.0 * self.near_z / self.width, 2.0 * self.near_z / self.height, 1.0)),
            near_z: self.near_z,
            _padding: Default::default(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
struct InstanceRaw {
    affine: math::Affine3,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
struct CameraRaw {
    view: math::Affine3,
    near_z: f32,
    // projection plane size
    _padding: [u32; 3],
}

const DEPTH_FORMAT: TextureFormat = TextureFormat::Depth32Float;
const INSTANCE_LAYOUT: VertexBufferLayout = VertexBufferLayout {
    array_stride: size_of::<InstanceRaw>() as BufferAddress,
    step_mode: VertexStepMode::Instance,
    attributes: &vertex_attr_array![
        5 => Float32x4,
        6 => Float32x4,
        7 => Float32x4,
    ],
};
const VERTEX_LAYOUT: VertexBufferLayout = VertexBufferLayout {
    array_stride: size_of::<Vertex>() as BufferAddress,
    step_mode: VertexStepMode::Vertex,
    attributes: &vertex_attr_array![
        0 => Float32x3,
    ],
};

fn compute_depth_divs(width: f32, height: f32, near: f32, far: f32, divs: &mut [f32]) {
    
}

fn compute_fits(
    camera_model: &math::Affine3,
    camera_near_z: f32,
    camera_far_z: f32,
    camera_width: f32,
    camera_height: f32,
    light_view: &math::Affine3,
    light_near_z: f32,
    light_width: f32,
    light_height: f32,
    max_fits: usize,
    out_fits: &mut [(Vector2, Scale2)],
) -> usize {
    let transform = camera_model.compose(light_view);

    let camera_origin = Vector3::IDENTITY
        .apply(&transform);
    
    let camera_rays = {
        let right = camera_width / 2.0;
        let top = camera_height / 2.0;
        let left = -right;
        let bottom = -top;
        let mut rays = [
            Vector3::new(left, bottom, 1.0),
            Vector3::new(right, bottom, 1.0),
            Vector3::new(left, top, 1.0),
            Vector3::new(right, top, 1.0),
        ];

        for ray in rays.iter_mut() {
            *ray = ray
                .apply(&transform)
                - camera_origin;
        }

        rays
    };

    // all z values of intersection are = to light_near_z
    let mut intersects = [(f32::NAN, Vector2::NAN); 4];
    let intersects = {
        let mut intersect_len = 0;

        // origin.z + z * ray.z = near_z
        // t = (near_z - origin.z) / ray.z
        for ray_i in 0..4 {
            // handle div by 0.0
            let z = (light_near_z - camera_origin.z) / camera_rays[ray_i].z;
            if z > 0.0 {
                intersects[intersect_len] = (z, Vector2::new(
                    camera_origin.x + z * camera_rays[ray_i].x,
                    camera_origin.y + z * camera_rays[ray_i].y,
                ));
                intersect_len += 1;
            }
        }

        intersects[..intersect_len].sort_unstable_by(
            |(z0, _), (z1, _)| 
            z0.partial_cmp(z1).unwrap()
        );

        &intersects[..intersect_len]
    };

    match intersects.len() {
        0 => todo!(),
        1 => todo!(),
        2 => todo!(),
        _ => todo!(),
    }
}

/// cuts camera view volume and light view plane,
/// projects cut volume onto light view plane,
/// intersects projection with light view frame.
fn compute_camera_fit_on_light_plane(
    camera_model: &math::Affine3,
    camera_far_z: f32,
    camera_near_z: f32,
    camera_width: f32,
    camera_height: f32,
    light_view: &math::Affine3,
    light_near_z: f32,
    light_width: f32,
    light_height: f32,
) -> Option<(Vector2, Scale2)> {
    let near_right = camera_width / 2.0;
    let near_top = camera_height / 2.0;
    let near_left = -near_right;
    let near_bottom = -near_top;

    let factor = camera_far_z / camera_near_z;
    let far_right = near_right * factor;
    let far_bottom = near_bottom * factor;
    let far_left = -far_right;
    let far_top = -far_bottom;    

    // camera view volume corners
    let mut corners = [
        Vector3::new(near_left, near_bottom, camera_near_z),
        Vector3::new(near_right, near_bottom, camera_near_z),
        Vector3::new(near_left, near_top, camera_near_z),
        Vector3::new(near_right,  near_top, camera_near_z),
        Vector3::new(far_left, far_bottom, camera_far_z),
        Vector3::new(far_right, far_bottom, camera_far_z),
        Vector3::new(far_left, far_top, camera_far_z),
        Vector3::new(far_right, far_top, camera_far_z),
    ];
    
    let affine = camera_model.compose(light_view);
    for corner in corners.iter_mut() {
        *corner = corner.apply(&affine);
    }

    /// maximum amount of projected cut camera view volume corners
    const MAX_CORNERS: usize = 10;
    let mut cut_corners = [Vector2::IDENTITY; MAX_CORNERS];
    let mut cut_corners_len = 0;
    for i in 0..corners.len() {
        let corner = corners[i];

        if corner.z < light_near_z {
            println!("AAA");
            let mut axis_mask = 0b100;
            while axis_mask != 0b000 {
                let other_corner = corners[i ^ axis_mask];
                if other_corner.z > light_near_z {
                    let t = (light_near_z - corner.z) / (other_corner.z - corner.z);
                    cut_corners[cut_corners_len] = Vector2::new(
                        (other_corner.x - corner.x) * t + corner.x, 
                        (other_corner.y - corner.y) * t + corner.y, 
                    );
                    cut_corners_len += 1;
                }
                axis_mask >>= 1;
            }
        } else {
            cut_corners[cut_corners_len] = Vector2::new(
                corner.x,  
                corner.y,
            ) * (light_near_z / corner.z);
            cut_corners_len += 1;
        }
    }
    if cut_corners_len == 0 {
        return None;
    }

    use polygon::Rect;

    let light_right = light_width / 2.0;
    let light_top = light_height / 2.0;
    let light_rect = Rect {
        max:  Vector2::new(light_right, light_top),
        min: -Vector2::new(light_right, light_top),
    };

    // rect of projected camera view volume
    let camera_rect = Rect::from_points(&cut_corners[..cut_corners_len]);
    if let Some(rect) = camera_rect.intersect(&light_rect) {
        Some((
            -rect.min,
            Scale2::new(light_width / rect.width(), light_height / rect.height()),
        ))
    } else {
        None
    }
}

async fn run() {
    use winit::*;

    let event_loop = event_loop::EventLoop::new();
    let window = window::Window::new(&event_loop).unwrap();
    window.set_inner_size(PhysicalSize::new(1000, 1000));

    let instance = wgpu::Instance::new(InstanceDescriptor::default());

    let surface = unsafe { instance.create_surface(&window) }.unwrap();
    let adapter = instance.request_adapter(&RequestAdapterOptions::default()).await.unwrap();

    let (device, queue) = adapter.request_device(&DeviceDescriptor::default(), None).await.unwrap();
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
    let mut config = SurfaceConfiguration {
        usage: TextureUsages::RENDER_ATTACHMENT,
        format: surface_format,
        width: size.width,
        height: size.height,
        present_mode: surface_caps.present_modes[0],
        alpha_mode: surface_caps.alpha_modes[0],
        view_formats: vec![],
    };
    surface.configure(&device, &config);

    let (mut depth_texture, mut depth_texture_view) = create_depth_texture(&device, size.width, size.height);
    
    let light_bind_group_layout =
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        entries: &[
            BindGroupLayoutEntry { // camera bind group
                binding: 0,
                visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            BindGroupLayoutEntry { // light bind group
                binding: 1,
                visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            BindGroupLayoutEntry { // shadow map bind group
                binding: 2,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Depth,
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry { // shadow sampler bind group
                binding: 3,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
        ],
        label: Some("light bind group layout"),
    });

    let shadow_bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        entries: &[
            BindGroupLayoutEntry { // light bind group
                binding: 0,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
        label: Some("shadow bind group layout"),
    });

    let light_shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("Lighting Shader"),
        source: ShaderSource::Wgsl(include_str!("light.wgsl").into()),
    });

    let shadow_shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("Full shadow Shader"),
        source: ShaderSource::Wgsl(include_str!("shadow.wgsl").into()),
    });


    let shadow_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("Shadow Render Pipeline Layout"),
        bind_group_layouts: &[&shadow_bind_group_layout],
        push_constant_ranges: &[],
    });

    let light_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("Light Render Pipeline Layout"),
        bind_group_layouts: &[&light_bind_group_layout],
        push_constant_ranges: &[],
    });

    let depth_stencil = DepthStencilState {
        format: DEPTH_FORMAT,
        depth_write_enabled: true,
        depth_compare: CompareFunction::Greater, // 1.
        stencil: StencilState::default(), // 2.
        bias: DepthBiasState::default(),
    };
    let multisample = MultisampleState {
        count: 1, // 2.
        mask: !0, // 3.
        alpha_to_coverage_enabled: false, // 4.
    };

    let shadow_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("Shadow mapping pipeline"),
        layout: Some(&shadow_pipeline_layout),
        vertex: VertexState {
            module: &shadow_shader,
            entry_point: "vs_main",
            buffers: &[
                VERTEX_LAYOUT,
                INSTANCE_LAYOUT,
            ],
        },
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList, // 1.
            strip_index_format: None,
            front_face: FrontFace::Ccw, // 2.
            cull_mode: Some(Face::Back),
            // Setting this to anything other than Fill requires Features::NON_FILL_POLYGON_MODE
            polygon_mode: PolygonMode::Fill,
            // Requires Features::DEPTH_CLIP_CONTROL
            unclipped_depth: false,
            // Requires Features::CONSERVATIVE_RASTERIZATION
            conservative: false,
        },
        depth_stencil: Some(depth_stencil.clone()),
        multisample,
        fragment: None,
        multiview: None,
    });

    let light_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("Light Pipeline"),
        layout: Some(&light_pipeline_layout),
        vertex: VertexState {
            module: &light_shader,
            entry_point: "vs_main", // 1.
            buffers: &[
                VERTEX_LAYOUT,
                INSTANCE_LAYOUT,
            ], // 2.
        },
        fragment: Some(FragmentState { // 3.
            module: &light_shader,
            entry_point: "fs_main",
            targets: &[Some(ColorTargetState { // 4.
                format: config.format,
                blend: Some(BlendState::REPLACE),
                write_mask: ColorWrites::ALL,
            })],
        }),
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList, // 1.
            strip_index_format: None,
            front_face: FrontFace::Ccw, // 2.
            cull_mode: Some(Face::Back),
            // Setting this to anything other than Fill requires Features::NON_FILL_POLYGON_MODE
            polygon_mode: PolygonMode::Fill,
            // Requires Features::DEPTH_CLIP_CONTROL
            unclipped_depth: false,
            // Requires Features::CONSERVATIVE_RASTERIZATION
            conservative: false,
        },
        depth_stencil: Some(depth_stencil.clone()), // 1.
        multisample,
        multiview: None, // 5.
    });

    let vertex_buffer = device.create_buffer_init(&util::BufferInitDescriptor {
        label: Some("Vertex buffer"),
        contents: bytemuck::cast_slice(&[
            Vertex {
                position: [-0.5, -0.5, -0.5],
            },
            Vertex {
                position: [-0.5, -0.5, 0.5],
            },
            Vertex {
                position: [-0.5, 0.5, -0.5],
            },
            Vertex {
                position: [-0.5, 0.5, 0.5],
            },
            Vertex {
                position: [0.5, -0.5, -0.5],
            },
            Vertex {
                position: [0.5, -0.5, 0.5],
            },
            Vertex {
                position: [0.5, 0.5, -0.5],
            },
            Vertex {
                position: [0.5, 0.5, 0.5],
            },
        ]),
        usage: BufferUsages::VERTEX,
    });

    let indices: &[u16] = &[
        0b000, 0b100, 0b010,
        0b110, 0b010, 0b100,

        0b000, 0b010, 0b001,
        0b011, 0b001, 0b010,

        0b000, 0b001, 0b100,
        0b101, 0b100, 0b001,

        0b110 ^ 0b111, 0b100 ^ 0b111, 0b010 ^ 0b111,
        0b000 ^ 0b111, 0b010 ^ 0b111, 0b100 ^ 0b111,

        0b011 ^ 0b111, 0b010 ^ 0b111, 0b001 ^ 0b111,
        0b000 ^ 0b111, 0b001 ^ 0b111, 0b010 ^ 0b111,

        0b101 ^ 0b111, 0b001 ^ 0b111, 0b100 ^ 0b111,
        0b000 ^ 0b111, 0b100 ^ 0b111, 0b001 ^ 0b111,
    ];
    let index_buffer = device.create_buffer_init(&util::BufferInitDescriptor {
        label: Some("Index buffer"),
        contents: bytemuck::cast_slice(indices),
        usage: BufferUsages::INDEX,
    });

    let camera_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("Camera Uniform Buffer"),
        size: size_of::<CameraRaw>() as BufferAddress,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let light_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("Light Uniform Buffer"),
        size: size_of::<CameraRaw>() as BufferAddress,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let shadow_texture_width = 1024;
    let shadow_texture_height = 1024;
    let shadow_texture = device.create_texture(&TextureDescriptor {
        label: Some("Shadow/Light depth texture"),
        size: Extent3d {
            width: shadow_texture_width,
            height: shadow_texture_height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let shadow_texture_view = shadow_texture.create_view(&TextureViewDescriptor::default());
    let shadow_sampler = device.create_sampler(&SamplerDescriptor {
        label: Some("Shadow sampler"),
        ..Default::default()
    });

    let shadow_bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("shadow bind group"),
        layout: &shadow_bind_group_layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: light_buffer.as_entire_binding(),
            }
        ],
    });
    let light_bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("light bind group"),
        layout: &light_bind_group_layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: light_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 2,
                resource: BindingResource::TextureView(&shadow_texture_view),
            },
            BindGroupEntry {
                binding: 3,
                resource: BindingResource::Sampler(&shadow_sampler),
            },
        ],
    });

    let instant = std::time::Instant::now();
    let mut last_frame_time = instant.elapsed().as_secs_f32();
    let mut delta_frame_time = 0.0;
    let mut time_rendered = 0.0;
    let mut frames = 0;

    let mut input = input::InputState::new();

    let mut camera = Camera {
        translation: Vector3::new(0.0, 0.0, -1.5),
        forward: Vector3::new(0.0, 0.0, 1.0),
        z_to_x: 0.0,
        xz_to_y: 0.0,
        near_z: 1.0,
        // remember this affects
        far_z: 10.0,
        width: 2.0 * size.width as f32 / size.height as f32,
        height: 2.0,
    };
    let mut light = Light {
        translation: Vector3::new(0.0, 0.0, -100.0),
        near_z: 4.0,
        width: 1.0,
        height: 1.0,
    };

    let mut instances = vec![
        Instance { 
            translation: Vector3::IDENTITY, 
            rotation: math::Rotor::IDENTITY,
            scale: math::Scale3::new(light.width * 1.01, light.height * 1.01, 0.1)
        },
        Instance {
            translation: Vector3::new(0.0, 0.0, 4.0), 
            rotation: math::BiVector3::new(0.0, -0.05, 0.0).exp(), 
            scale: math::Scale3::new(4.0, 4.0, 1.0)
        },
        Instance {
            translation: Vector3::new(-3.0, -1.0, 6.0), 
            rotation: math::BiVector3::new(0.8, 0.3, 0.9).exp(), 
            scale: math::Scale3::new(4.0, 4.0, 1.0)
        },
        Instance {
            translation: Vector3::new(0.0, 0.0, 10.0), 
            rotation: math::BiVector3::new(0.0, 0.0, 0.0).exp(), 
            scale: math::Scale3::new(10.0, 30.0, 0.1)
        },
        Instance {
            translation: Vector3::new(0.0, 10.0, -3.0), 
            rotation: math::BiVector3::new(0.3, -0.4, 0.2).exp(), 
            scale: math::Scale3::new(5.0, 2.0, 1.0)
        },
        Instance {
            translation: Vector3::new(2.0, 5.0, -3.0), 
            rotation: math::BiVector3::new(0.7, -0.4, -0.3).exp(), 
            scale: math::Scale3::new(4.0, 3.0, 1.0)
        },
        Instance {
            translation: Vector3::new(-3.0, 5.0, 0.0), 
            rotation: math::BiVector3::new(-0.3, 0.2, -0.7).exp(), 
            scale: math::Scale3::new(4.0, 1.0, 2.0)
        },
        Instance {
            translation: Vector3::new(3.0, 1.0, 4.0), 
            rotation: math::BiVector3::new(0.1, -0.05, 0.0).exp(), 
            scale: math::Scale3::new(1.0, 5.0, 0.2)
        },
    ];
    
    let instance_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("Instance buffer"),
        size: (instances.len() * size_of::<InstanceRaw>()) as BufferAddress,
        usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let mut shadow_fit = false;

    let camera_translation_speed = 3.0;
    let camera_rotation_speed = 1.5;
    event_loop.run(move |event: event::Event<'_, ()>, _, control_flow| {
        use winit::{event_loop::*, event::*};

        match event {
            Event::RedrawRequested(..) => {
                queue.write_buffer(
                    &camera_buffer, 
                    0, 
                    bytes_of(&camera.to_raw()),
                );

                queue.write_buffer(
                    &instance_buffer, 
                    0,
                    bytemuck::cast_slice(&instances
                        .iter()
                        .map(|i| i.to_raw())
                        .collect::<Vec<_>>()
                    )
                );

                frames += 1;
                let frame_time = instant.elapsed().as_secs_f32();
                delta_frame_time = frame_time - last_frame_time;
                last_frame_time = frame_time;
                time_rendered += delta_frame_time;

                // window.set_title(&format!("fps: {}, average fps: {}, time rendered: {}", 
                //     (1.0 / delta_frame_time) as u32,
                //     (frames as f32 / time_rendered) as u32,
                //     time_rendered,
                // ));

                let output = surface.get_current_texture().unwrap();
                let output_view = output.texture.create_view(&TextureViewDescriptor::default());
                let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
                    label: Some("command block")
                });

                let mut light_view = light.compute_view();
                let fit = compute_camera_fit_on_light_plane(
                    &camera.compute_model(), 
                    camera.far_z, 
                    camera.near_z, 
                    camera.width, 
                    camera.height, 
                    &light_view, 
                    light.near_z, 
                    light.width, 
                    light.height,
                );

                if let Some((trans, scale)) = fit {
                    if shadow_fit {
                        light_view = *light_view
                        .translate(&Vector3::new(trans.x, trans.y, 0.0))
                        .scale(&Scale3::new(scale.x, scale.y, 1.0))
                        .translate(&(Vector3::new(-light.width / 2.0, -light.height / 2.0, 0.0)));

                        window.set_title(&format!("trans: ({}, {}), scale: ({}, {})",
                            trans.x, trans.y,
                            scale.x, scale.y,
                        ));
                    } else {
                        window.set_title(&format!(""));
                    }
                }

                light_view = *light_view
                    .scale(&Scale3::new(
                        2.0 * light.near_z / light.width, 
                        2.0 * light.near_z / light.height, 
                        1.0
                    ));

                queue.write_buffer(
                    &light_buffer, 
                    0,
                    bytes_of(&light.into_raw(&light_view)), 
                );

                if fit.is_some() {
                    let mut shadow_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                        label: None,
                        color_attachments: &[
                        ],
                        depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                            view: &shadow_texture_view,
                            depth_ops: Some(Operations {
                                load: LoadOp::Clear(0.0),
                                store: true,
                            }),
                            stencil_ops: None,
                        }),
                    });

                    shadow_pass.set_pipeline(&shadow_pipeline);
                    shadow_pass.set_bind_group(0, &shadow_bind_group, &[]);

                    shadow_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                    shadow_pass.set_vertex_buffer(1, instance_buffer.slice(..));
                    shadow_pass.set_index_buffer(index_buffer.slice(..), IndexFormat::Uint16);

                    shadow_pass.draw_indexed(
                        0..indices.len() as u32,
                        0,
                        1..instances.len() as u32,
                    );
                }

                {
                    let mut light_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                        label: Some("light pass"),
                        color_attachments: &[
                            Some(RenderPassColorAttachment {
                                view: &output_view,
                                resolve_target: None,
                                ops: Operations {
                                    load: LoadOp::Clear(Color{
                                        r: 0.05,
                                        g: 0.02,
                                        b: 0.07,
                                        a: 1.0,
                                    }),
                                    store: true,
                                },
                            }),
                        ],
                        depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                            view: &depth_texture_view,
                            depth_ops: Some(Operations {
                                load: LoadOp::Clear(0.0),
                                store: true,
                            }),
                            stencil_ops: None,
                        }),
                    });

                    light_pass.set_pipeline(&light_pipeline);
                    light_pass.set_bind_group(0, &light_bind_group, &[]);

                    light_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                    light_pass.set_vertex_buffer(1, instance_buffer.slice(..));
                    light_pass.set_index_buffer(index_buffer.slice(..), IndexFormat::Uint16);

                    light_pass.draw_indexed(
                        0..indices.len() as u32, 
                        0, 
                        0..instances.len() as u32
                    );
                }

                
                queue.submit(std::iter::once(encoder.finish()));
                output.present();
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
                        camera.width = camera.height * config.width as f32 / size.height as f32;
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
                },
                _ => {}
            }
            Event::MainEventsCleared => {
                if config.width == 0 || config.height == 0 {
                    return;
                }

                instances[0].translation = light.translation;
                instances[0].translation.z += light.near_z + 0.001;

                camera.update_forward();

                use VirtualKeyCode::*;
                let w_pressed = input.is_key_pressed(W);
                let s_pressed = input.is_key_pressed(S);
                let d_pressed = input.is_key_pressed(D);
                let a_pressed = input.is_key_pressed(A);

                let up_pressed = input.is_key_pressed(Up);
                let down_pressed = input.is_key_pressed(Down);
                let right_pressed = input.is_key_pressed(Right);
                let left_pressed = input.is_key_pressed(Left);

                let delta_translation = camera.forward * camera_translation_speed * delta_frame_time;
                let delta_rotation = camera_rotation_speed * delta_frame_time;

                let e_pressed = input.is_key_pressed(E);
                let r_pressed = input.is_key_pressed(R);

                if w_pressed && !s_pressed {
                    camera.translation += delta_translation;
                } else if !w_pressed && s_pressed {
                    camera.translation -= delta_translation;
                }
                if d_pressed && !a_pressed {
                    camera.translation.z -= delta_translation.x;
                    camera.translation.x += delta_translation.z;
                } else if !d_pressed && a_pressed {
                    camera.translation.z += delta_translation.x;
                    camera.translation.x -= delta_translation.z;
                }
                if up_pressed && !down_pressed {
                    camera.xz_to_y += delta_rotation;
                } else if !up_pressed && down_pressed {
                    camera.xz_to_y -= delta_rotation;
                }
                if right_pressed && !left_pressed {
                    camera.z_to_x += delta_rotation;
                } else if !right_pressed && left_pressed {
                    camera.z_to_x -= delta_rotation;
                }
                if e_pressed && !r_pressed {
                    light.translation.z += 10.0 * delta_frame_time;
                } else if !e_pressed && r_pressed {
                    light.translation.z -= 10.0 * delta_frame_time;
                }

                if input.is_key_pressed(Space) && !input.was_key_pressed(Space) {
                    shadow_fit = !shadow_fit;
                }

                input.previous_keys_pressed_bitmask = input.keys_pressed_bitmask;

                window.request_redraw();
            }
            _ => {}
        }
    });
}

fn create_depth_texture(device: &Device, width: u32, height: u32) -> (Texture, TextureView) {  
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("depth texture"),
        size: Extent3d {
            width: width,
            height: height,
            depth_or_array_layers: 1,
        },
        format: DEPTH_FORMAT,
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });  

    let texture_view = texture.create_view(&TextureViewDescriptor::default());

    (texture, texture_view)
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}