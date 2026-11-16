//! The Attribute Manager: properties of the active object.

use super::widgets::{attr_label, header_bar, heading, menu_check, number};
use crate::editor::{AttrTab, Editor};
use crate::icons;
use crate::csg::BoolMode;
use crate::deformer::Deformer;
use crate::generator::{ArrayMode, Generator, MAX_COPIES, MAX_LEVELS, MirrorPlane};
use crate::primitive::Primitive;
use crate::scene::{ObjectKind, Visibility};
use crate::theme;
use eframe::egui::{self, Ui, vec2};

pub fn show(ui: &mut Ui, ed: &mut Editor) {
    header_bar(ui, |ui| {
        ui.menu_button("Mode", |ui| {
            menu_check(ui, true, "Object", "");
        });
        ui.menu_button("Edit", |ui| {
            super::commands::menu_item(ui, ed, super::commands::Cmd::ResetTransform);
            super::commands::menu_item(ui, ed, super::commands::Cmd::MakeEditable);
        });
    });
    let Some(id) = ed.active() else {
        ui.add_space(10.0);
        ui.vertical_centered(|ui| {
            ui.colored_label(theme::TEXT_DIM, "No object selected");
        });
        return;
    };
    let Some(obj) = ed.scene.get(id) else { return };
    let icon = icons::for_kind(&obj.kind);
    let header = format!("{} Object [{}]", obj.kind.type_name(), obj.name);

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        let (rect, _) = ui.allocate_exact_size(vec2(18.0, 18.0), egui::Sense::hover());
        icons::paint(ui.painter(), rect, icon, theme::TEXT);
        ui.label(egui::RichText::new(header).size(13.5).strong());
        if ed.selection.len() > 1 {
            ui.colored_label(theme::TEXT_DIM, format!("(+{} selected)", ed.selection.len() - 1));
        }
    });
    ui.add_space(3.0);
    // Tabs: a row of flat buttons, the current one lit.
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        ui.spacing_mut().item_spacing.x = 2.0;
        for (tab, name) in [(AttrTab::Basic, "Basic"), (AttrTab::Coord, "Coord."), (AttrTab::Object, "Object")] {
            if attr_tab(ui, name, ed.attr_tab == tab).clicked() {
                ed.attr_tab = tab;
            }
        }
    });
    ui.add_space(3.0);

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 3.0;
        egui::Frame::new().inner_margin(egui::Margin { left: 0, right: 6, top: 0, bottom: 6 }).show(ui, |ui| match ed.attr_tab {
            AttrTab::Basic => basic(ui, ed, id),
            AttrTab::Coord => coord(ui, ed, id),
            AttrTab::Object => object(ui, ed, id),
        });
    });
}

fn attr_tab(ui: &mut Ui, label: &str, selected: bool) -> egui::Response {
    let font = egui::FontId::proportional(12.5);
    let w = ui.fonts_mut(|f| f.layout_no_wrap(label.to_owned(), font.clone(), theme::TEXT).size().x) + 18.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 21.0), egui::Sense::click());
    let fill = if selected {
        theme::BG_HOVER
    } else if resp.hovered() {
        theme::BG_HEADER.gamma_multiply(1.3)
    } else {
        theme::BG_HEADER
    };
    ui.painter().rect_filled(rect, 3.0, fill);
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, font, if selected { egui::Color32::WHITE } else { theme::TEXT_DIM });
    resp
}

fn vis_combo(ui: &mut Ui, label: &str, value: &mut Visibility) {
    attr_label(ui, label);
    egui::ComboBox::from_id_salt(label)
        .selected_text(match value {
            Visibility::Default => "Default",
            Visibility::On => "On",
            Visibility::Off => "Off",
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(value, Visibility::Default, "Default");
            ui.selectable_value(value, Visibility::On, "On");
            ui.selectable_value(value, Visibility::Off, "Off");
        });
    ui.end_row();
}

