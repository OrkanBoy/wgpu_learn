struct Camera {
    view_0: vec4<f32>,
    view_1: vec4<f32>,
    view_2: vec4<f32>,
    near_z: f32,
};
@group(0) @binding(0)
var<uniform> camera: Camera;
@group(0) @binding(1)
var<uniform> light: Camera;

struct VertexIn {
    @location(0) position: vec3<f32>,
}

struct InstanceIn {
    @location(5) model_0: vec4<f32>,
    @location(6) model_1: vec4<f32>,
    @location(7) model_2: vec4<f32>,
}

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    // xy are texture coordinates for light in range (0, 0) to (1, 1)
    // +y is downwards in texture coordinates
    // z is depth value from light
    @location(0) from_light: vec3<f32>,
}

@vertex
fn vs_main(
    vertex: VertexIn,
    instance: InstanceIn,
) -> VertexOut {
    var out: VertexOut;

    let position = apply_affine(
        instance.model_0,
        instance.model_1,
        instance.model_2,
        vertex.position,
    );

    out.clip_position = vec4<f32>(
        apply_affine(
            camera.view_0,
            camera.view_1,
            camera.view_2,
            position,
        ),
        1.0,
    );

    out.from_light = apply_affine(
        light.view_0,
        light.view_1,
        light.view_2,
        position,
    );

    // prepare (-1.0, 1.0) range to (0.0, 1.0) range
    out.from_light.x = ( out.from_light.x + out.from_light.z) * 0.5;
    out.from_light.y = (-out.from_light.y + out.from_light.z) * 0.5;
    
    out.clip_position.w = out.clip_position.z;
    // using infinite reversed z for better f32 depth precision
    out.clip_position.z = camera.near_z;
    return out;
}

@group(0) @binding(2) var shadow_texture: texture_depth_2d;
@group(0) @binding(3) var shadow_sampler: sampler;

struct FragmentOut {
    @location(0) color: vec4<f32>,
}

@fragment
fn fs_main(
    in: VertexOut,
) -> FragmentOut {
    var out: FragmentOut;

    var lighting = 0.0;

    if in.from_light.z > light.near_z
    && 0.0 < in.from_light.x && in.from_light.x < in.from_light.z 
    && 0.0 < in.from_light.y && in.from_light.y < in.from_light.z {
        let depth = textureSampleLevel(
            shadow_texture, 
            shadow_sampler,
            in.from_light.xy / in.from_light.z,
            0.0,
        );

        if light.near_z + 0.001 > depth * in.from_light.z {
            lighting = 1.0;
        }
    }

    out.color = vec4(1.0) * lighting;

    return out;
}

fn apply_affine(
    _0: vec4<f32>,
    _1: vec4<f32>,
    _2: vec4<f32>,
    pos: vec3<f32>
) -> vec3<f32> {
    return vec3<f32>(
        dot(_0.xyz, pos) + _0.w,
        dot(_1.xyz, pos) + _1.w,
        dot(_2.xyz, pos) + _2.w,
    );
}