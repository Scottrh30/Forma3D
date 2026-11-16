// Forma3D viewport shaders: lit solids, edge overlays and the fading grid.

struct Camera {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    // xyz: camera forward, w: 1 for orthographic projection
    forward: vec4<f32>,
    // xyz: unit vector pointing towards the key light
    light_dir: vec4<f32>,
    // x: 1 when the render target is sRGB (colours must be linearised)
    params: vec4<f32>,
    // xyz: grid fade centre, w: grid fade radius
    fade: vec4<f32>,
    // world -> shadow map clip space
    light_view_proj: mat4x4<f32>,
    // x: shadows on, y: reflections on, z: shadow texel size (world units),
    // w: the floor plane (y = 0) is shown
    fx: vec4<f32>,
    // rgb: ambient from Dome lights; w: 1 when scene lights replace the camera light
    ambient: vec4<f32>,
    // x: number of direct lights
    light_count: vec4<f32>,
    lights: array<SceneLight, 8>,
};

struct SceneLight {
    // xyz: position; w: kind (0 point, 1 spot, 2 area, 3 directional)
    pos: vec4<f32>,
    // xyz: direction the light travels; w: cos(outer cone)
    dir: vec4<f32>,
    // rgb: colour x intensity; w: cos(inner cone)
    color: vec4<f32>,
    // x: falloff radius (0 = none); y: area half-size
    params: vec4<f32>,
};

// Lighting from the scene's lights: Dome ambient plus each direct light. The
// first light casts the shadows.
fn scene_lighting(base: vec3<f32>, world: vec3<f32>, n: vec3<f32>, to_eye: vec3<f32>) -> vec3<f32> {
    let sky = 0.5 + 0.5 * n.y;
    // Light arriving at the surface (irradiance), and highlights.
    var light_in = cam.ambient.rgb * mix(0.35, 1.0, sky) + vec3<f32>(0.04);
    var highlight = vec3<f32>(0.0);
    let count = i32(cam.light_count.x);
    for (var i = 0; i < count; i = i + 1) {
        let light = cam.lights[i];
        let kind = light.pos.w;
        var l: vec3<f32>;
        var att = 1.0;
        var dist = 1.0;
        if (kind > 2.5) {
            l = normalize(-light.dir.xyz);
        } else {
            let d = light.pos.xyz - world;
            dist = max(length(d), 1e-3);
            l = d / dist;
            if (light.params.x > 0.0) {
                att = clamp(1.0 - (dist / light.params.x) * (dist / light.params.x), 0.0, 1.0);
            }
            if (kind > 0.5 && kind < 1.5) {
                // Spot cone.
                att = att * smoothstep(light.dir.w, light.color.w, dot(-l, normalize(light.dir.xyz)));
            }
            if (kind > 1.5) {
                // Area: one-sided, towards its +Z.
                att = att * clamp(dot(-l, normalize(light.dir.xyz)) * 1.5 + 0.15, 0.0, 1.0);
            }
        }
        var ndl = dot(n, l);
        if (kind > 1.5 && kind < 2.5) {
            // A large source near the surface wraps light further round.
            let wrap = clamp(light.params.y / dist, 0.0, 0.6);
            ndl = (ndl + wrap) / (1.0 + wrap);
        }
        let diffuse = max(ndl, 0.0);
        if (diffuse <= 0.0 || att <= 0.0) {
            continue;
        }
        var shade = 1.0;
        if (i == 0) {
            shade = shadow_factor(world, n);
        }
        let h = normalize(l + to_eye);
        let spec = pow(max(dot(n, h), 0.0), 40.0) * 0.16;
        light_in = light_in + light.color.rgb * att * shade * diffuse;
        highlight = highlight + light.color.rgb * att * shade * spec;
    }
    // The viewport draws without gamma correction: a display curve keeps
    // half-lit faces from looking much darker than under the default light.
    return base * pow(light_in, vec3<f32>(0.65)) + highlight;
}

@group(0) @binding(0) var<uniform> cam: Camera;
@group(0) @binding(1) var shadow_map: texture_depth_2d;
@group(0) @binding(2) var shadow_sampler: sampler_comparison;

// 1 where the key light reaches `world`, 0 in full shadow (3x3 PCF).
fn shadow_factor(world: vec3<f32>, n: vec3<f32>) -> f32 {
    if (cam.fx.x < 0.5) {
        return 1.0;
    }
    let texel = cam.fx.z;
    let p = cam.light_view_proj * vec4<f32>(world + n * texel * 2.0, 1.0);
    let uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || p.z < 0.0) {
        return 1.0;
    }
    let z = min(p.z, 1.0) - 0.0008;
    let step = 1.0 / f32(textureDimensions(shadow_map).x);
    var lit = 0.0;
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let o = vec2<f32>(f32(x), f32(y)) * step * 1.2;
            lit = lit + textureSampleCompareLevel(shadow_map, shadow_sampler, uv + o, z);
        }
    }
    return lit / 9.0;
}