fn basic(ui: &mut Ui, ed: &mut Editor, id: u64) {
    heading(ui, "Basic Properties");
    let materials: Vec<(u64, String)> = ed.scene.materials().iter().map(|m| (m.id, m.name.clone())).collect();
    let layers: Vec<(u64, String)> = ed.scene.layers().iter().map(|l| (l.id, l.name.clone())).collect();
    let Some(o) = ed.scene.get_mut(id) else { return };
    egui::Grid::new("basic").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
        attr_label(ui, "Name");
        let mut name = o.name.clone();
        if ui.text_edit_singleline(&mut name).changed() && !name.trim().is_empty() {
            o.name = name;
        }
        ui.end_row();
        vis_combo(ui, "Visible in Editor", &mut o.vis_editor);
        vis_combo(ui, "Visible in Renderer", &mut o.vis_render);
        if matches!(o.kind, ObjectKind::Generator(_) | ObjectKind::Deformer(_)) {
            attr_label(ui, "Enabled");
            ui.checkbox(&mut o.enabled, "");
            ui.end_row();
        }
        attr_label(ui, "Material");
        let current = o
            .material
            .and_then(|m| materials.iter().find(|(i, _)| *i == m))
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| "None".to_owned());
        egui::ComboBox::from_id_salt("material").selected_text(current).show_ui(ui, |ui| {
            ui.selectable_value(&mut o.material, None, "None");
            for (mid, name) in &materials {
                ui.selectable_value(&mut o.material, Some(*mid), name);
            }
        });
        ui.end_row();
        attr_label(ui, "Layer");
        let current = o.layer.and_then(|l| layers.iter().find(|(i, _)| *i == l)).map(|(_, n)| n.clone()).unwrap_or_else(|| "None".to_owned());
        egui::ComboBox::from_id_salt("layer").selected_text(current).show_ui(ui, |ui| {
            ui.selectable_value(&mut o.layer, None, "None");
            for (lid, name) in &layers {
                ui.selectable_value(&mut o.layer, Some(*lid), name);
            }
        });
        ui.end_row();
    });
    ui.add_space(6.0);
    hint(ui, "Render visibility is stored but there is no renderer yet.");
}

/// Cinema 4D's Coord. tab: three columns P / S / R with X Y Z (H P B) rows.
fn coord(ui: &mut Ui, ed: &mut Editor, id: u64) {
    heading(ui, "Coordinates");
    let Some(o) = ed.scene.get_mut(id) else { return };
    egui::Grid::new("coord").num_columns(6).spacing([4.0, 4.0]).show(ui, |ui| {
        let rows = [("X", "X", "H"), ("Y", "Y", "P"), ("Z", "Z", "B")];
        for (i, (p, sc, r)) in rows.into_iter().enumerate() {
            short_label(ui, &format!("P . {p}"));
            number(ui, &mut o.pos[i], 1.0, " cm");
            short_label(ui, &format!("S . {sc}"));
            number(ui, &mut o.scale[i], 0.01, "");
            short_label(ui, &format!("R . {r}"));
            number(ui, &mut o.rot[i], 0.5, "°");
            ui.end_row();
        }
    });
    heading(ui, "Transform");
    let reset = ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.button("Reset Transform").clicked()
    });
    if reset.inner {
        o.pos = glam::Vec3::ZERO;
        o.rot = glam::Vec3::ZERO;
        o.scale = glam::Vec3::ONE;
        ed.label("Reset Transform");
    }
}

fn short_label(ui: &mut Ui, text: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(38.0, 20.0), egui::Sense::hover());
    ui.painter().text(
        rect.right_center() - vec2(4.0, 0.0),
        egui::Align2::RIGHT_CENTER,
        text,
        egui::FontId::proportional(12.5),
        theme::TEXT,
    );
}

fn dv(ui: &mut Ui, label: &str, v: &mut f32) {
    attr_label(ui, label);
    number(ui, v, 1.0, " cm");
    ui.end_row();
}

fn seg(ui: &mut Ui, label: &str, v: &mut u32) {
    attr_label(ui, label);
    ui.add(egui::DragValue::new(v).range(1..=256));
    ui.end_row();
}

