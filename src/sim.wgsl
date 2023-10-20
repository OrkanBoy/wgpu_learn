struct SimParams {
    dt: f32,
    rule1d_sqr: f32,
    rule2d_sqr: f32,
    rule1s: f32,
    rule2s: f32,
}

struct Boid {
    position: vec2<f32>,
    velocity: vec2<f32>,
}

// boids_now should be read but error appears
// BufferUses(STORAGE_READ_WRITE) is an exclusive
// usage and cannot be used with any other usages
// within the usage scope (renderpass or compute dispatch).
@group(0) @binding(0) var<storage, read_write> boids_now: array<Boid>;
@group(0) @binding(1) var<storage, read_write> boids_next: array<Boid>;
@group(0) @binding(2) var<uniform> sim_params: SimParams;

@compute @workgroup_size(64, 1, 1)
fn main(@builtin(global_invocation_id) global_invocation_id: vec3<u32>) {
    let index = global_invocation_id.x;
    if index >= arrayLength(&boids_now) {
        return;
    }

    var boid = boids_now[index];
    var c_pos = vec2(0.0); 
    var c_vel = vec2(0.0);
    var c_pos_count = 0;
    var c_vel_count = 0;

    for (var i = 0u; i < arrayLength(&boids_now); i += 1u) {
        if i == index {
            continue;
        }

        let other_boid = boids_now[index];
        let diff = other_boid.position - boid.position;
        let dst_sqr = dot(diff, diff);

        if dst_sqr < sim_params.rule1d_sqr {
            c_pos += other_boid.position;
            c_pos_count += 1;
        }
        if dst_sqr < sim_params.rule2d_sqr {
            c_vel += other_boid.velocity;
            c_pos_count += 1;
        }
    }
    if c_pos_count != 0 {
        c_pos /= f32(c_pos_count);
    }
    if c_vel_count != 0 {
        c_vel /= f32(c_vel_count);
    }

    boid.velocity += 
        (c_pos - boid.position) * sim_params.rule1s +
        c_vel * sim_params.rule2s;

    boid.velocity = normalize(boid.velocity);

    boid.position += boid.velocity * sim_params.dt;

    if boid.position.x > 1.0 {
        boid.position.x = -1.0;
    } else if boid.position.x < -1.0 {
        boid.position.x = 1.0;
    }

    if boid.position.y > 1.0 {
        boid.position.y = -1.0;
    } else if boid.position.y < -1.0 {
        boid.position.y = 1.0;
    }
    
    boids_next[index] = boid;
    return;
}