// A soft photo-studio surround: dark floor, bright sky and two softboxes
// fixed in the world, so reflections slide over objects as the view orbits.
fn studio(r: vec3<f32>) -> vec3<f32> {
    let floor = mix(vec3<f32>(0.10, 0.10, 0.11), vec3<f32>(0.24, 0.24, 0.25), smoothstep(-1.0, 0.0, r.y));
    let sky = mix(vec3<f32>(0.52, 0.54, 0.57), vec3<f32>(0.80, 0.82, 0.86), smoothstep(0.0, 0.9, r.y));
    var c = mix(floor, sky, smoothstep(-0.03, 0.05, r.y));
    let az = atan2(r.x, r.z);
    let box_a = (1.0 - smoothstep(0.18, 0.34, abs(az - 0.9))) * smoothstep(0.12, 0.28, r.y) * (1.0 - smoothstep(0.6, 0.8, r.y));
    let box_b = (1.0 - smoothstep(0.14, 0.28, abs(az + 2.2))) * smoothstep(0.08, 0.22, r.y) * (1.0 - smoothstep(0.5, 0.7, r.y));
    c = c + vec3<f32>(1.0) * box_a * 0.9 + vec3<f32>(0.88, 0.93, 1.0) * box_b * 0.6;
    return c;
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

// Colours below are authored as display (sRGB-encoded) values. egui prefers a
// non-sRGB framebuffer, in which case they are written as-is.
fn to_target(c: vec3<f32>) -> vec3<f32> {
    if (cam.params.x > 0.5) {
        return srgb_to_linear(c);
    }
    return c;
}

struct MeshIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) m0: vec4<f32>,
    @location(3) m1: vec4<f32>,
    @location(4) m2: vec4<f32>,
    @location(5) m3: vec4<f32>,
    @location(6) n0: vec4<f32>,
    @location(7) n1: vec4<f32>,
    @location(8) n2: vec4<f32>,
    @location(9) color: vec4<f32>,
    // x: 0 none / 1 selected / 2 active, y: 1 when drawing pure wireframe
    @location(10) flags: vec4<f32>,
};

struct MeshOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) flags: vec4<f32>,
};

fn transform(in: MeshIn) -> MeshOut {
    let model = mat4x4<f32>(in.m0, in.m1, in.m2, in.m3);
    let world = model * vec4<f32>(in.pos, 1.0);
    let normal_matrix = mat3x3<f32>(in.n0.xyz, in.n1.xyz, in.n2.xyz);
    var out: MeshOut;
    out.clip = cam.view_proj * world;
    out.world_pos = world.xyz;
    out.normal = normal_matrix * in.normal;
    out.color = in.color;
    out.flags = in.flags;
    return out;
}

@vertex
fn vs_solid(in: MeshIn) -> MeshOut {
    return transform(in);
}

@fragment
fn fs_solid(in: MeshOut) -> @location(0) vec4<f32> {
    var n = normalize(in.normal);
    var to_eye: vec3<f32>;
    if (cam.forward.w > 0.5) {
        to_eye = -cam.forward.xyz;
    } else {
        to_eye = normalize(cam.eye.xyz - in.world_pos);
    }
    // Two-sided lighting: planes are visible from both sides.
    if (dot(n, to_eye) < 0.0) {
        n = -n;
    }
    if (cam.params.y > 0.5) {
        // Quick shading: one light at the camera, no highlights or effects.
        let facing = clamp(dot(n, to_eye), 0.0, 1.0);
        let quick = in.color.rgb * (0.22 + 0.78 * facing);
        return vec4<f32>(to_target(clamp(quick, vec3<f32>(0.0), vec3<f32>(1.0))), 1.0);
    }
    var lit: vec3<f32>;
    if (cam.ambient.w > 0.5) {
        lit = scene_lighting(in.color.rgb, in.world_pos, n, to_eye);
    } else {
        let l = normalize(cam.light_dir.xyz);
        let diffuse = max(dot(n, l), 0.0);
        var shadow = 1.0;
        if (diffuse > 0.0) {
            shadow = shadow_factor(in.world_pos, n);
        }
        let sky = 0.5 + 0.5 * n.y;
        let ambient = mix(0.30, 0.46, sky);
        let h = normalize(l + to_eye);
        let spec = pow(max(dot(n, h), 0.0), 40.0) * 0.16 * shadow;
        lit = in.color.rgb * (ambient + 0.78 * diffuse * shadow) + vec3<f32>(spec);
    }
    if (cam.fx.y > 0.5) {
        // Fresnel-weighted reflection of the studio surround.
        let ndv = clamp(dot(n, to_eye), 0.0, 1.0);
        let fresnel = 0.04 + 0.96 * pow(1.0 - ndv, 5.0);
        let env = studio(reflect(-to_eye, n));
        let amount = clamp(0.3 + 0.62 * fresnel, 0.0, 0.85);
        lit = mix(lit, env * mix(vec3<f32>(1.0), in.color.rgb, 0.45), amount);
    }
    return vec4<f32>(to_target(clamp(lit, vec3<f32>(0.0), vec3<f32>(1.0))), 1.0);
}

