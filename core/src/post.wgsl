// Forma3D viewport post-processing: screen-space ambient occlusion and the
// composite pass (AO blur, FXAA, tone shaping and vignette).
//
// The view is first rendered offscreen with premultiplied colour over a
// transparent background. The composite lays it over egui's viewport
// gradient with premultiplied-alpha blending.

struct Post {
    proj: mat4x4<f32>,
    inv_proj: mat4x4<f32>,
    // x: AO radius (scene units), y: AO strength, z: SSAO on, w: post effects on
    params: vec4<f32>,
    // width, height, 1 / width, 1 / height
    size: vec4<f32>,
    // x: 1 for orthographic views, y: sign of view-space depth,
    // z: screen-space reflections on
    misc: vec4<f32>,
};

@group(0) @binding(0) var<uniform> post: Post;
@group(0) @binding(1) var color_tex: texture_2d<f32>;
@group(0) @binding(2) var depth_tex: texture_2d<f32>;
@group(0) @binding(3) var ao_tex: texture_2d<f32>;
@group(0) @binding(4) var lin: sampler;

struct FsIn {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_fullscreen(@builtin(vertex_index) i: u32) -> FsIn {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    var out: FsIn;
    out.pos = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    out.uv = vec2<f32>(x, y);
    return out;
}

fn texel_count() -> vec2<i32> {
    return vec2<i32>(textureDimensions(depth_tex));
}

fn load_depth(p: vec2<i32>) -> f32 {
    let q = clamp(p, vec2<i32>(0), texel_count() - vec2<i32>(1));
    return textureLoad(depth_tex, q, 0).r;
}

// View-space position of the pixel `p` at depth `d`.
fn view_pos(p: vec2<i32>, d: f32) -> vec3<f32> {
    let uv = (vec2<f32>(p) + vec2<f32>(0.5)) / vec2<f32>(texel_count());
    let ndc = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, d, 1.0);
    let v = post.inv_proj * ndc;
    return v.xyz / v.w;
}

// Distance in front of the camera.
fn ahead(v: vec3<f32>) -> f32 {
    return v.z * post.misc.y;
}

fn hash(p: vec2<f32>) -> f32 {
    // Interleaved gradient noise (Jimenez 2014).
    return fract(52.9829189 * fract(dot(p, vec2<f32>(0.06711056, 0.00583715))));
}

// View-space normal at pixel `px` (position `p`), from the neighbour on
// whichever side is closer in depth, so silhouettes don't smear. It faces the
// camera.
fn view_normal(px: vec2<i32>, p: vec3<f32>) -> vec3<f32> {
    let pr = view_pos(px + vec2<i32>(1, 0), load_depth(px + vec2<i32>(1, 0)));
    let pl = view_pos(px - vec2<i32>(1, 0), load_depth(px - vec2<i32>(1, 0)));
    let pd = view_pos(px + vec2<i32>(0, 1), load_depth(px + vec2<i32>(0, 1)));
    let pu = view_pos(px - vec2<i32>(0, 1), load_depth(px - vec2<i32>(0, 1)));
    var dx = pr - p;
    if (abs(ahead(pl) - ahead(p)) < abs(ahead(pr) - ahead(p))) {
        dx = p - pl;
    }
    var dy = pd - p;
    if (abs(ahead(pu) - ahead(p)) < abs(ahead(pd) - ahead(p))) {
        dy = p - pu;
    }
    var n = normalize(cross(dx, dy));
    if (dot(n, -view_dir(p)) < 0.0) {
        n = -n;
    }
    return n;
}

// Direction from the camera through view-space point `p`.
fn view_dir(p: vec3<f32>) -> vec3<f32> {
    if (post.misc.x > 0.5) {
        return vec3<f32>(0.0, 0.0, post.misc.y);
    }
    return normalize(p);
}