fn object(ui: &mut Ui, ed: &mut Editor, id: u64) {
    let Some(o) = ed.scene.get_mut(id) else { return };
    match &mut o.kind {
        ObjectKind::Null => {
            heading(ui, "Null Object");
            hint(ui, "A Null has no parameters. Use it to group objects.");
        }
        ObjectKind::Camera(c) => {
            heading(ui, c.kind.name());
            hint(ui, "Cameras are set up in the Forma3D app.");
        }
        ObjectKind::Sky(s) => {
            heading(ui, s.kind.name());
            hint(ui, "Skies are set up in the Forma3D app.");
        }
        ObjectKind::Light(l) => {
            use crate::light::LightKind as K;
            heading(ui, l.kind.name());
            egui::Grid::new("light").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                attr_label(ui, "Color");
                ui.color_edit_button_rgb(&mut l.color);
                ui.end_row();
                attr_label(ui, "Intensity");
                ui.add(egui::DragValue::new(&mut l.intensity).range(0.0..=10_000.0).speed(1.0).suffix(" %"));
                ui.end_row();
                if !matches!(l.kind, K::Dome) {
                    attr_label(ui, "Shadows");
                    ui.checkbox(&mut l.shadows, "");
                    ui.end_row();
                }
                if matches!(l.kind, K::Spot) {
                    attr_label(ui, "Inner Angle");
                    number(ui, &mut l.inner_angle, 0.5, "°");
                    ui.end_row();
                    attr_label(ui, "Outer Angle");
                    number(ui, &mut l.outer_angle, 0.5, "°");
                    ui.end_row();
                }
                if matches!(l.kind, K::Area) {
                    attr_label(ui, "Size X");
                    number(ui, &mut l.size[0], 1.0, " cm");
                    ui.end_row();
                    attr_label(ui, "Size Y");
                    number(ui, &mut l.size[1], 1.0, " cm");
                    ui.end_row();
                }
                if matches!(l.kind, K::Area | K::Point | K::Spot | K::Omni) {
                    attr_label(ui, "Falloff");
                    ui.checkbox(&mut l.falloff, "");
                    ui.end_row();
                    if l.falloff {
                        attr_label(ui, "Radius");
                        number(ui, &mut l.radius, 1.0, " cm");
                        ui.end_row();
                    }
                }
            });
            ui.add_space(6.0);
            hint(
                ui,
                match l.kind {
                    K::Sun => "Rotate the Sun to set the time of day: high is white noon light, low is warm evening light. It brings a soft blue sky with it.",
                    K::Dome => "Lights the whole scene evenly from the sky. Its colour tints every shadow side.",
                    K::Infinite => "Parallel light from one direction, like sunlight: only its rotation matters.",
                    K::Parallel => "Parallel light from a plane: like Infinite, but only what's in front of it is lit.",
                    K::Area => "Shines from a rectangle towards its +Z axis, with softer shading than a point.",
                    K::Spot => "A cone of light along +Z: full inside the inner angle, fading to the outer one.",
                    K::Point => "Shines in every direction from one point and fades with distance.",
                    K::Omni => "Shines in every direction from one point, the same at any distance unless Falloff is on.",
                },
            );
            *l = l.sanitized();
        }
        ObjectKind::Spline(s) => {
            heading(ui, "Spline Object");
            egui::Grid::new("spline").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                attr_label(ui, "Type");
                egui::ComboBox::from_id_salt("spline-type").selected_text(s.kind.label()).show_ui(ui, |ui| {
                    for k in crate::spline::SplineType::ALL {
                        ui.selectable_value(&mut s.kind, k, k.label());
                    }
                });
                ui.end_row();
                attr_label(ui, "Close Spline");
                ui.checkbox(&mut s.closed, "");
                ui.end_row();
                attr_label(ui, "Intermediate Points");
                ui.add(egui::DragValue::new(&mut s.steps).range(1..=256));
                ui.end_row();
                attr_label(ui, "Points");
                ui.label(s.points.len().to_string());
                ui.end_row();
            });
            ui.add_space(6.0);
            hint(ui, "Draw and edit it with the Spline Pen: click to add points, drag to pull out curves, drag a point or handle to move it.");
            *s = s.sanitized();
        }
        ObjectKind::Primitive(p) => {
            primitive(ui, p);
            *p = p.sanitized();
        }
        ObjectKind::Generator(g) => {
            generator(ui, g);
            *g = g.sanitized();
        }
        ObjectKind::Polygon(m) => {
            heading(ui, "Polygon Object");
            egui::Grid::new("polyinfo").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                attr_label(ui, "Points");
                ui.label(m.points.len().to_string());
                ui.end_row();
                attr_label(ui, "Polygons");
                ui.label(m.polys.len().to_string());
                ui.end_row();
                attr_label(ui, "Edges");
                ui.label(m.edges().len().to_string());
                ui.end_row();
            });
            ui.add_space(6.0);
            hint(ui, "Edit it in Points, Edges or Polygons mode (left palette).");
        }
        ObjectKind::Deformer(d) => {
            let has_parent = o.parent.is_some();
            let fit = deformer(ui, d, has_parent);
            *d = d.sanitized();
            if fit {
                ed.fit_deformer(id);
            }
        }
    }
}

