//! Procedurally drawn icons. Everything is built from strokes in a unit square,
//! so icons stay crisp at any size and the app ships no image assets.

use eframe::egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, pos2, vec2};
use std::f32::consts::{PI, TAU};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Select,
    Move,
    Scale,
    Rotate,
    Cube,
    Sphere,
    Cylinder,
    Cone,
    Torus,
    Pyramid,
    Tube,
    Capsule,
    Landscape,
    Figure,
    Spline,
    SplinePen,
    LightArea,
    LightPoint,
    LightSpot,
    LightOmni,
    LightInfinite,
    LightDome,
    LightSun,
    PolygonPen,
    Plane,
    Null,
    Undo,
    Redo,
    Play,
    Pause,
    Material,
    SingleView,
    QuadView,
    Frame,
    Delete,
    Duplicate,
    Group,
    Zoom,
    Subdivision,
    Array,
    Symmetry,
    Bend,
    Twist,
    Taper,
    Shear,
    Boolean,
    Polygon,
    Points,
    Edges,
    Polygons,
    Model,
    Extrude,
    Inset,
    LoopCut,
    Bevel,
    Knife,
    WorldCoords,
    ObjectCoords,
    MakeEditable,
    Solo,
    NavPan,
    NavZoom,
    NavOrbit,
    NavMaximize,
    ToStart,
    ToEnd,
    PrevFrame,
    NextFrame,
    PlayBackward,
    Layout,
    PostEffects,
    Tessellation,
    Shadows,
    Reflections,
    Ssao,
    BackfaceCulling,
    Grid,
    Compass,
    FrameAll,
    Gouraud,
    GouraudLines,
    Wireframe,
    Quick,
    QuickLines,
    Timeline,
    Commander,
    Shelf,
    ObjectMode,
    AxisMode,
    Workplane,
    Tweak,
    Snap,
    Quantize,
    Layers,
    Takes,
    Structure,
    Assets,
    Console,
}

