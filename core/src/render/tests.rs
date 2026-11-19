use super::*;
use crate::camera::{Camera, ViewKind};
use crate::editor::Editor;
use crate::light::LightKind;
use crate::primitive::PrimitiveKind;
use crate::scene::{ObjectKind, Scene};

fn small(engine: Engine) -> RenderSettings {
    RenderSettings { engine, width: 48, height: 32, samples: 3, ..RenderSettings::default() }
}

fn view(ed: &mut Editor) -> Camera {
    ed.frame_all(ViewKind::Perspective);
    ed.cameras[ViewKind::Perspective.index()]
}

fn render(ed: &mut Editor, s: &RenderSettings) -> FrameImages {
    let cam = view(ed);
    let pose = crate::scene_camera::CameraPose::from_editor(&cam, s.width as f32 / s.height as f32);
    let mut job = FrameJob::new(ed, s, 0, (s.width, s.height), &pose, None);
    let mut guard = 0;
    while !job.step(7) {
        guard += 1;
        assert!(guard < 10_000);
    }
    assert!((job.progress() - 1.0).abs() < 1e-6);
    job.images()
}

fn centre(img: &Image) -> [u8; 4] {
    img.get(img.width / 2, img.height / 2)
}

#[test]
fn renders_a_cube_over_the_background() {
    let mut ed = Editor::new(Scene::new());
    ed.scene.add_primitive(PrimitiveKind::Cube, None);
    for engine in [Engine::Standard, Engine::Physical] {
        let s = RenderSettings { passes: vec![Pass::Depth, Pass::Normal, Pass::ObjectId, Pass::Alpha, Pass::AmbientOcclusion, Pass::Shadow], ..small(engine) };
        let out = render(&mut ed, &s);
        assert_eq!(out.passes.len(), 7);
        let beauty = &out.passes[0].1;
        assert_eq!((beauty.width, beauty.height), (48, 32));
        let c = centre(beauty);
        assert_eq!(c[3], 255, "the cube covers the centre");
        assert!(c[0] as u32 + c[1] as u32 + c[2] as u32 > 30, "lit: {c:?} ({engine:?})");
        assert_eq!(beauty.get(0, 0)[3], 0, "the corner is background");
        let depth = &out.passes.iter().find(|p| p.0 == Pass::Depth).unwrap().1;
        assert!(centre(depth)[0] > 0);
        let png = encode(beauty, ImageFormat::Png, 90, None).unwrap();
        assert_eq!(&png[1..4], b"PNG");
        let jpg = encode(beauty, ImageFormat::Jpeg, 80, None).unwrap();
        assert_eq!(&jpg[0..2], &[0xFF, 0xD8]);
    }
}

#[test]
fn hidden_from_render_and_light_linking() {
    let mut ed = Editor::new(Scene::new());
    let cube = ed.scene.add_primitive(PrimitiveKind::Cube, None);
    let s = small(Engine::Standard);
    let lit = centre(&render(&mut ed, &s).passes[0].1);
    // A light that excludes the cube leaves only its ambient.
    let light = ed.scene.add_object("Light", ObjectKind::Light(LightKind::Point.default_light()), None);
    ed.scene.get_mut(light).unwrap().pos = glam::Vec3::new(0.0, 300.0, -400.0);
    if let Some(ObjectKind::Light(l)) = ed.scene.get_mut(light).map(|o| &mut o.kind) {
        l.link_mode = crate::light::LinkMode::Exclude;
        l.link_objects = vec![cube];
    }
    let excluded = centre(&render(&mut ed, &s).passes[0].1);
    assert!(excluded[0] < lit[0], "{excluded:?} vs {lit:?}");
    ed.scene.get_mut(cube).unwrap().vis_render = crate::scene::Visibility::Off;
    assert_eq!(centre(&render(&mut ed, &s).passes[0].1)[3], 0, "hidden in renders");
}