/// Returns true when "Fit to Parent" was clicked.
fn deformer(ui: &mut Ui, d: &mut Deformer, has_parent: bool) -> bool {
    heading(ui, &format!("{} Object", d.kind().name()));
    egui::Grid::new("deformer").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
        let mut size = d.size();
        vec3_row(ui, "Size", &mut size, 1.0, " cm");
        d.set_size(size);
        let mode = |ui: &mut Ui, limited: &mut bool| {
            attr_label(ui, "Mode");
            egui::ComboBox::from_id_salt("deform-mode")
                .selected_text(if *limited { "Limited" } else { "Unlimited" })
                .show_ui(ui, |ui| {
                    ui.selectable_value(limited, true, "Limited");
                    ui.selectable_value(limited, false, "Unlimited");
                });
            ui.end_row();
        };
        match d {
            Deformer::Bend { strength, direction, limited, .. } => {
                mode(ui, limited);
                attr_label(ui, "Strength");
                number(ui, strength, 0.5, "°");
                ui.end_row();
                attr_label(ui, "Angle");
                number(ui, direction, 0.5, "°");
                ui.end_row();
            }
            Deformer::Twist { angle, limited, .. } => {
                mode(ui, limited);
                attr_label(ui, "Angle");
                number(ui, angle, 0.5, "°");
                ui.end_row();
            }
            Deformer::Taper { strength, curvature, limited, .. } => {
                mode(ui, limited);
                attr_label(ui, "Strength");
                number(ui, strength, 0.5, " %");
                ui.end_row();
                attr_label(ui, "Curvature");
                number(ui, curvature, 0.5, " %");
                *curvature = curvature.clamp(0.0, 100.0);
                ui.end_row();
            }
            Deformer::Shear { strength, direction, curvature, limited, .. } => {
                mode(ui, limited);
                attr_label(ui, "Strength");
                number(ui, strength, 0.5, " %");
                ui.end_row();
                attr_label(ui, "Angle");
                number(ui, direction, 0.5, "°");
                ui.end_row();
                attr_label(ui, "Curvature");
                number(ui, curvature, 0.5, " %");
                *curvature = curvature.clamp(0.0, 100.0);
                ui.end_row();
            }
        }
    });
    ui.add_space(6.0);
    let fit = ui
        .add_enabled(has_parent, egui::Button::new("Fit to Parent"))
        .on_hover_text("Size and place the box around the object being deformed")
        .clicked();
    hint(ui, "Deforms its parent object (for a Null or generator: everything inside).");
    hint(ui, "Smooth results need segments: raise them on the object's Object tab.");
    fit
}

fn vec3_row(ui: &mut Ui, label: &str, v: &mut glam::Vec3, speed: f64, suffix: &str) {
    attr_label(ui, label);
    ui.horizontal(|ui| {
        number(ui, &mut v.x, speed, suffix);
        number(ui, &mut v.y, speed, suffix);
        number(ui, &mut v.z, speed, suffix);
    });
    ui.end_row();
}

fn generator(ui: &mut Ui, g: &mut Generator) {
    heading(ui, &format!("{} Object", g.kind().name()));
    egui::Grid::new("generator").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| match g {
        Generator::Subdivision { levels } => {
            attr_label(ui, "Subdivision");
            ui.add(egui::DragValue::new(levels).range(0..=MAX_LEVELS));
            ui.end_row();
        }
        Generator::Array { mode, count, offset, rotation, scale, radius } => {
            attr_label(ui, "Mode");
            egui::ComboBox::from_id_salt("array-mode")
                .selected_text(match mode {
                    ArrayMode::Linear => "Linear",
                    ArrayMode::Radial => "Radial",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(mode, ArrayMode::Linear, "Linear");
                    ui.selectable_value(mode, ArrayMode::Radial, "Radial");
                });
            ui.end_row();
            attr_label(ui, "Copies");
            ui.add(egui::DragValue::new(count).range(1..=MAX_COPIES));
            ui.end_row();
            match mode {
                ArrayMode::Linear => {
                    vec3_row(ui, "Offset", offset, 1.0, " cm");
                    vec3_row(ui, "Rotation (HPB)", rotation, 0.5, "°");
                    vec3_row(ui, "Scale per copy", scale, 0.01, "");
                }
                ArrayMode::Radial => {
                    attr_label(ui, "Radius");
                    number(ui, radius, 1.0, " cm");
                    ui.end_row();
                }
            }
        }
        Generator::Boolean { mode } => {
            attr_label(ui, "Mode");
            egui::ComboBox::from_id_salt("bool-mode").selected_text(mode.label()).show_ui(ui, |ui| {
                for m in BoolMode::ALL {
                    ui.selectable_value(mode, m, m.label());
                }
            });
            ui.end_row();
        }
        Generator::Symmetry { plane } => {
            attr_label(ui, "Mirror Plane");
            egui::ComboBox::from_id_salt("mirror-plane").selected_text(plane.label()).show_ui(ui, |ui| {
                for p in MirrorPlane::ALL {
                    ui.selectable_value(plane, p, p.label());
                }
            });
            ui.end_row();
        }
    });
    ui.add_space(6.0);
    hint(ui, "Child objects are the input and are not drawn on their own.");
    if matches!(g, Generator::Boolean { .. }) {
        hint(ui, "A is the first child, B the second. Both should be closed solids.");
    }
    if matches!(g, Generator::Subdivision { levels } if *levels >= 3) {
        hint(ui, "High levels get slow on dense meshes.");
    }
}