// Pixel coordinates of view-space point `s`, or -1 off screen.
fn to_screen(s: vec3<f32>) -> vec2<f32> {
    let clip = post.proj * vec4<f32>(s, 1.0);
    if (clip.w <= 0.0) {
        return vec2<f32>(-1.0);
    }
    let ndc = clip.xy / clip.w;
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
        return vec2<f32>(-1.0);
    }
    return uv;
}

// Screen-space reflection at pixel `px`: marches the mirror ray through the
// depth buffer. rgb: what it hits (premultiplied), a: how strongly to show it.
fn ssr(px: vec2<i32>, frag: vec2<f32>) -> vec4<f32> {
    let d = load_depth(px);
    if (d >= 1.0) {
        return vec4<f32>(0.0);
    }
    let p = view_pos(px, d);
    let n = view_normal(px, p);
    let v = view_dir(p);
    let r = reflect(v, n);
    let ndv = clamp(dot(n, -v), 0.0, 1.0);
    let fresnel = 0.04 + 0.96 * pow(1.0 - ndv, 5.0);
    // The longest reach: about the scene's size on screen.
    let reach = post.params.x / 0.045 * 1.2;
    let steps = 40;
    let step_len = reach / f32(steps);
    let start = p + n * step_len * 0.2;
    var t = step_len * (0.5 + hash(frag));
    var hit_uv = vec2<f32>(-1.0);
    var prev = 0.0;
    for (var i = 0; i < steps; i = i + 1) {
        let s = start + r * t;
        let uv = to_screen(s);
        if (uv.x < 0.0) {
            break;
        }
        let sp = vec2<i32>(uv * vec2<f32>(texel_count()));
        let sd = load_depth(sp);
        if (sd < 1.0) {
            let behind = ahead(s) - ahead(view_pos(sp, sd));
            if (behind > 0.0 && behind < step_len * 2.5) {
                // Refine between the last two steps.
                var lo = prev;
                var hi = t;
                for (var k = 0; k < 5; k = k + 1) {
                    let mid = (lo + hi) * 0.5;
                    let m = start + r * mid;
                    let muv = to_screen(m);
                    if (muv.x < 0.0) {
                        break;
                    }
                    let mp = vec2<i32>(muv * vec2<f32>(texel_count()));
                    let md = load_depth(mp);
                    if (ahead(m) > ahead(view_pos(mp, md))) {
                        hi = mid;
                    } else {
                        lo = mid;
                    }
                }
                hit_uv = to_screen(start + r * hi);
                t = hi;
                break;
            }
        }
        prev = t;
        t = t + step_len * (1.0 + f32(i) * 0.04);
    }
    if (hit_uv.x < 0.0) {
        return vec4<f32>(0.0);
    }
    let hit = textureSampleLevel(color_tex, lin, hit_uv, 0.0);
    // Fade towards the screen's edges and the end of the ray.
    let edge = min(min(hit_uv.x, 1.0 - hit_uv.x), min(hit_uv.y, 1.0 - hit_uv.y));
    let fade = smoothstep(0.0, 0.08, edge) * (1.0 - smoothstep(0.6, 1.0, t / (reach * 1.8)));
    return vec4<f32>(hit.rgb, clamp(0.12 + 0.6 * fresnel, 0.0, 0.7) * fade * hit.a);
}

