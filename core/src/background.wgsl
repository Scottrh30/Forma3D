// A view panel's background (reference) image, drawn first, behind the scene.

struct Bg {
    // Clip-space rectangle: x0, y0, x1, y1 (y up)
    rect: vec4<f32>,
    // x: opacity
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> bg: Bg;
@group(0) @binding(1) var bg_tex: texture_2d<f32>;
@group(0) @binding(2) var bg_sampler: sampler;

struct BgOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_background(@builtin(vertex_index) i: u32) -> BgOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );
    let c = corners[i];
    var out: BgOut;
    out.pos = vec4<f32>(mix(bg.rect.x, bg.rect.z, c.x), mix(bg.rect.w, bg.rect.y, c.y), 1.0, 1.0);
    out.uv = c;
    return out;
}

@fragment
fn fs_background(in: BgOut) -> @location(0) vec4<f32> {
    let c = textureSampleLevel(bg_tex, bg_sampler, in.uv, 0.0);
    return vec4<f32>(c.rgb, c.a * bg.params.x);
}
