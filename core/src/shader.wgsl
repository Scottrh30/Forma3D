// Forma3D viewport shaders: lit solids, edge overlays and the fading grid.

struct Camera {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    // xyz: camera forward, w: 1 for orthographic projection
    forward: vec4<f32>,
    // xyz: unit vector pointing towards the key light
    light_dir: vec4<f32>,
    // x: 1 when the render target is sRGB (colours must be linearised),
    // y: quick shading, z: wireframe display
    params: vec4<f32>,
    // xyz: grid fade centre, w: grid fade radius
    fade: vec4<f32>,
    // the shadow map the shadow pass is drawing
    shadow_draw: mat4x4<f32>,
    // x: number of shadow maps, y: reflections on, w: the floor plane (y = 0) is shown
    fx: vec4<f32>,
    // rgb: ambient from Dome lights; w: 1 when scene lights replace the camera light
    ambient: vec4<f32>,
    // x: number of direct lights
    light_count: vec4<f32>,
    lights: array<SceneLight, 8>,
    // world -> shadow clip space, per shadow map
    shadow_mats: array<mat4x4<f32>, 4>,
    // atlas uv offset (xy) and size (zw), per shadow map
    shadow_rects: array<vec4<f32>, 4>,
    // x: texel size (per unit distance when perspective), y: 1 perspective,
    // z: blur radius in texels, w: light index (-1: the camera light)
    shadow_info: array<vec4<f32>, 4>,
};

struct SceneLight {
    // xyz: position; w: kind (0 point, 1 spot, 2 area, 3 directional)
    pos: vec4<f32>,
    // xyz: direction the light travels; w: cos(outer cone)
    dir: vec4<f32>,
    // rgb: colour x intensity; w: cos(inner cone)
    color: vec4<f32>,
    // x: falloff radius (0 = none); y: shadow softness; z: casts shadows;
    // w: shadow map + 1 (0: none)
    params: vec4<f32>,
    // area lights: xyz half-width axis, w half height
    axis: vec4<f32>,
};

@group(0) @binding(0) var<uniform> cam: Camera;
@group(0) @binding(1) var shadow_map: texture_depth_2d;
@group(0) @binding(2) var shadow_sampler: sampler_comparison;

// Direction towards a light from `world` and how much of it arrives there
// (falloff and spot cone), ignoring the surface's orientation.
fn light_reach(light: SceneLight, world: vec3<f32>) -> vec4<f32> {
    let kind = light.pos.w;
    if (kind > 2.5) {
        return vec4<f32>(normalize(-light.dir.xyz), 1.0);
    }
    let d = light.pos.xyz - world;
    let dist = max(length(d), 1e-3);
    let l = d / dist;
    var att = 1.0;
    if (light.params.x > 0.0) {
        let t = clamp(1.0 - dist / light.params.x, 0.0, 1.0);
        att = t * t;
    }
    if (kind > 0.5 && kind < 1.5) {
        att = att * smoothstep(light.dir.w, max(light.color.w, light.dir.w + 1e-4), dot(-l, normalize(light.dir.xyz)));
    }
    return vec4<f32>(l, att);
}

// An area light as the final render sees it: light from every point of its
// rectangle (4 x 4 points), each shining from the front only. Returns the
// diffuse amount (x) and, in yzw, the direction to the point of the
// rectangle nearest the mirror direction (for the highlight).
fn area_light(light: SceneLight, world: vec3<f32>, n: vec3<f32>, to_eye: vec3<f32>) -> vec4<f32> {
    let c = light.pos.xyz;
    let fwd = normalize(light.dir.xyz);
    let ax = light.axis.xyz;
    let ay = normalize(cross(fwd, ax)) * light.axis.w;
    var sum = 0.0;
    for (var j = 0; j < 4; j = j + 1) {
        for (var i = 0; i < 4; i = i + 1) {
            let u = (f32(i) + 0.5) / 4.0 * 2.0 - 1.0;
            let v = (f32(j) + 0.5) / 4.0 * 2.0 - 1.0;
            let p = c + ax * u + ay * v;
            let d = p - world;
            let dist = max(length(d), 1e-3);
            let l = d / dist;
            let emit = dot(-l, fwd);
            if (emit > 0.0) {
                var att = 1.0;
                if (light.params.x > 0.0) {
                    let t = clamp(1.0 - dist / light.params.x, 0.0, 1.0);
                    att = t * t;
                }
                sum = sum + max(dot(n, l), 0.0) * sqrt(emit) * att;
            }
        }
    }
    // The highlight: where the mirror ray meets the light's plane, clamped
    // to the rectangle.
    let r = reflect(-to_eye, n);
    let denom = dot(r, fwd);
    var q = c;
    if (abs(denom) > 1e-4) {
        let t = dot(c - world, fwd) / denom;
        if (t > 0.0) {
            q = world + r * t;
        }
    }
    let rel = q - c;
    let wx = max(length(ax), 1e-4);
    let wy = max(light.axis.w, 1e-4);
    let px = clamp(dot(rel, ax / wx), -wx, wx);
    let py = clamp(dot(rel, ay / wy), -wy, wy);
    let nearest = c + ax / wx * px + ay / wy * py;
    return vec4<f32>(sum / 16.0, normalize(nearest - world));
}