/// Paints `icon` fitted into `rect`.
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let side = rect.width().min(rect.height());
    let r = Rect::from_center_size(rect.center(), vec2(side, side));
    let p = |x: f32, y: f32| pos2(r.min.x + x * r.width(), r.min.y + y * r.height());
    let w = (side / 14.0).clamp(1.0, 2.0);
    let stroke = Stroke::new(w, color);
    let thin = Stroke::new((w * 0.7).max(1.0), color);
    let line = |a: Pos2, b: Pos2| painter.line_segment([a, b], stroke);
    let poly = |pts: Vec<Pos2>, closed: bool| {
        if closed {
            painter.add(Shape::closed_line(pts, stroke));
        } else {
            painter.add(Shape::line(pts, stroke));
        }
    };
    let ellipse = |cx: f32, cy: f32, rx: f32, ry: f32, from: f32, to: f32, st: Stroke| {
        let n = 28;
        let pts: Vec<Pos2> = (0..=n)
            .map(|i| {
                let a = from + (to - from) * i as f32 / n as f32;
                p(cx + rx * a.cos(), cy + ry * a.sin())
            })
            .collect();
        painter.add(Shape::line(pts, st));
    };
    let arrow_head = |tip: Pos2, dir: Pos2, size: f32| {
        // `dir` is a unit direction expressed as a position offset from the origin.
        let d = vec2(dir.x, dir.y);
        let n = vec2(-d.y, d.x);
        let base = tip - d * size;
        painter.add(Shape::convex_polygon(
            vec![tip, base + n * size * 0.55, base - n * size * 0.55],
            color,
            Stroke::NONE,
        ));
    };

    match icon {
        Icon::Select => {
            let pts = vec![p(0.26, 0.12), p(0.26, 0.82), p(0.43, 0.66), p(0.56, 0.90), p(0.67, 0.85), p(0.55, 0.62), p(0.78, 0.62)];
            painter.add(Shape::convex_polygon(pts.clone(), color.gamma_multiply(0.35), stroke));
        }
        Icon::Move => {
            line(p(0.5, 0.14), p(0.5, 0.86));
            line(p(0.14, 0.5), p(0.86, 0.5));
            arrow_head(p(0.5, 0.06), pos2(0.0, -1.0), side * 0.2);
            arrow_head(p(0.5, 0.94), pos2(0.0, 1.0), side * 0.2);
            arrow_head(p(0.06, 0.5), pos2(-1.0, 0.0), side * 0.2);
            arrow_head(p(0.94, 0.5), pos2(1.0, 0.0), side * 0.2);
        }
        Icon::Scale => {
            painter.rect_stroke(Rect::from_min_max(p(0.12, 0.40), p(0.60, 0.88)), 1.0, stroke, StrokeKind::Inside);
            line(p(0.60, 0.40), p(0.86, 0.14));
            arrow_head(p(0.90, 0.10), pos2(0.7, -0.7), side * 0.22);
        }
        Icon::Rotate => {
            ellipse(0.5, 0.5, 0.34, 0.34, -0.3 * PI, 1.35 * PI, stroke);
            let tip = p(0.5 + 0.34 * (1.35 * PI).cos(), 0.5 + 0.34 * (1.35 * PI).sin());
            arrow_head(tip, pos2(0.8, 0.6), side * 0.22);
        }
        Icon::Cube => {
            poly(vec![p(0.5, 0.08), p(0.88, 0.28), p(0.88, 0.72), p(0.5, 0.92), p(0.12, 0.72), p(0.12, 0.28)], true);
            line(p(0.12, 0.28), p(0.5, 0.48));
            line(p(0.88, 0.28), p(0.5, 0.48));
            line(p(0.5, 0.48), p(0.5, 0.92));
        }
        Icon::Sphere => {
            ellipse(0.5, 0.5, 0.40, 0.40, 0.0, TAU, stroke);
            ellipse(0.5, 0.5, 0.40, 0.14, 0.0, TAU, thin);
            ellipse(0.5, 0.5, 0.15, 0.40, 0.0, TAU, thin);
        }
        Icon::Cylinder => {
            ellipse(0.5, 0.22, 0.32, 0.12, 0.0, TAU, stroke);
            ellipse(0.5, 0.78, 0.32, 0.12, 0.0, PI, stroke);
            line(p(0.18, 0.22), p(0.18, 0.78));
            line(p(0.82, 0.22), p(0.82, 0.78));
        }
        Icon::Cone => {
            line(p(0.5, 0.08), p(0.16, 0.76));
            line(p(0.5, 0.08), p(0.84, 0.76));
            ellipse(0.5, 0.76, 0.34, 0.13, 0.0, TAU, stroke);
        }
        Icon::Pyramid => {
            let (apex, l, f, r, b) = (p(0.5, 0.08), p(0.08, 0.72), p(0.42, 0.90), p(0.92, 0.74), p(0.60, 0.60));
            poly(vec![l, f, r, apex], true);
            line(apex, f);
            painter.line_segment([l, b], thin);
            painter.line_segment([b, r], thin);
            painter.line_segment([b, apex], thin);
        }
        Icon::Capsule => {
            let (l, r) = (p(0.30, 0.0).x, p(0.70, 0.0).x);
            ellipse(0.5, 0.30, 0.20, 0.20, PI, TAU, stroke);
            ellipse(0.5, 0.70, 0.20, 0.20, 0.0, PI, stroke);
            line(pos2(l, p(0.0, 0.30).y), pos2(l, p(0.0, 0.70).y));
            line(pos2(r, p(0.0, 0.30).y), pos2(r, p(0.0, 0.70).y));
            ellipse(0.5, 0.50, 0.20, 0.06, 0.0, PI, thin);
        }
        Icon::LightPoint | Icon::LightOmni | Icon::LightSun => {
            // A bulb (point), a star burst (omni) or a sun.
            let c = r.center();
            let (core, inner, outer, n) = match icon {
                Icon::LightPoint => (0.13, 0.22, 0.34, 6),
                Icon::LightOmni => (0.16, 0.24, 0.46, 8),
                _ => (0.20, 0.30, 0.44, 12),
            };
            if icon == Icon::LightSun {
                painter.circle_stroke(c, side * core, stroke);
            } else {
                painter.circle_filled(c, side * core, color);
            }
            for k in 0..n {
                let a = TAU * k as f32 / n as f32;
                let d = vec2(a.cos(), a.sin());
                painter.line_segment([c + d * side * inner, c + d * side * outer], thin);
            }
        }
        Icon::LightSpot => {
            painter.circle_filled(p(0.5, 0.16), side * 0.08, color);
            line(p(0.5, 0.16), p(0.16, 0.80));
            line(p(0.5, 0.16), p(0.84, 0.80));
            ellipse(0.5, 0.80, 0.34, 0.10, 0.0, TAU, stroke);
        }
        Icon::LightArea => {
            poly(vec![p(0.14, 0.18), p(0.70, 0.10), p(0.70, 0.56), p(0.14, 0.64)], true);
            for (x, y) in [(0.30, 0.66), (0.50, 0.62), (0.70, 0.58)] {
                painter.line_segment([p(x, y), p(x + 0.12, y + 0.26)], thin);
            }
        }
        Icon::LightInfinite => {
            for y in [0.26, 0.50, 0.74] {
                line(p(0.08, y), p(0.80, y));
                arrow_head(p(0.92, y), pos2(1.0, 0.0), side * 0.18);
            }
        }
        Icon::LightDome => {
            ellipse(0.5, 0.78, 0.42, 0.42, PI, TAU, stroke);
            line(p(0.08, 0.78), p(0.92, 0.78));
            ellipse(0.5, 0.78, 0.16, 0.42, PI, TAU, thin);
        }
        Icon::Spline => {
            // An S-curve with its end points.
            let pts: Vec<Pos2> = (0..=16)
                .map(|k| {
                    let t = k as f32 / 16.0;
                    p(0.10 + 0.80 * t, 0.5 - 0.32 * (t * TAU).sin())
                })
                .collect();
            painter.add(Shape::line(pts, stroke));
            painter.rect_filled(Rect::from_center_size(p(0.10, 0.5), vec2(side * 0.12, side * 0.12)), 0.0, color);
            painter.rect_filled(Rect::from_center_size(p(0.90, 0.5), vec2(side * 0.12, side * 0.12)), 0.0, color);
        }
        Icon::SplinePen | Icon::PolygonPen => {
            // A pen nib, with a curve or a polygon behind it.
            if icon == Icon::SplinePen {
                let pts: Vec<Pos2> = (0..=12)
                    .map(|k| {
                        let t = k as f32 / 12.0;
                        p(0.06 + 0.5 * t, 0.86 - 0.30 * (t * PI).sin())
                    })
                    .collect();
                painter.add(Shape::line(pts, thin));
            } else {
                poly(vec![p(0.06, 0.88), p(0.30, 0.56), p(0.56, 0.70), p(0.40, 0.94)], true);
            }
            let (tip, a, b, top) = (p(0.50, 0.62), p(0.66, 0.24), p(0.88, 0.46), p(0.90, 0.08));
            painter.add(Shape::convex_polygon(vec![tip, a, top, b], color.gamma_multiply(0.25), stroke));
            painter.line_segment([tip, p(0.72, 0.38)], thin);
            painter.circle_filled(p(0.72, 0.38), side * 0.04, color);
        }
        Icon::Landscape => {
            // Two peaks over a ground line.
            poly(vec![p(0.04, 0.82), p(0.30, 0.34), p(0.46, 0.56), p(0.64, 0.20), p(0.96, 0.82)], false);
            line(p(0.04, 0.82), p(0.96, 0.82));
            painter.line_segment([p(0.56, 0.34), p(0.64, 0.42)], thin);
            painter.line_segment([p(0.64, 0.42), p(0.72, 0.34)], thin);
        }
        Icon::Figure => {
            painter.circle_stroke(p(0.5, 0.15), side * 0.10, stroke);
            line(p(0.5, 0.26), p(0.5, 0.58));
            line(p(0.24, 0.50), p(0.36, 0.32));
            line(p(0.36, 0.32), p(0.64, 0.32));
            line(p(0.64, 0.32), p(0.76, 0.50));
            line(p(0.5, 0.58), p(0.36, 0.92));
            line(p(0.5, 0.58), p(0.64, 0.92));
        }
        Icon::Tube => {
            ellipse(0.5, 0.24, 0.36, 0.13, 0.0, TAU, stroke);
            ellipse(0.5, 0.24, 0.17, 0.06, 0.0, TAU, thin);
            ellipse(0.5, 0.78, 0.36, 0.13, 0.0, PI, stroke);
            line(p(0.14, 0.24), p(0.14, 0.78));
            line(p(0.86, 0.24), p(0.86, 0.78));
        }
        Icon::Torus => {
            ellipse(0.5, 0.5, 0.42, 0.28, 0.0, TAU, stroke);
            ellipse(0.5, 0.5, 0.17, 0.10, 0.0, TAU, thin);
        }
        Icon::Plane => {
            poly(vec![p(0.30, 0.28), p(0.92, 0.28), p(0.70, 0.76), p(0.08, 0.76)], true);
            line(p(0.61, 0.28), p(0.39, 0.76));
            line(p(0.19, 0.52), p(0.81, 0.52));
        }
        Icon::Null => {
            line(p(0.15, 0.5), p(0.85, 0.5));
            line(p(0.5, 0.15), p(0.5, 0.85));
            line(p(0.28, 0.72), p(0.72, 0.28));
        }
        Icon::Undo | Icon::Redo => {
            let flip = icon == Icon::Redo;
            let x = |v: f32| if flip { 1.0 - v } else { v };
            let pts: Vec<Pos2> = (0..=16)
                .map(|i| {
                    let a = -0.5 * PI + PI * 1.05 * i as f32 / 16.0;
                    p(x(0.52 + 0.28 * a.cos()), 0.60 + 0.28 * a.sin())
                })
                .collect();
            painter.add(Shape::line(pts, stroke));
            line(p(x(0.52), 0.32), p(x(0.30), 0.32));
            arrow_head(p(x(0.14), 0.32), pos2(if flip { 1.0 } else { -1.0 }, 0.0), side * 0.24);
        }
        Icon::Play => {
            painter.add(Shape::convex_polygon(vec![p(0.28, 0.14), p(0.28, 0.86), p(0.84, 0.5)], color, Stroke::NONE));
        }
        Icon::Pause => {
            painter.rect_filled(Rect::from_min_max(p(0.24, 0.16), p(0.43, 0.84)), 1.0, color);
            painter.rect_filled(Rect::from_min_max(p(0.57, 0.16), p(0.76, 0.84)), 1.0, color);
        }
        Icon::Material => {
            painter.circle_filled(r.center(), side * 0.42, color.gamma_multiply(0.55));
            painter.circle_filled(p(0.40, 0.38), side * 0.14, color);
            painter.circle_stroke(r.center(), side * 0.42, thin);
        }
        Icon::SingleView => {
            painter.rect_stroke(Rect::from_min_max(p(0.10, 0.18), p(0.90, 0.82)), 1.0, stroke, StrokeKind::Inside);
        }
        Icon::QuadView => {
            painter.rect_stroke(Rect::from_min_max(p(0.10, 0.18), p(0.90, 0.82)), 1.0, stroke, StrokeKind::Inside);
            line(p(0.5, 0.18), p(0.5, 0.82));
            line(p(0.10, 0.5), p(0.90, 0.5));
        }
        Icon::Frame => {
            for (cx, cy, dx, dy) in [(0.14, 0.14, 1.0, 1.0), (0.86, 0.14, -1.0, 1.0), (0.14, 0.86, 1.0, -1.0), (0.86, 0.86, -1.0, -1.0)] {
                poly(vec![p(cx + dx * 0.22, cy), p(cx, cy), p(cx, cy + dy * 0.22)], false);
            }
            painter.circle_filled(r.center(), side * 0.07, color);
        }
        Icon::Delete => {
            line(p(0.22, 0.22), p(0.78, 0.78));
            line(p(0.78, 0.22), p(0.22, 0.78));
        }
        Icon::Duplicate => {
            painter.rect_stroke(Rect::from_min_max(p(0.12, 0.12), p(0.62, 0.62)), 1.0, stroke, StrokeKind::Inside);
            painter.rect_stroke(Rect::from_min_max(p(0.38, 0.38), p(0.88, 0.88)), 1.0, stroke, StrokeKind::Inside);
        }
        Icon::Zoom => {
            painter.circle_stroke(p(0.43, 0.43), side * 0.28, stroke);
            line(p(0.64, 0.64), p(0.88, 0.88));
            line(p(0.30, 0.43), p(0.56, 0.43));
            line(p(0.43, 0.30), p(0.43, 0.56));
        }
        Icon::Subdivision => {
            painter.rect_stroke(Rect::from_min_max(p(0.12, 0.12), p(0.88, 0.88)), 1.0, thin, StrokeKind::Inside);
            ellipse(0.5, 0.5, 0.36, 0.36, 0.0, TAU, stroke);
            ellipse(0.5, 0.5, 0.36, 0.14, 0.0, TAU, thin);
        }
        Icon::Array => {
            for x in [0.10, 0.38, 0.66] {
                painter.rect_stroke(Rect::from_min_max(p(x, 0.34), p(x + 0.24, 0.66)), 1.0, stroke, StrokeKind::Inside);
            }
        }
        Icon::Symmetry => {
            line(p(0.5, 0.10), p(0.5, 0.90));
            poly(vec![p(0.40, 0.25), p(0.40, 0.75), p(0.12, 0.62)], true);
            poly(vec![p(0.60, 0.25), p(0.60, 0.75), p(0.88, 0.62)], true);
        }
        Icon::Polygon => {
            // A small triangulated patch.
            poly(vec![p(0.12, 0.80), p(0.30, 0.22), p(0.74, 0.14), p(0.88, 0.66), p(0.50, 0.88)], true);
            line(p(0.30, 0.22), p(0.50, 0.88));
            line(p(0.50, 0.88), p(0.74, 0.14));
            line(p(0.74, 0.14), p(0.12, 0.80));
        }
        Icon::Model => {
            painter.rect_stroke(Rect::from_min_max(p(0.18, 0.18), p(0.82, 0.82)), 2.0, stroke, StrokeKind::Inside);
            painter.circle_filled(r.center(), side * 0.10, color);
        }
        Icon::Points => {
            poly(vec![p(0.20, 0.30), p(0.80, 0.22), p(0.74, 0.78), p(0.26, 0.80)], true);
            for (x, y) in [(0.20, 0.30), (0.80, 0.22), (0.74, 0.78), (0.26, 0.80)] {
                painter.circle_filled(p(x, y), side * 0.10, color);
            }
        }
        Icon::Edges => {
            painter.add(Shape::closed_line(
                vec![p(0.20, 0.30), p(0.80, 0.22), p(0.74, 0.78), p(0.26, 0.80)],
                thin,
            ));
            painter.line_segment([p(0.80, 0.22), p(0.74, 0.78)], Stroke::new(w * 2.2, color));
            painter.line_segment([p(0.20, 0.30), p(0.80, 0.22)], Stroke::new(w * 2.2, color));
        }
        Icon::Polygons => {
            painter.add(Shape::convex_polygon(
                vec![p(0.20, 0.30), p(0.80, 0.22), p(0.74, 0.78), p(0.26, 0.80)],
                color.gamma_multiply(0.55),
                thin,
            ));
        }
        Icon::Extrude => {
            painter.rect_stroke(Rect::from_min_max(p(0.16, 0.56), p(0.84, 0.88)), 1.0, thin, StrokeKind::Inside);
            painter.rect_filled(Rect::from_min_max(p(0.30, 0.34), p(0.70, 0.56)), 1.0, color.gamma_multiply(0.6));
            line(p(0.5, 0.34), p(0.5, 0.12));
            arrow_head(p(0.5, 0.06), pos2(0.0, -1.0), side * 0.18);
        }
        Icon::Inset => {
            painter.rect_stroke(Rect::from_min_max(p(0.12, 0.12), p(0.88, 0.88)), 1.0, thin, StrokeKind::Inside);
            painter.rect_filled(Rect::from_min_max(p(0.32, 0.32), p(0.68, 0.68)), 1.0, color.gamma_multiply(0.6));
            for (a, b) in [((0.12, 0.12), (0.32, 0.32)), ((0.88, 0.12), (0.68, 0.32)), ((0.12, 0.88), (0.32, 0.68)), ((0.88, 0.88), (0.68, 0.68))] {
                painter.line_segment([p(a.0, a.1), p(b.0, b.1)], thin);
            }
        }
        Icon::WorldCoords => {
            painter.circle_stroke(r.center(), side * 0.38, stroke);
            ellipse(0.5, 0.5, 0.17, 0.38, 0.0, TAU, thin);
            line(p(0.12, 0.5), p(0.88, 0.5));
        }
        Icon::ObjectCoords => {
            poly(vec![p(0.5, 0.12), p(0.86, 0.30), p(0.86, 0.70), p(0.5, 0.88), p(0.14, 0.70), p(0.14, 0.30)], true);
            line(p(0.5, 0.5), p(0.5, 0.12));
            line(p(0.5, 0.5), p(0.86, 0.70));
            line(p(0.5, 0.5), p(0.14, 0.70));
        }
        Icon::MakeEditable => {
            // A cube turning into points.
            poly(vec![p(0.14, 0.32), p(0.50, 0.14), p(0.86, 0.32), p(0.86, 0.72), p(0.50, 0.90), p(0.14, 0.72)], true);
            for (x, y) in [(0.14, 0.32), (0.50, 0.14), (0.86, 0.32), (0.86, 0.72), (0.50, 0.90), (0.14, 0.72), (0.5, 0.5)] {
                painter.circle_filled(p(x, y), side * 0.07, color);
            }
            line(p(0.5, 0.5), p(0.5, 0.90));
        }
        Icon::Solo => {
            painter.circle_stroke(p(0.5, 0.42), side * 0.22, stroke);
            painter.add(Shape::convex_polygon(vec![p(0.5, 0.42), p(0.28, 0.90), p(0.72, 0.90)], color.gamma_multiply(0.5), thin));
            painter.circle_filled(p(0.5, 0.42), side * 0.08, color);
        }
        Icon::NavPan => {
            line(p(0.5, 0.16), p(0.5, 0.84));
            line(p(0.16, 0.5), p(0.84, 0.5));
            arrow_head(p(0.5, 0.08), pos2(0.0, -1.0), side * 0.24);
            arrow_head(p(0.5, 0.92), pos2(0.0, 1.0), side * 0.24);
            arrow_head(p(0.08, 0.5), pos2(-1.0, 0.0), side * 0.24);
            arrow_head(p(0.92, 0.5), pos2(1.0, 0.0), side * 0.24);
        }
        Icon::NavZoom => {
            // Converging arrows (in / out).
            line(p(0.16, 0.84), p(0.84, 0.16));
            arrow_head(p(0.88, 0.12), pos2(0.7, -0.7), side * 0.26);
            arrow_head(p(0.12, 0.88), pos2(-0.7, 0.7), side * 0.26);
            painter.circle_stroke(r.center(), side * 0.16, thin);
        }
        Icon::NavOrbit => {
            ellipse(0.5, 0.5, 0.38, 0.38, 0.15 * PI, 1.85 * PI, stroke);
            let tip = p(0.5 + 0.38 * (0.15 * PI).cos(), 0.5 + 0.38 * (0.15 * PI).sin());
            arrow_head(tip, pos2(-0.5, 0.86), side * 0.26);
            painter.circle_filled(r.center(), side * 0.08, color);
        }
        Icon::NavMaximize => {
            painter.rect_stroke(Rect::from_min_max(p(0.12, 0.12), p(0.88, 0.88)), 1.0, stroke, StrokeKind::Inside);
            painter.rect_filled(Rect::from_min_max(p(0.12, 0.12), p(0.5, 0.5)), 0.0, color.gamma_multiply(0.6));
        }
        Icon::ToStart | Icon::ToEnd => {
            let f = icon == Icon::ToEnd;
            let x = |v: f32| if f { 1.0 - v } else { v };
            painter.rect_filled(Rect::from_min_max(p(x(0.18).min(x(0.28)), 0.2), p(x(0.18).max(x(0.28)), 0.8)), 0.0, color);
            painter.add(Shape::convex_polygon(
                if f { vec![p(0.24, 0.2), p(0.66, 0.5), p(0.24, 0.8)] } else { vec![p(0.76, 0.2), p(0.76, 0.8), p(0.34, 0.5)] },
                color,
                Stroke::NONE,
            ));
        }
        Icon::PrevFrame | Icon::NextFrame => {
            let f = icon == Icon::NextFrame;
            let pts = if f {
                vec![p(0.30, 0.22), p(0.74, 0.5), p(0.30, 0.78)]
            } else {
                vec![p(0.70, 0.22), p(0.70, 0.78), p(0.26, 0.5)]
            };
            painter.add(Shape::closed_line(pts, stroke));
        }
        Icon::PlayBackward => {
            painter.add(Shape::convex_polygon(vec![p(0.72, 0.14), p(0.72, 0.86), p(0.16, 0.5)], color, Stroke::NONE));
        }
        Icon::Layout => {
            painter.rect_stroke(Rect::from_min_max(p(0.10, 0.16), p(0.90, 0.84)), 1.0, stroke, StrokeKind::Inside);
            line(p(0.64, 0.16), p(0.64, 0.84));
            line(p(0.10, 0.62), p(0.64, 0.62));
        }
        Icon::Bevel => {
            // A square corner with its tip cut off.
            poly(vec![p(0.14, 0.88), p(0.14, 0.14), p(0.56, 0.14), p(0.86, 0.44), p(0.86, 0.88)], false);
            painter.line_segment([p(0.56, 0.14), p(0.86, 0.44)], Stroke::new(w * 2.2, color));
        }
        Icon::Knife => {
            painter.add(Shape::convex_polygon(vec![p(0.12, 0.88), p(0.62, 0.30), p(0.74, 0.42)], color.gamma_multiply(0.6), stroke));
            painter.line_segment([p(0.68, 0.36), p(0.88, 0.14)], Stroke::new(w * 2.4, color));
        }
        Icon::LoopCut => {
            painter.rect_stroke(Rect::from_min_max(p(0.12, 0.20), p(0.88, 0.80)), 1.0, thin, StrokeKind::Inside);
            painter.line_segment([p(0.45, 0.12), p(0.45, 0.88)], Stroke::new(w * 1.6, color));
        }
        Icon::Boolean => {
            painter.circle_stroke(p(0.38, 0.5), side * 0.28, stroke);
            painter.circle_filled(p(0.62, 0.5), side * 0.28, color.gamma_multiply(0.45));
            painter.circle_stroke(p(0.62, 0.5), side * 0.28, thin);
        }
        Icon::Bend => {
            // A bar standing at the bottom left and curling over to the right.
            ellipse(0.88, 0.88, 0.70, 0.70, PI, 1.5 * PI, stroke);
            ellipse(0.88, 0.88, 0.40, 0.40, PI, 1.5 * PI, stroke);
            line(p(0.18, 0.88), p(0.48, 0.88));
            line(p(0.88, 0.18), p(0.88, 0.48));
        }
        Icon::Taper => {
            // A column narrowing towards the top.
            poly(vec![p(0.12, 0.90), p(0.88, 0.90), p(0.64, 0.12), p(0.36, 0.12)], true);
            painter.line_segment([p(0.24, 0.62), p(0.76, 0.62)], thin);
            painter.line_segment([p(0.30, 0.36), p(0.70, 0.36)], thin);
        }
        Icon::Shear => {
            // A column leaning over, with an arrow showing the slide.
            poly(vec![p(0.10, 0.90), p(0.52, 0.90), p(0.90, 0.22), p(0.48, 0.22)], true);
            painter.line_segment([p(0.20, 0.56), p(0.62, 0.56)], thin);
            line(p(0.18, 0.10), p(0.58, 0.10));
            arrow_head(p(0.70, 0.10), pos2(1.0, 0.0), side * 0.18);
        }
        Icon::Twist => {
            let wave = |phase: f32| -> Vec<Pos2> {
                (0..=20)
                    .map(|i| {
                        let t = i as f32 / 20.0;
                        p(0.5 + 0.30 * (t * TAU * 0.75 + phase).sin(), 0.10 + 0.80 * t)
                    })
                    .collect()
            };
            painter.add(Shape::line(wave(0.0), stroke));
            painter.add(Shape::line(wave(PI), thin));
        }
        Icon::Group => {
            painter.rect_stroke(Rect::from_min_max(p(0.10, 0.10), p(0.90, 0.90)), 2.0, thin, StrokeKind::Inside);
            painter.circle_filled(p(0.34, 0.5), side * 0.11, color);
            painter.circle_filled(p(0.66, 0.5), side * 0.11, color);
        }
        Icon::PostEffects => {
            // A frame with a sparkle.
            painter.rect_stroke(Rect::from_min_max(p(0.08, 0.20), p(0.92, 0.80)), 1.5, thin, StrokeKind::Inside);
            let c = p(0.5, 0.5);
            let (a, b) = (side * 0.26, side * 0.07);
            painter.add(Shape::convex_polygon(
                vec![c + vec2(0.0, -a), c + vec2(b, -b), c + vec2(a, 0.0), c + vec2(b, b), c + vec2(0.0, a), c + vec2(-b, b), c + vec2(-a, 0.0), c + vec2(-b, -b)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Tessellation => {
            // A triangle split into nine.
            let (a, b, c) = (p(0.5, 0.10), p(0.08, 0.88), p(0.92, 0.88));
            poly(vec![a, b, c], true);
            let at = |u: f32, v: f32| a + (b - a) * u + (c - a) * v;
            for k in [1.0 / 3.0, 2.0 / 3.0] {
                painter.line_segment([at(k, 0.0), at(0.0, k)], thin);
                painter.line_segment([at(k, 0.0), at(k, 1.0 - k)], thin);
                painter.line_segment([at(0.0, k), at(1.0 - k, k)], thin);
            }
        }
        Icon::Shadows => {
            painter.add(Shape::ellipse_filled(p(0.62, 0.80), vec2(side * 0.30, side * 0.09), color.gamma_multiply(0.55)));
            painter.circle_filled(p(0.42, 0.42), side * 0.25, color);
        }
        Icon::Reflections => {
            painter.circle_stroke(r.center(), side * 0.38, stroke);
            ellipse(0.5, 0.5, 0.26, 0.26, PI * 1.1, PI * 1.45, thin);
            painter.rect_filled(Rect::from_min_max(p(0.30, 0.28), p(0.40, 0.40)), 0.0, color);
            painter.rect_filled(Rect::from_min_max(p(0.43, 0.28), p(0.53, 0.40)), 0.0, color);
        }
        Icon::Ssao => {
            // A wall meeting the floor, darkened along the crease.
            poly(vec![p(0.20, 0.10), p(0.20, 0.72), p(0.92, 0.72)], false);
            line(p(0.20, 0.72), p(0.06, 0.90));
            painter.add(Shape::convex_polygon(vec![p(0.22, 0.30), p(0.22, 0.70), p(0.62, 0.70)], color.gamma_multiply(0.45), Stroke::NONE));
        }
        Icon::BackfaceCulling => {
            // A face with its normal; the hidden back is dashed.
            poly(vec![p(0.12, 0.30), p(0.62, 0.18), p(0.62, 0.82), p(0.12, 0.70)], true);
            line(p(0.37, 0.50), p(0.86, 0.50));
            arrow_head(p(0.94, 0.50), pos2(1.0, 0.0), side * 0.2);
            for k in 0..3 {
                let y = 0.30 + k as f32 * 0.17;
                painter.line_segment([p(0.04, y), p(0.04, y + 0.08)], thin);
            }
        }
        Icon::Grid => {
            // A floor grid in perspective.
            let (bl, br, tr, tl) = (p(0.02, 0.86), p(0.98, 0.86), p(0.78, 0.22), p(0.22, 0.22));
            poly(vec![bl, br, tr, tl], true);
            for k in 1..4 {
                let t = k as f32 / 4.0;
                painter.line_segment([bl + (br - bl) * t, tl + (tr - tl) * t], thin);
                let y = t * t;
                painter.line_segment([bl + (tl - bl) * (1.0 - y), br + (tr - br) * (1.0 - y)], thin);
            }
        }
        Icon::Compass => {
            let o = p(0.36, 0.64);
            line(o, p(0.36, 0.08));
            line(o, p(0.92, 0.64));
            line(o, p(0.08, 0.92));
            painter.circle_filled(o, side * 0.07, color);
        }
        Icon::FrameAll => {
            for (cx, cy, dx, dy) in [(0.10, 0.10, 1.0, 1.0), (0.90, 0.10, -1.0, 1.0), (0.10, 0.90, 1.0, -1.0), (0.90, 0.90, -1.0, -1.0)] {
                poly(vec![p(cx + dx * 0.2, cy), p(cx, cy), p(cx, cy + dy * 0.2)], false);
            }
            for (x, y) in [(0.36, 0.40), (0.62, 0.36), (0.48, 0.64)] {
                painter.circle_filled(p(x, y), side * 0.08, color);
            }
        }
        Icon::Gouraud | Icon::GouraudLines | Icon::Wireframe | Icon::Quick | Icon::QuickLines => {
            let c = r.center();
            let rad = side * 0.40;
            if matches!(icon, Icon::Gouraud | Icon::GouraudLines) {
                painter.circle_filled(c, rad, color.gamma_multiply(0.45));
                painter.circle_filled(c + vec2(-rad * 0.3, -rad * 0.3), rad * 0.45, color.gamma_multiply(0.8));
            } else if icon != Icon::Wireframe {
                // Quick: flat fill, brighter towards the middle, no highlight.
                painter.circle_filled(c, rad, color.gamma_multiply(0.35));
                painter.circle_filled(c, rad * 0.62, color.gamma_multiply(0.6));
            }
            painter.circle_stroke(c, rad, thin);
            if matches!(icon, Icon::GouraudLines | Icon::Wireframe | Icon::QuickLines) {
                ellipse(0.5, 0.5, 0.40, 0.14, 0.0, TAU, thin);
                ellipse(0.5, 0.5, 0.14, 0.40, 0.0, TAU, thin);
            }
        }
        Icon::Timeline => {
            line(p(0.06, 0.62), p(0.94, 0.62));
            for k in 0..7 {
                let x = 0.10 + k as f32 * 0.133;
                painter.line_segment([p(x, 0.62), p(x, if k % 3 == 0 { 0.46 } else { 0.54 })], thin);
            }
            painter.add(Shape::convex_polygon(vec![p(0.36, 0.20), p(0.56, 0.20), p(0.46, 0.34)], color, Stroke::NONE));
            line(p(0.46, 0.30), p(0.46, 0.86));
        }
        Icon::Commander => {
            painter.circle_stroke(p(0.42, 0.42), side * 0.25, stroke);
            line(p(0.60, 0.60), p(0.88, 0.88));
        }
        Icon::Shelf => {
            painter.rect_stroke(Rect::from_min_max(p(0.06, 0.30), p(0.94, 0.70)), 2.0, thin, StrokeKind::Inside);
            for x in [0.22, 0.42, 0.62] {
                painter.rect_filled(Rect::from_center_size(p(x, 0.5), vec2(side * 0.12, side * 0.12)), 1.0, color);
            }
            line(p(0.80, 0.42), p(0.80, 0.58));
            line(p(0.72, 0.50), p(0.88, 0.50));
        }
        Icon::ObjectMode => {
            // A cube with its axis cross.
            poly(vec![p(0.14, 0.34), p(0.50, 0.18), p(0.86, 0.34), p(0.86, 0.72), p(0.50, 0.88), p(0.14, 0.72)], true);
            poly(vec![p(0.14, 0.34), p(0.50, 0.50), p(0.86, 0.34)], false);
            line(p(0.50, 0.50), p(0.50, 0.88));
            painter.circle_filled(p(0.5, 0.5), side * 0.08, color);
        }
        Icon::AxisMode => {
            // Three axis arrows from a pivot.
            let o = p(0.34, 0.66);
            line(o, p(0.34, 0.14));
            arrow_head(p(0.34, 0.08), pos2(0.0, -1.0), side * 0.18);
            line(o, p(0.86, 0.66));
            arrow_head(p(0.92, 0.66), pos2(1.0, 0.0), side * 0.18);
            line(o, p(0.12, 0.88));
            painter.circle_stroke(o, side * 0.09, stroke);
        }
        Icon::Workplane => {
            // A tilted plane with its grid and normal.
            let (a, b, c, d) = (p(0.06, 0.70), p(0.62, 0.92), p(0.94, 0.56), p(0.38, 0.36));
            poly(vec![a, b, c, d], true);
            for k in 1..3 {
                let t = k as f32 / 3.0;
                painter.line_segment([a + (b - a) * t, d + (c - d) * t], thin);
                painter.line_segment([a + (d - a) * t, b + (c - b) * t], thin);
            }
            line(p(0.50, 0.64), p(0.50, 0.12));
            arrow_head(p(0.50, 0.06), pos2(0.0, -1.0), side * 0.18);
        }
        Icon::Tweak => {
            // A hand-ish pointer dragging a point.
            painter.circle_filled(p(0.30, 0.30), side * 0.10, color);
            painter.circle_stroke(p(0.30, 0.30), side * 0.18, thin);
            let pts = vec![p(0.46, 0.42), p(0.46, 0.92), p(0.58, 0.80), p(0.66, 0.96), p(0.74, 0.92), p(0.66, 0.76), p(0.82, 0.76)];
            painter.add(Shape::convex_polygon(pts, color.gamma_multiply(0.35), stroke));
        }
        Icon::Snap => {
            // A horseshoe magnet.
            ellipse(0.5, 0.48, 0.30, 0.30, PI, TAU, Stroke::new(w * 2.4, color));
            painter.line_segment([p(0.20, 0.48), p(0.20, 0.80)], Stroke::new(w * 2.4, color));
            painter.line_segment([p(0.80, 0.48), p(0.80, 0.80)], Stroke::new(w * 2.4, color));
            painter.rect_filled(Rect::from_min_max(p(0.13, 0.74), p(0.27, 0.90)), 0.0, color.gamma_multiply(0.5));
            painter.rect_filled(Rect::from_min_max(p(0.73, 0.74), p(0.87, 0.90)), 0.0, color.gamma_multiply(0.5));
        }
        Icon::Quantize => {
            // A staircase.
            poly(vec![p(0.10, 0.86), p(0.10, 0.66), p(0.34, 0.66), p(0.34, 0.44), p(0.58, 0.44), p(0.58, 0.22), p(0.86, 0.22)], false);
            for (x, y) in [(0.10, 0.66), (0.34, 0.44), (0.58, 0.22)] {
                painter.circle_filled(p(x, y), side * 0.06, color);
            }
        }
        Icon::Layers => {
            // Three stacked sheets.
            for (k, y) in [0.30f32, 0.50, 0.70].into_iter().enumerate() {
                let pts = vec![p(0.50, y - 0.16), p(0.90, y), p(0.50, y + 0.16), p(0.10, y)];
                let fill = if k == 0 { color.gamma_multiply(0.45) } else { Color32::TRANSPARENT };
                painter.add(Shape::convex_polygon(pts, fill, thin));
            }
        }
        Icon::Takes => {
            // A clapperboard.
            painter.rect_stroke(Rect::from_min_max(p(0.12, 0.40), p(0.88, 0.86)), 1.0, stroke, StrokeKind::Inside);
            poly(vec![p(0.12, 0.40), p(0.16, 0.18), p(0.88, 0.18), p(0.88, 0.40)], false);
            for x in [0.30, 0.50, 0.70] {
                line(p(x, 0.18), p(x - 0.06, 0.40));
            }
        }
        Icon::Structure => {
            // A table.
            painter.rect_stroke(Rect::from_min_max(p(0.10, 0.16), p(0.90, 0.84)), 1.0, stroke, StrokeKind::Inside);
            painter.rect_filled(Rect::from_min_max(p(0.10, 0.16), p(0.90, 0.32)), 0.0, color.gamma_multiply(0.5));
            for y in [0.50, 0.67] {
                painter.line_segment([p(0.10, y), p(0.90, y)], thin);
            }
            for x in [0.36, 0.63] {
                painter.line_segment([p(x, 0.16), p(x, 0.84)], thin);
            }
        }
        Icon::Assets => {
            // A box of tiles.
            for (x, y) in [(0.14, 0.14), (0.54, 0.14), (0.14, 0.54), (0.54, 0.54)] {
                painter.rect_stroke(Rect::from_min_max(p(x, y), p(x + 0.32, y + 0.32)), 2.0, stroke, StrokeKind::Inside);
            }
            painter.circle_filled(p(0.30, 0.30), side * 0.07, color);
            painter.rect_filled(Rect::from_min_max(p(0.62, 0.62), p(0.78, 0.78)), 0.0, color);
        }
        Icon::Console => {
            painter.rect_stroke(Rect::from_min_max(p(0.08, 0.16), p(0.92, 0.84)), 2.0, stroke, StrokeKind::Inside);
            poly(vec![p(0.22, 0.36), p(0.36, 0.50), p(0.22, 0.64)], false);
            line(p(0.44, 0.66), p(0.72, 0.66));
        }
    }
}

/// The icon of a light type.
pub fn for_light(kind: crate::light::LightKind) -> Icon {
    use crate::light::LightKind as L;
    match kind {
        L::Area => Icon::LightArea,
        L::Point => Icon::LightPoint,
        L::Spot => Icon::LightSpot,
        L::Omni => Icon::LightOmni,
        L::Infinite | L::Parallel => Icon::LightInfinite,
        L::Dome => Icon::LightDome,
        L::Sun => Icon::LightSun,
    }
}

/// The icon of a viewport render option.
pub fn for_view_fx(fx: crate::renderer::ViewFx) -> Icon {
    use crate::renderer::ViewFx;
    match fx {
        ViewFx::PostEffects => Icon::PostEffects,
        ViewFx::Tessellation => Icon::Tessellation,
        ViewFx::Shadows => Icon::Shadows,
        ViewFx::Reflections => Icon::Reflections,
        ViewFx::Ssao => Icon::Ssao,
        ViewFx::BackfaceCulling => Icon::BackfaceCulling,
    }
}

/// The icon representing a whole object in lists.
pub fn for_kind(kind: &crate::scene::ObjectKind) -> Icon {
    use crate::generator::GeneratorKind as G;
    use crate::scene::ObjectKind;
    match kind {
        ObjectKind::Null => Icon::Null,
        ObjectKind::Spline(_) => Icon::Spline,
        ObjectKind::Light(l) => for_light(l.kind),
        ObjectKind::Camera(_) => Icon::Null,
        ObjectKind::Sky(_) => Icon::LightDome,
        ObjectKind::Primitive(p) => for_primitive(p.kind()),
        ObjectKind::Generator(g) => match g.kind() {
            G::Subdivision => Icon::Subdivision,
            G::Array => Icon::Array,
            G::Symmetry => Icon::Symmetry,
            G::Boolean => Icon::Boolean,
        },
        ObjectKind::Polygon(_) => Icon::Polygon,
        ObjectKind::Deformer(d) => match d.kind() {
            crate::deformer::DeformerKind::Bend => Icon::Bend,
            crate::deformer::DeformerKind::Twist => Icon::Twist,
            crate::deformer::DeformerKind::Taper => Icon::Taper,
            crate::deformer::DeformerKind::Shear => Icon::Shear,
        },
    }
}

/// The icon representing an object type in lists.
pub fn for_primitive(kind: crate::primitive::PrimitiveKind) -> Icon {
    use crate::primitive::PrimitiveKind as K;
    match kind {
        K::Cube => Icon::Cube,
        K::Sphere => Icon::Sphere,
        K::Cylinder => Icon::Cylinder,
        K::Cone => Icon::Cone,
        K::Torus => Icon::Torus,
        K::Plane => Icon::Plane,
        K::Pyramid => Icon::Pyramid,
        K::Tube => Icon::Tube,
        K::Capsule => Icon::Capsule,
        K::Landscape => Icon::Landscape,
        K::Figure => Icon::Figure,
    }
}