#[test]
fn shadows_darken_the_floor_and_sketch_draws_lines() {
    let mut ed = Editor::new(Scene::new());
    let floor = ed.scene.add_primitive(PrimitiveKind::Plane, None);
    let _ = floor;
    let cube = ed.scene.add_primitive(PrimitiveKind::Cube, None);
    ed.scene.get_mut(cube).unwrap().pos = glam::Vec3::new(0.0, 120.0, 0.0);
    let sun = ed.scene.add_object("Light", ObjectKind::Light(LightKind::Infinite.default_light()), None);
    ed.scene.get_mut(sun).unwrap().rot = crate::light::aim_hpb(glam::Vec3::new(0.3, -1.0, 0.2));
    let s = RenderSettings { passes: vec![Pass::Shadow], ..small(Engine::Standard) };
    let out = render(&mut ed, &s);
    let shadow = &out.passes[1].1;
    assert!(shadow.pixels.chunks_exact(4).any(|p| p[0] > 128), "some of the floor is in shadow");
    let s = RenderSettings { sketch: SketchMode::Contour, line_color: [1.0, 0.0, 0.0], ..small(Engine::Standard) };
    let out = render(&mut ed, &s);
    assert!(out.passes[0].1.pixels.chunks_exact(4).any(|p| p[0] > 200 && p[1] < 60 && p[2] < 60), "red outlines");
}

#[test]
fn render_camera_sky_and_stereo() {
    use crate::scene_camera::{CameraKind, StereoMode};
    let mut ed = Editor::new(Scene::new());
    ed.scene.add_primitive(PrimitiveKind::Cube, None);
    view(&mut ed);
    let cam = ed.create_camera(CameraKind::Stereo);
    ed.scene.render_camera = Some(cam);
    if let Some(ObjectKind::Camera(c)) = ed.scene.get_mut(cam).map(|o| &mut o.kind) {
        c.stereo_mode = StereoMode::SideBySide;
    }
    ed.create_sky(crate::sky::SkyKind::Gradient);
    let s = small(Engine::Standard);
    let out = render(&mut ed, &s);
    assert_eq!(out.width, 96, "two eyes side by side");
    let corner = out.passes[0].1.get(0, 0);
    assert!(corner[2] > corner[0], "blue sky behind: {corner:?}");
}

#[test]
fn tokens_and_frames() {
    let ctx = TokenContext { project: "My Scene.forma3d".into(), camera: "Cam/1".into(), frame: 7, width: 640, height: 480, ..Default::default() };
    assert_eq!(resolve_tokens("out/$prj_$camera_$frame_$res", &ctx), "out/My Scene_Cam_1_0007_640x480");
    let s = RenderSettings { frame_range: FrameRange::Manual, frame_from: 10, frame_to: 0, frame_step: 5, ..RenderSettings::default() };
    assert_eq!(s.frames(3, (0, 90)), vec![0, 5, 10]);
    assert_eq!(RenderSettings::default().frames(3, (0, 90)), vec![3]);
    assert!(view_transform(glam::Vec3::splat(4.0), ViewTransform::Standard).min_element() > 0.999);
    assert!(view_transform(glam::Vec3::splat(4.0), ViewTransform::AcesSdr).x < 1.0);
    let back: RenderSettings = serde_json::from_str(r#"{"samples":4}"#).unwrap();
    assert_eq!(back.samples, 4);
    assert_eq!(back.width, 1280);
}

#[test]
fn requests_name_files_and_dates() {
    let mut ed = Editor::new(Scene::new());
    ed.scene.add_primitive(PrimitiveKind::Cube, None);
    let cam = view(&mut ed);
    let req = RenderRequest {
        scene: ed.scene.clone(),
        fps: 30.0,
        frames: vec![2],
        settings: RenderSettings { save_passes: true, passes: vec![Pass::Depth], format: ImageFormat::Png16, ..small(Engine::Standard) },
        fallback: crate::scene_camera::CameraPose::from_editor(&cam, 1.5),
        region: Some([10, 5, 30, 20]),
        project: "Scene".into(),
        take: "Main".into(),
    };
    let json = serde_json::to_string(&req).unwrap();
    let back: RenderRequest = serde_json::from_str(&json).unwrap();
    let mut job = back.job(2);
    while !job.pass() {}
    let images = job.images();
    let files = back.files(&images, "20261116", "120000");
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].0, "Scene_Editor Camera_0002.png");
    assert_eq!(files[1].0, "Scene_Editor Camera_0002_depth.png");
    assert!(files.iter().all(|f| f.1.is_ok()));
    // Outside the region nothing is rendered.
    assert_eq!(images.passes[0].1.get(0, 0)[3], 0);
    assert_eq!(date_time(0), ("19700101".to_owned(), "000000".to_owned()));
    assert_eq!(date_time(1_794_830_400).0, "20261116");
}

