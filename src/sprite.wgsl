struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec3<f32>,
}

struct Camera {
    aspect_ratio: f32,
}

@group(0) @binding(0)
var<uniform> camera: Camera;

@vertex
fn vs_main(
    @location(0) vertex_position: vec2<f32>,
    @location(1) boid_position: vec2<f32>,
    @location(2) boid_velocity: vec2<f32>,
) -> VertexOut {
    var out: VertexOut;
    let orientation = normalize(boid_velocity);
    out.position.x = 
        orientation.x * vertex_position.x - orientation.y * vertex_position.y;
    out.position.y = 
        orientation.y * vertex_position.x + orientation.x * vertex_position.y;
    out.position.x += boid_position.x;
    out.position.y += boid_position.y;


    // todo: make tip vertex different color
    // todo: color hue by rotation around (r: 1, g: 1, b:1)
    out.color = vec3<f32>(orientation.x * orientation.x, 0.0, orientation.y * orientation.y);

    out.position.z = 0.0;
    out.position.w = 1.0;
    out.position.y *= -1.0;
    out.position.y *= camera.aspect_ratio;
    return out;
}

@fragment
fn fs_main(vertex: VertexOut) -> @location(0) vec4<f32> {
    return vec4<f32>(vertex.color, 1.0);
}