// Lighting from the scene's lights: Dome ambient plus each direct light, each
// shadowed by its own shadow map when it has one.
fn scene_lighting(base: vec3<f32>, world: vec3<f32>, n: vec3<f32>, to_eye: vec3<f32>) -> vec3<f32> {
    let sky = 0.5 + 0.5 * n.y;
    // Light arriving at the surface (irradiance), and highlights.
    var light_in = cam.ambient.rgb * mix(0.35, 1.0, sky) + vec3<f32>(0.04);
    var highlight = vec3<f32>(0.0);
    let count = i32(cam.light_count.x);
    for (var i = 0; i < count; i = i + 1) {
        let light = cam.lights[i];
        let kind = light.pos.w;
        var diffuse: f32;
        var l: vec3<f32>;
        var att = 1.0;
        if (kind > 1.5 && kind < 2.5) {
            let a = area_light(light, world, n, to_eye);
            diffuse = a.x;
            l = a.yzw;
        } else {
            let reach = light_reach(light, world);
            l = reach.xyz;
            att = reach.w;
            diffuse = max(dot(n, l), 0.0);
        }
        if (diffuse <= 0.0 || att <= 0.0) {
            continue;
        }
        var shade = 1.0;
        let map = i32(light.params.w) - 1;
        if (map >= 0) {
            shade = shadow_at(map, world, n);
        }
        let h = normalize(l + to_eye);
        var spec = pow(max(dot(n, h), 0.0), 40.0) * 0.16;
        if (kind > 1.5 && kind < 2.5) {
            spec = spec * clamp(dot(n, l) * 2.0, 0.0, 1.0);
        }
        light_in = light_in + light.color.rgb * att * shade * diffuse;
        highlight = highlight + light.color.rgb * att * shade * spec;
    }
    // The viewport draws without gamma correction: a display curve keeps
    // half-lit faces from looking much darker than under the default light.
    return base * pow(light_in, vec3<f32>(0.65)) + highlight;
}

// 1 where shadow map `k`'s light reaches `world`, 0 in full shadow (PCF,
// wider for soft lights).
fn shadow_at(k: i32, world: vec3<f32>, n: vec3<f32>) -> f32 {
    if (k >= i32(cam.fx.x)) {
        return 1.0;
    }
    let info = cam.shadow_info[k];
    var texel = info.x;
    if (info.y > 0.5) {
        // Perspective maps: texels grow with distance from the light.
        var origin = vec3<f32>(0.0);
        let li = i32(info.w);
        if (li >= 0) {
            origin = cam.lights[li].pos.xyz;
        }
        texel = info.x * distance(world, origin);
    }
    let p4 = cam.shadow_mats[k] * vec4<f32>(world + n * texel * 2.0, 1.0);
    if (p4.w <= 1e-6) {
        return 1.0;
    }
    let p = p4.xyz / p4.w;
    let local = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    if (local.x < 0.0 || local.x > 1.0 || local.y < 0.0 || local.y > 1.0 || p.z < 0.0) {
        return 1.0;
    }
    let rect = cam.shadow_rects[k];
    let uv = rect.xy + local * rect.zw;
    let step = 1.0 / f32(textureDimensions(shadow_map).x);
    let lo = rect.xy + vec2<f32>(step * 0.5);
    let hi = rect.xy + rect.zw - vec2<f32>(step * 0.5);
    var bias = 0.0008;
    if (info.y > 0.5) {
        bias = 0.00015;
    }
    let z = min(p.z, 1.0) - bias;
    let radius = info.z;
    var lit = 0.0;
    for (var y = -2; y <= 2; y = y + 1) {
        for (var x = -2; x <= 2; x = x + 1) {
            let o = vec2<f32>(f32(x), f32(y)) * 0.5 * step * radius;
            lit = lit + textureSampleCompareLevel(shadow_map, shadow_sampler, clamp(uv + o, lo, hi), z);
        }
    }
    return lit / 25.0;
}

// The default camera light's shadow (the first shadow map).
fn shadow_factor(world: vec3<f32>, n: vec3<f32>) -> f32 {
    return shadow_at(0, world, n);
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
    return cam.shadow_draw * (model * vec4<f32>(in.pos, 1.0));
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
    var dark = 0.0;
    if (cam.ambient.w > 0.5) {
        // Each light's shadow, weighted by how much of the floor's light
        // comes from it.
        var total = dot(cam.ambient.rgb, vec3<f32>(0.33)) * 0.6 + 0.02;
        var blocked = 0.0;
        let count = i32(cam.light_count.x);
        for (var i = 0; i < count; i = i + 1) {
            let light = cam.lights[i];
            var reach = light_reach(light, in.world_pos);
            var amount = max(reach.y, 0.0) * reach.w;
            if (light.pos.w > 1.5 && light.pos.w < 2.5) {
                amount = area_light(light, in.world_pos, up, up).x;
            }
            let w = amount * dot(light.color.rgb, vec3<f32>(0.33));
            total = total + w;
            let map = i32(light.params.w) - 1;
            if (map >= 0 && w > 0.0) {
                blocked = blocked + w * (1.0 - shadow_at(map, in.world_pos, up));
            }
        }
        dark = blocked / total * 0.55;
    } else {
        let l = normalize(cam.light_dir.xyz);
        // Light from below the floor casts nothing onto it.
        if (l.y <= 0.02) {
            discard;
        }
        dark = (1.0 - shadow_factor(in.world_pos, up)) * 0.42;
    }
    let d = distance(in.world_pos, cam.fade.xyz);
    let fade = 1.0 - smoothstep(cam.fade.w * 0.35, cam.fade.w, d);
    let a = dark * fade;
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
    if (in.flags.y > 0.5 || cam.params.z > 0.5) {
        // Pure wireframe mode (and line-only objects): lines carry the object colour.
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