@fragment
fn fs_ssao(in: FsIn) -> @location(0) vec4<f32> {
    let px = vec2<i32>(in.pos.xy);
    let d = load_depth(px);
    if (d >= 1.0) {
        return vec4<f32>(1.0);
    }
    let p = view_pos(px, d);

    let n = view_normal(px, p);

    // A randomly rotated tangent frame per pixel.
    let angle = hash(in.pos.xy) * 6.2831853;
    var helper = vec3<f32>(1.0, 0.0, 0.0);
    if (abs(n.x) > 0.8) {
        helper = vec3<f32>(0.0, 1.0, 0.0);
    }
    let t0 = normalize(cross(helper, n));
    let b0 = cross(n, t0);
    let t = t0 * cos(angle) + b0 * sin(angle);
    let b = cross(n, t);

    let radius = post.params.x;
    let count = 16;
    var occlusion = 0.0;
    for (var i = 0; i < count; i = i + 1) {
        let fi = f32(i);
        // Points on a hemisphere (golden-angle spiral), denser near the centre.
        let z = 1.0 - (fi + 0.5) / f32(count);
        let r = sqrt(max(0.0, 1.0 - z * z));
        let phi = fi * 2.3999632;
        var k = vec3<f32>(cos(phi) * r, sin(phi) * r, z);
        let scale = mix(0.12, 1.0, pow((fi + 1.0) / f32(count), 2.0));
        let s = p + (t * k.x + b * k.y + n * k.z) * radius * scale;

        let clip = post.proj * vec4<f32>(s, 1.0);
        if (clip.w <= 0.0) {
            continue;
        }
        let ndc = clip.xy / clip.w;
        let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
        if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) {
            continue;
        }
        let sp = vec2<i32>(uv * vec2<f32>(texel_count()));
        let sd = load_depth(sp);
        if (sd >= 1.0) {
            continue;
        }
        let scene = view_pos(sp, sd);
        let bias = radius * 0.03;
        if (ahead(scene) < ahead(s) - bias) {
            let range = smoothstep(0.0, 1.0, radius / abs(ahead(p) - ahead(scene)));
            occlusion = occlusion + range;
        }
    }
    let ao = 1.0 - clamp(occlusion / f32(count) * post.params.y, 0.0, 1.0);
    return vec4<f32>(ao, ao, ao, 1.0);
}

// Depth-aware 5x5 blur of the AO term.
fn blurred_ao(px: vec2<i32>) -> f32 {
    let d0 = load_depth(px);
    if (d0 >= 1.0) {
        return 1.0;
    }
    let z0 = ahead(view_pos(px, d0));
    let dims = vec2<i32>(textureDimensions(ao_tex));
    var sum = 0.0;
    var weight = 0.0;
    for (var y = -2; y <= 2; y = y + 1) {
        for (var x = -2; x <= 2; x = x + 1) {
            let q = clamp(px + vec2<i32>(x, y), vec2<i32>(0), dims - vec2<i32>(1));
            let dq = load_depth(q);
            let zq = ahead(view_pos(q, dq));
            let w = select(0.0, 1.0, abs(zq - z0) < post.params.x * 0.5);
            sum = sum + textureLoad(ao_tex, q, 0).r * w;
            weight = weight + w;
        }
    }
    return select(1.0, sum / weight, weight > 0.0);
}

fn luma(c: vec4<f32>) -> f32 {
    // Judge edges as if over a mid-grey background, since the real one is
    // drawn underneath later.
    let over = c.rgb + vec3<f32>(0.3) * (1.0 - c.a);
    return dot(over, vec3<f32>(0.299, 0.587, 0.114));
}