// Depth-only pass into the shadow map.
@vertex
fn vs_shadow(in: MeshIn) -> @builtin(position) vec4<f32> {
    let model = mat4x4<f32>(in.m0, in.m1, in.m2, in.m3);
    return cam.light_view_proj * (model * vec4<f32>(in.pos, 1.0));
}

// The floor (y = 0) as an invisible shadow catcher: a quad under the grid
// that only darkens where objects block the key light.
struct CatcherOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
};

@vertex
fn vs_catcher(@builtin(vertex_index) i: u32) -> CatcherOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    let c = corners[i] * cam.fade.w;
    let p = vec3<f32>(cam.fade.x + c.x, 0.0, cam.fade.z + c.y);
    var out: CatcherOut;
    out.clip = cam.view_proj * vec4<f32>(p, 1.0);
    out.clip.z = out.clip.z - out.clip.w * 3.0e-6;
    out.world_pos = p;
    return out;
}

@fragment
fn fs_catcher(in: CatcherOut) -> @location(0) vec4<f32> {
    let up = vec3<f32>(0.0, 1.0, 0.0);
    let l = normalize(cam.light_dir.xyz);
    // Light from below the floor casts nothing onto it.
    if (l.y <= 0.02) {
        discard;
    }
    let shade = 1.0 - shadow_factor(in.world_pos, up);
    let d = distance(in.world_pos, cam.fade.xyz);
    let fade = 1.0 - smoothstep(cam.fade.w * 0.35, cam.fade.w, d);
    let a = shade * 0.42 * fade;
    if (a < 0.004) {
        discard;
    }
    return vec4<f32>(to_target(vec3<f32>(0.02, 0.02, 0.025)), a);
}

@fragment
fn fs_floor_depth(in: CatcherOut) -> @location(0) vec4<f32> {
    return vec4<f32>(0.0);
}

@vertex
fn vs_edge(in: MeshIn) -> MeshOut {
    var out = transform(in);
    // Pull edges slightly towards the camera so they win the depth test against
    // the faces they belong to.
    out.clip.z = out.clip.z - out.clip.w * 1.2e-5;
    return out;
}

@fragment
fn fs_edge(in: MeshOut) -> @location(0) vec4<f32> {
    var rgb = vec3<f32>(0.03, 0.03, 0.035);
    var a = 0.42;
    if (in.flags.y > 0.5) {
        // Pure wireframe mode: lines carry the object colour.
        rgb = mix(in.color.rgb, vec3<f32>(1.0), 0.35);
        a = 0.95;
    }
    if (in.flags.x > 1.5) {
        rgb = vec3<f32>(1.0, 0.80, 0.25);
        a = 1.0;
    } else if (in.flags.x > 0.5) {
        rgb = vec3<f32>(1.0, 0.58, 0.12);
        a = 1.0;
    }
    return vec4<f32>(to_target(rgb), a);
}

struct GridIn {
    @location(0) pos: vec3<f32>,
    @location(1) color: vec4<f32>,
};

struct GridOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_grid(in: GridIn) -> GridOut {
    var out: GridOut;
    out.clip = cam.view_proj * vec4<f32>(in.pos, 1.0);
    out.clip.z = out.clip.z - out.clip.w * 6.0e-6;
    out.world_pos = in.pos;
    out.color = in.color;
    return out;
}

@fragment
fn fs_grid(in: GridOut) -> @location(0) vec4<f32> {
    let d = distance(in.world_pos, cam.fade.xyz);
    let fade = 1.0 - smoothstep(cam.fade.w * 0.35, cam.fade.w, d);
    return vec4<f32>(to_target(in.color.rgb), in.color.a * fade);
}
