struct Boid {
    pos: vec2<f32>,
    vel: vec2<f32>,
}
struct SimParams {
    dt : f32,
    rule1_d2 : f32,
    rule2_d2 : f32,
    rule3_d2 : f32,
    rule1_w : f32,
    rule2_w : f32,
    rule3_w : f32,
}

@binding(0) @group(0) var<storage, read_write> boids_now : array<Boid>;
@binding(1) @group(0) var<storage, read_write> boids_next : array<Boid>;
@binding(2) @group(0) var<uniform> params : SimParams;

// https://github.com/austinEng/Project6-Vulkan-Flocking/blob/master/data/shaders/computeparticles/particle.comp
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id : vec3<u32>) {
    var index = id.x;

    if index >= arrayLength(&boids_now) {
        return;
    }

    var boid = boids_now[index];
    var center_mass = vec2(0.0);
    var center_vel = vec2(0.0);
    var repel_vel = vec2(0.0);
    var center_mass_count = 0;
    var center_vel_count = 0;

    var pos : vec2<f32>;
    var vel : vec2<f32>;
    var away_from: vec2<f32>;
    var d2: f32;

    for (var i = 0u; i < arrayLength(&boids_now); i += 1u) {
        if i == index {
            continue;
        }

        pos = boids_now[i].pos;
        vel = boids_now[i].vel;
        away_from = boid.pos - pos;
        d2 = dot(away_from, away_from);

        // move to center of neighbours
        if d2 < params.rule1_d2 {
            center_mass += pos;
            center_mass_count += 1;
        }

        // repelled by neighbours
        if d2 < params.rule2_d2 {
            repel_vel += away_from;
        }

        // align with neighbours
        if d2 < params.rule3_d2 {
            center_vel += vel;
            center_vel_count += 1;
        }
    }
    if center_mass_count != 0 {
        center_mass = center_mass / f32(center_mass_count) - boid.pos;
        boid.vel += center_mass * params.rule1_w;
    }
    if center_mass_count != 0 {
        center_vel /= f32(center_vel_count);
        boid.vel += center_vel * params.rule3_w;
    }
    boid.vel += repel_vel * params.rule2_w;

    // clamp velocity for a more pleasing simulation
    let boid_vel_len = length(boid.vel);
    if boid_vel_len > 0.001 {
        boid.vel = boid.vel / boid_vel_len * min(boid_vel_len, 0.1);
    } else {
        boid.vel = boids_now[index].vel;
    }
    // kinematic update
    boid.pos += boid.vel * params.dt;
    // Wrap around boundary
    if (boid.pos.x < -0.5) {
        boid.pos.x = 0.5;
    } else if (boid.pos.x > 0.5) {
        boid.pos.x = -0.5;
    }
    if (boid.pos.y < -0.5) {
        boid.pos.y = 0.5;
    } else if (boid.pos.y > 0.5) {
        boid.pos.y = -0.5;
    }
    // Write back
    boids_next[index] = boid;
}