// A compact FXAA (after Timothy Lottes' FXAA, PC "fast" variant).
fn fxaa(uv: vec2<f32>) -> vec4<f32> {
    let px = post.size.zw;
    let c_m = textureSampleLevel(color_tex, lin, uv, 0.0);
    let c_nw = textureSampleLevel(color_tex, lin, uv + vec2<f32>(-1.0, -1.0) * px, 0.0);
    let c_ne = textureSampleLevel(color_tex, lin, uv + vec2<f32>(1.0, -1.0) * px, 0.0);
    let c_sw = textureSampleLevel(color_tex, lin, uv + vec2<f32>(-1.0, 1.0) * px, 0.0);
    let c_se = textureSampleLevel(color_tex, lin, uv + vec2<f32>(1.0, 1.0) * px, 0.0);
    let l_m = luma(c_m);
    let l_nw = luma(c_nw);
    let l_ne = luma(c_ne);
    let l_sw = luma(c_sw);
    let l_se = luma(c_se);
    let l_min = min(l_m, min(min(l_nw, l_ne), min(l_sw, l_se)));
    let l_max = max(l_m, max(max(l_nw, l_ne), max(l_sw, l_se)));
    if (l_max - l_min < max(0.0312, l_max * 0.125)) {
        return c_m;
    }
    var dir = vec2<f32>(-((l_nw + l_ne) - (l_sw + l_se)), (l_nw + l_sw) - (l_ne + l_se));
    let reduce = max((l_nw + l_ne + l_sw + l_se) * 0.25 * 0.125, 1.0 / 128.0);
    let rcp = 1.0 / (min(abs(dir.x), abs(dir.y)) + reduce);
    dir = clamp(dir * rcp, vec2<f32>(-8.0), vec2<f32>(8.0)) * px;
    let a = 0.5 * (textureSampleLevel(color_tex, lin, uv + dir * (1.0 / 3.0 - 0.5), 0.0)
        + textureSampleLevel(color_tex, lin, uv + dir * (2.0 / 3.0 - 0.5), 0.0));
    let b = a * 0.5 + 0.25 * (textureSampleLevel(color_tex, lin, uv + dir * -0.5, 0.0)
        + textureSampleLevel(color_tex, lin, uv + dir * 0.5, 0.0));
    let l_b = luma(b);
    if (l_b < l_min || l_b > l_max) {
        return a;
    }
    return b;
}

@fragment
fn fs_composite(in: FsIn) -> @location(0) vec4<f32> {
    let px = vec2<i32>(in.uv * vec2<f32>(texel_count()));
    let post_on = post.params.w > 0.5;
    var c: vec4<f32>;
    if (post_on) {
        c = fxaa(in.uv);
    } else {
        c = textureLoad(color_tex, clamp(px, vec2<i32>(0), texel_count() - vec2<i32>(1)), 0);
    }
    if (post.params.z > 0.5) {
        // Darken the scene and, where the floor is, the background under it:
        // "over" with a darkening of ao, as for the vignette below.
        let ao = blurred_ao(px);
        c = vec4<f32>(c.rgb * ao, 1.0 - (1.0 - c.a) * ao);
    }
    if (post.misc.z > 0.5) {
        let r = ssr(px, in.pos.xy);
        if (r.a > 0.0) {
            if (c.a > 0.5) {
                // On objects: mix the reflection over the shaded surface.
                c = vec4<f32>(mix(c.rgb, r.rgb * c.a, r.a), c.a);
            } else {
                // On the floor: a faint mirror image over the background.
                let k = r.a * 0.45;
                c = vec4<f32>(c.rgb + r.rgb * k, c.a + (1.0 - c.a) * k);
            }
        }
    }
    if (!post_on) {
        return c;
    }

    // Gentle S-curve and a little extra saturation on the scene itself.
    if (c.a > 0.0) {
        var rgb = c.rgb / c.a;
        rgb = mix(rgb, rgb * rgb * (3.0 - 2.0 * rgb), 0.3);
        let grey = dot(rgb, vec3<f32>(0.299, 0.587, 0.114));
        rgb = clamp(mix(vec3<f32>(grey), rgb, 1.12), vec3<f32>(0.0), vec3<f32>(1.0));
        c = vec4<f32>(rgb * c.a, c.a);
    }
    // Vignette over scene and background alike: darken by v everywhere,
    // including the gradient drawn underneath (premultiplied "over").
    let q = in.uv - vec2<f32>(0.5);
    let aspect = post.size.x * post.size.w;
    let v = 1.0 - 0.32 * smoothstep(0.25, 0.85, length(q * vec2<f32>(aspect, 1.0)) / max(aspect, 1.0) * 1.4);
    return vec4<f32>(c.rgb * v, 1.0 - (1.0 - c.a) * v);
}