#[test]
fn materials_glow_let_light_through_and_texture() {
    let mut ed = Editor::new(Scene::new());
    let cube = ed.scene.add_primitive(PrimitiveKind::Cube, None);
    let m = ed.scene.add_material("m", [0.2, 0.2, 0.2]);
    ed.scene.get_mut(cube).unwrap().material = Some(m);
    let s = RenderSettings { background: [0.0, 0.0, 1.0], alpha: false, ..small(Engine::Standard) };
    let plain = centre(&render(&mut ed, &s).passes[0].1);
    // Glowing: brighter.
    ed.scene.material_mut(m).unwrap().emission = [1.0, 0.0, 0.0];
    ed.scene.material_mut(m).unwrap().emission_strength = 2.0;
    let glow = centre(&render(&mut ed, &s).passes[0].1);
    assert!(glow[0] > plain[0] + 80, "{glow:?} vs {plain:?}");
    // See-through: the blue background shows through a clear, colourless cube.
    let mat = ed.scene.material_mut(m).unwrap();
    mat.emission_strength = 0.0;
    mat.transparency = 1.0;
    mat.ior = 1.0;
    mat.color = [1.0, 1.0, 1.0];
    let clear = centre(&render(&mut ed, &s).passes[0].1);
    assert!(clear[2] > clear[0] + 60, "blue shows through: {clear:?}");
    // A red picture on a white material.
    let mat = ed.scene.material_mut(m).unwrap();
    mat.transparency = 0.0;
    let red = encode_png8(&Image { width: 2, height: 2, pixels: [255u8, 0, 0, 255].repeat(4) });
    mat.texture = Some(crate::texture::Texture::from_file("red.png", &red).unwrap());
    let tex = centre(&render(&mut ed, &s).passes[0].1);
    assert!(tex[0] > tex[1] + 40 && tex[0] > tex[2] + 40, "textured red: {tex:?}");
    // Old scene files without the new fields still load.
    let old: crate::scene::Material = serde_json::from_str(r#"{"id":1,"name":"a","color":[1,0,0]}"#).unwrap();
    assert_eq!(old.roughness, 0.5);
    assert!(old.texture.is_none());
}

#[test]
fn every_filter_peaks_at_the_sample_and_ends_at_its_width() {
    for f in Filter::ALL {
        let w = f.default_width();
        let peak = f.weight(0.0, w);
        assert!(peak > 0.8 && (1..40).all(|k| f.weight(k as f32 * 0.07, w) <= peak + 1e-5), "{f:?} peaks at the centre");
        assert_eq!(f.weight(w * 0.5 + 0.01, w), 0.0, "{f:?} past its edge");
        // Over a pixel grid the weights add up to something positive.
        let sum: f32 = (-8..=8).map(|k| f.weight(k as f32 + 0.25, w)).sum();
        assert!(sum > 0.5, "{f:?} sums to {sum}");
    }
    // Cube keeps each sample inside its own pixel.
    assert_eq!(Filter::Box.weight(0.49, 1.0), 1.0);
    assert_eq!(Filter::Box.weight(0.51, 1.0), 0.0);
    // The sharpening filters have negative lobes; Gauss doesn't.
    assert!(Filter::Mitchell.weight(1.5, 4.0) < 0.0);
    assert!(Filter::Sinc.weight(1.3, 6.0) < 0.0);
    assert!((0..60).all(|k| Filter::Gaussian.weight(k as f32 * 0.025, 3.0) >= 0.0));
}

#[test]
fn sampling_plans_follow_the_engine_and_its_settings() {
    let std = |aa| RenderSettings { engine: Engine::Standard, antialiasing: aa, samples: 9, ..RenderSettings::default() }.plan();
    assert_eq!((std(AntiAliasing::None).min, std(AntiAliasing::None).max), (1, 1));
    assert_eq!(std(AntiAliasing::Geometry).max, 9);
    let best = RenderSettings { antialiasing: AntiAliasing::Best, aa_min_level: AaLevel::L2x2, aa_max_level: AaLevel::L8x8, ..RenderSettings::default() }.plan();
    assert_eq!((best.min, best.max), (4, 64));
    assert!(best.threshold.is_some());
    // A max below the min is raised to it.
    let odd = RenderSettings { antialiasing: AntiAliasing::Best, aa_min_level: AaLevel::L8x8, aa_max_level: AaLevel::L2x2, ..RenderSettings::default() }.plan();
    assert_eq!((odd.min, odd.max), (64, 64));

    let phys = |sampler, q| RenderSettings { engine: Engine::Physical, sampler, sampling_quality: q, sampling_subdivisions: 3, ..RenderSettings::default() };
    assert_eq!(phys(Sampler::Fixed, SamplingQuality::Custom).plan().max, 64);
    let adaptive = phys(Sampler::Adaptive, SamplingQuality::Custom).plan();
    assert_eq!((adaptive.min, adaptive.max), (16, 64));
    // Presets decide the subdivisions unless Custom.
    assert_eq!(phys(Sampler::Fixed, SamplingQuality::Low).plan().max, 4);
    assert_eq!(phys(Sampler::Fixed, SamplingQuality::High).plan().max, 64);
    let high = phys(Sampler::Fixed, SamplingQuality::High).with_quality();
    assert!(high.shadow_subdivisions > phys(Sampler::Fixed, SamplingQuality::Low).with_quality().shadow_subdivisions);
    let prog = |mode| RenderSettings { engine: Engine::Physical, sampler: Sampler::Progressive, progressive_mode: mode, progressive_passes: 12, time_limit: 5.0, ..RenderSettings::default() }.plan();
    assert_eq!(prog(ProgressiveMode::PassCount).max, 12);
    assert_eq!(prog(ProgressiveMode::TimeLimit).time_limit_ms, Some(5000.0));
    assert!(prog(ProgressiveMode::Infinite).open_ended);
}

#[test]
fn best_anti_aliasing_stops_early_where_the_picture_is_clean() {
    let mut ed = Editor::new(Scene::new());
    ed.scene.add_primitive(PrimitiveKind::Cube, None);
    let s = RenderSettings { antialiasing: AntiAliasing::Best, aa_min_level: AaLevel::L2x2, aa_max_level: AaLevel::L16x16, aa_threshold: 5.0, ..small(Engine::Standard) };
    let cam = view(&mut ed);
    let pose = crate::scene_camera::CameraPose::from_editor(&cam, 1.5);
    let mut job = FrameJob::new(&mut ed, &s, 0, (s.width, s.height), &pose, None);
    while !job.pass() {}
    // A plain, evenly lit cube converges long before 256 samples.
    assert!(job.passes_done() < 256, "took {} passes", job.passes_done());
    let b = &job.eyes[0].buffers;
    // Background pixels stop after the minimum; edges get more.
    let corner = b.count[0];
    assert_eq!(corner, 4, "the empty corner stops at Min Level");
    assert!(b.count.iter().max().copied().unwrap_or(0) > 4, "edges are refined");
    let img = job.images();
    assert_eq!(centre(&img.passes[0].1)[3], 255);
}

#[test]
fn a_progressive_time_limit_ends_the_render() {
    let mut ed = Editor::new(Scene::new());
    ed.scene.add_primitive(PrimitiveKind::Cube, None);
    let s = RenderSettings { sampler: Sampler::Progressive, progressive_mode: ProgressiveMode::TimeLimit, time_limit: 2.0, ..small(Engine::Physical) };
    let cam = view(&mut ed);
    let pose = crate::scene_camera::CameraPose::from_editor(&cam, 1.5);
    let mut job = FrameJob::new(&mut ed, &s, 0, (s.width, s.height), &pose, None);
    assert!(!job.pass());
    job.set_elapsed_ms(1000.0);
    assert!(!job.done());
    assert!((job.progress() - 0.5).abs() < 0.01);
    job.set_elapsed_ms(2500.0);
    assert!(job.done());
    let inf = RenderSettings { progressive_mode: ProgressiveMode::Infinite, ..s };
    let mut job = FrameJob::new(&mut ed, &inf, 0, (inf.width, inf.height), &pose, None);
    for _ in 0..5 {
        job.pass();
    }
    assert!(!job.done() && job.open_ended());
}

#[test]
fn options_switch_parts_of_the_render_off() {
    let mut ed = Editor::new(Scene::new());
    ed.scene.add_primitive(PrimitiveKind::Cube, None);
    let lum = |img: &Image| {
        let c = centre(img);
        c[0] as u32 + c[1] as u32 + c[2] as u32
    };
    let base = small(Engine::Standard);
    let lit = lum(&render(&mut ed, &base).passes[0].1);
    // No lights and no default light: only the background (black) is left.
    let dark = lum(&render(&mut ed, &RenderSettings { default_light: false, ..base.clone() }).passes[0].1);
    assert!(dark < lit / 4, "{dark} vs {lit}");
    // Global Brightness scales the light.
    let dim = lum(&render(&mut ed, &RenderSettings { global_brightness: 30.0, ..base.clone() }).passes[0].1);
    assert!(dim < lit, "{dim} vs {lit}");
    // Glow brightens around bright pixels; Vignette darkens the corners.
    let bright = RenderSettings { exposure: 3.0, ..base.clone() };
    let plain = render(&mut ed, &bright).passes[0].1.clone();
    let glow = render(&mut ed, &RenderSettings { glow: true, glow_threshold: 0.5, glow_strength: 200.0, ..bright.clone() }).passes[0].1.clone();
    let sum = |img: &Image| img.pixels.chunks(4).map(|p| p[0] as u32).sum::<u32>();
    assert!(sum(&glow) > sum(&plain));
    let vig = render(&mut ed, &RenderSettings { vignette: 100.0, background: [0.5; 3], ..base.clone() }).passes[0].1.clone();
    let flat = render(&mut ed, &RenderSettings { background: [0.5; 3], ..base.clone() }).passes[0].1.clone();
    assert!(vig.get(0, 0)[0] < flat.get(0, 0)[0], "corner darker");
    // Post Effects off turns Glow back off.
    let off = render(&mut ed, &RenderSettings { glow: true, glow_threshold: 0.5, glow_strength: 200.0, post_effects: false, ..bright }).passes[0].1.clone();
    assert_eq!(sum(&off), sum(&plain));
}

#[test]
fn wide_filters_soften_and_custom_sizes_apply() {
    let mut ed = Editor::new(Scene::new());
    ed.scene.add_primitive(PrimitiveKind::Cube, None);
    let edge_softness = |s: &RenderSettings, ed: &mut Editor| {
        let img = render(ed, s).passes[0].1.clone();
        // Count partly covered pixels along the middle row.
        let y = img.height / 2;
        (0..img.width).filter(|&x| (20..235).contains(&img.get(x, y)[3])).count()
    };
    let cube = RenderSettings { filter: Filter::Box, samples: 8, ..small(Engine::Standard) };
    let wide = RenderSettings { filter: Filter::Gaussian, custom_filter_size: true, filter_width: 6.0, filter_height: 6.0, ..cube.clone() };
    assert!(edge_softness(&wide, &mut ed) > edge_softness(&cube, &mut ed));
    assert_eq!(RenderSettings { filter: Filter::Sinc, ..RenderSettings::default() }.filter_size(), (6.0, 6.0));
}