fn primitive(ui: &mut Ui, p: &mut Primitive) {
    heading(ui, &format!("{} Object", p.kind().name()));
    egui::Grid::new("object").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| match p {
        Primitive::Cube { size, segments } => {
            dv(ui, "Size X", &mut size.x);
            dv(ui, "Size Y", &mut size.y);
            dv(ui, "Size Z", &mut size.z);
            seg(ui, "Segments X", &mut segments[0]);
            seg(ui, "Segments Y", &mut segments[1]);
            seg(ui, "Segments Z", &mut segments[2]);
        }
        Primitive::Sphere { radius, segments } => {
            dv(ui, "Radius", radius);
            seg(ui, "Segments", segments);
        }
        Primitive::Cylinder { radius, height, segments, hseg, caps } | Primitive::Cone { radius, height, segments, hseg, caps } => {
            dv(ui, "Radius", radius);
            dv(ui, "Height", height);
            seg(ui, "Rotation Segments", segments);
            seg(ui, "Height Segments", hseg);
            attr_label(ui, "Caps");
            ui.checkbox(caps, "");
            ui.end_row();
        }
        Primitive::Torus { ring_radius, pipe_radius, ring_seg, pipe_seg } => {
            dv(ui, "Ring Radius", ring_radius);
            dv(ui, "Pipe Radius", pipe_radius);
            seg(ui, "Ring Segments", ring_seg);
            seg(ui, "Pipe Segments", pipe_seg);
        }
        Primitive::Plane { width, height, wseg, hseg } => {
            dv(ui, "Width", width);
            dv(ui, "Height", height);
            seg(ui, "Width Segments", wseg);
            seg(ui, "Height Segments", hseg);
        }
        Primitive::Pyramid { size, segments } => {
            dv(ui, "Size X", &mut size.x);
            dv(ui, "Size Y", &mut size.y);
            dv(ui, "Size Z", &mut size.z);
            seg(ui, "Segments", segments);
        }
        Primitive::Landscape { size, segments, roughness, scale, seed, borders } => {
            dv(ui, "Width", &mut size.x);
            dv(ui, "Height", &mut size.y);
            dv(ui, "Depth", &mut size.z);
            seg(ui, "Width Segments", &mut segments[0]);
            seg(ui, "Depth Segments", &mut segments[1]);
            attr_label(ui, "Roughness");
            ui.add(egui::DragValue::new(roughness).range(0.0..=100.0).speed(0.5).suffix(" %"));
            ui.end_row();
            attr_label(ui, "Scale");
            ui.add(egui::DragValue::new(scale).range(1.0..=1000.0).speed(1.0).suffix(" %"));
            ui.end_row();
            attr_label(ui, "Seed");
            ui.add(egui::DragValue::new(seed).range(0..=99_999));
            ui.end_row();
            attr_label(ui, "Borders at Sea Level");
            ui.checkbox(borders, "");
            ui.end_row();
        }
        Primitive::Figure { height, segments } => {
            dv(ui, "Height", height);
            seg(ui, "Segments", segments);
        }
        Primitive::Capsule { radius, height, segments, hseg } => {
            dv(ui, "Radius", radius);
            dv(ui, "Height", height);
            seg(ui, "Rotation Segments", segments);
            seg(ui, "Height Segments", hseg);
        }
        Primitive::Tube { inner_radius, outer_radius, height, segments, hseg } => {
            dv(ui, "Inner Radius", inner_radius);
            dv(ui, "Outer Radius", outer_radius);
            dv(ui, "Height", height);
            seg(ui, "Rotation Segments", segments);
            seg(ui, "Height Segments", hseg);
        }
    });
}

/// A dim explanatory line, indented to line up with the parameter labels.
fn hint(ui: &mut Ui, text: impl Into<egui::RichText>) {
    ui.horizontal_wrapped(|ui| {
        ui.add_space(8.0);
        ui.colored_label(theme::TEXT_DIM, text);
    });
}
