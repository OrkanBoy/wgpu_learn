struct Camera {
    view_0: vec4<f32>,
    view_1: vec4<f32>,
    view_2: vec4<f32>,
    near_z: f32,
};
@group(0) @binding(0) var<uniform> light: Camera;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,  
}

struct VertexInput {
    @location(0) position: vec3<f32>,
}

struct InstanceInput {
    @location(5) model_0: vec4<f32>,
    @location(6) model_1: vec4<f32>,
    @location(7) model_2: vec4<f32>,
}

@vertex fn vs_main(
    vertex: VertexInput,
    instance: InstanceInput,
) -> VertexOutput {
    var out: VertexOutput;

    out.clip_position = vec4<f32>(apply_affine(
        light.view_0,
        light.view_1,
        light.view_2,
        apply_affine(
            instance.model_0,
            instance.model_1,
            instance.model_2,
            vertex.position,
        ),
    ), 1.0);

    out.clip_position.w = out.clip_position.z;
    out.clip_position.z = light.near_z;

    // textured is stored such that +y is down, so we need to invert y

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