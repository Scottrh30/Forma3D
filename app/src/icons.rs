//! Stroke icons (24×24 SVG paths), drawn in the current text colour.

use leptos::prelude::*;

pub fn path(name: &str) -> &'static str {
    match name {
        "undo" => r#"<path d="M9 14 4 9l5-5"/><path d="M4 9h10a6 6 0 0 1 0 12h-3"/>"#,
        "redo" => r#"<path d="m15 14 5-5-5-5"/><path d="M20 9H10a6 6 0 0 0 0 12h3"/>"#,
        "select" => r#"<path d="M5 3l14 8-6 2-3 6z"/>"#,
        "move" => r#"<path d="M12 2v20M2 12h20M12 2l-3 3m3-3 3 3M12 22l-3-3m3 3 3-3M2 12l3-3m-3 3 3 3M22 12l-3-3m3 3-3 3"/>"#,
        "scale" => r#"<rect x="3" y="11" width="10" height="10" rx="1"/><path d="M14 10l7-7m-5 0h5v5"/>"#,
        "rotate" => r#"<path d="M20 12a8 8 0 1 1-2.4-5.7"/><path d="M20 4v5h-5"/>"#,
        "world" => r#"<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c3 3 3 15 0 18M12 3c-3 3-3 15 0 18"/>"#,
        "local" => r#"<path d="M12 3 20 7.5v9L12 21 4 16.5v-9z"/><path d="M12 12V7M12 12l4 2M12 12l-4 2"/>"#,
        "cube" => r#"<path d="M12 3 20 7.5v9L12 21 4 16.5v-9z"/><path d="M4 7.5 12 12l8-4.5M12 12v9"/>"#,
        "sphere" => r#"<circle cx="12" cy="12" r="8"/><ellipse cx="12" cy="12" rx="8" ry="3.2"/><ellipse cx="12" cy="12" rx="3.2" ry="8"/>"#,
        "cylinder" => r#"<ellipse cx="12" cy="6" rx="7" ry="2.5"/><path d="M5 6v12c0 1.4 3.1 2.5 7 2.5s7-1.1 7-2.5V6"/>"#,
        "cone" => r#"<path d="M12 3 5 18M12 3l7 15"/><ellipse cx="12" cy="18" rx="7" ry="2.5"/>"#,
        "torus" => r#"<ellipse cx="12" cy="12" rx="9" ry="5.5"/><ellipse cx="12" cy="11.5" rx="3.6" ry="1.6"/>"#,
        "plane" => r#"<path d="M3 15 12 19l9-5-9-4z"/>"#,
        "pyramid" => r#"<path d="M12 3 3 18l9 3 9-3z"/><path d="M12 3v18"/>"#,
        "tube" => r#"<ellipse cx="12" cy="6" rx="7" ry="2.5"/><ellipse cx="12" cy="6" rx="3" ry="1"/><path d="M5 6v12c0 1.4 3.1 2.5 7 2.5s7-1.1 7-2.5V6"/>"#,
        "capsule" => r#"<rect x="7" y="3" width="10" height="18" rx="5"/><path d="M7 9c3 1.5 7 1.5 10 0"/>"#,
        "landscape" => r#"<path d="M2 19 8 9l4 6 3-4 7 8z"/><circle cx="17" cy="6" r="1.8"/>"#,
        "figure" => r#"<circle cx="12" cy="4.5" r="2"/><path d="M12 7v7M7 9.5h10M12 14l-3 7M12 14l3 7"/>"#,
        "null" => r#"<circle cx="12" cy="12" r="7" stroke-dasharray="3 2.5"/><path d="M12 9v6M9 12h6"/>"#,
        "spline" => r#"<path d="M3 18c4-12 8 4 11-6s6-6 7-7"/><circle cx="3" cy="18" r="1.4"/><circle cx="21" cy="5" r="1.4"/>"#,
        "pen" => r#"<path d="M12 19 19 12l3 3-7 7z"/><path d="M18 13 16.5 5.5 2 2l3.5 14.5L13 18z"/><circle cx="11" cy="11" r="2"/>"#,
        "polygon" => r#"<path d="M5 6 18 5 19 17 8 18z"/><circle cx="5" cy="6" r="1.4"/><circle cx="18" cy="5" r="1.4"/><circle cx="19" cy="17" r="1.4"/><circle cx="8" cy="18" r="1.4"/>"#,
        "light" => r#"<circle cx="12" cy="10" r="5"/><path d="M9.5 17h5M10 20.5h4M12 2v1"/>"#,
        "camera" => r#"<rect x="3" y="7" width="13" height="10" rx="2"/><path d="m16 11 5-3v8l-5-3"/>"#,
        "deformer" => r#"<path d="M6 3c6 4 6 14 0 18M18 3c-6 4-6 14 0 18"/>"#,
        "generator" => r#"<rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/>"#,
        "material" => r#"<circle cx="12" cy="12" r="8"/><path d="M8 9a5 5 0 0 1 4-2"/>"#,
        "model" => r#"<path d="M12 3 20 7.5v9L12 21 4 16.5v-9z"/><circle cx="12" cy="12" r="2" fill="currentColor"/>"#,
        "object" => r#"<path d="M12 3 20 7.5v9L12 21 4 16.5v-9z"/><path d="M4 7.5 12 12l8-4.5"/><path d="M12 12v9"/><circle cx="12" cy="12" r="1.6" fill="currentColor"/>"#,
        "points" => r#"<circle cx="5" cy="6" r="1.8"/><circle cx="18" cy="5" r="1.8"/><circle cx="8" cy="18" r="1.8"/><circle cx="19" cy="17" r="1.8"/>"#,
        "edges" => r#"<path d="M5 6 18 5 19 17 8 18z"/>"#,
        "polys" => r#"<path d="M5 6 18 5 19 17 8 18z" fill="currentColor" fill-opacity=".3"/>"#,
        "workplane" => r#"<path d="M3 15 12 19l9-5-9-4z"/><path d="M12 14V3"/><path d="m9 6 3-3 3 3"/>"#,
        "editable" => r#"<path d="M12 3 20 7.5v9L12 21 4 16.5v-9z"/><circle cx="4" cy="7.5" r="1.4" fill="currentColor"/><circle cx="20" cy="7.5" r="1.4" fill="currentColor"/><circle cx="12" cy="21" r="1.4" fill="currentColor"/>"#,
        "frame" => r#"<path d="M4 8V4h4M16 4h4v4M20 16v4h-4M8 20H4v-4"/>"#,
        "frame-sel" => r#"<path d="M4 8V4h4M16 4h4v4M20 16v4h-4M8 20H4v-4"/><rect x="9" y="9" width="6" height="6" rx="1"/>"#,
        "shade" => r#"<circle cx="12" cy="12" r="8" fill="currentColor" fill-opacity=".35"/>"#,
        "shade-lines" => r#"<circle cx="12" cy="12" r="8" fill="currentColor" fill-opacity=".25"/><path d="M4 12h16M12 4v16"/>"#,
        "wire" => r#"<circle cx="12" cy="12" r="8"/><path d="M4 12h16M12 4v16"/>"#,
        "grid" => r#"<path d="M3 9h18M3 15h18M9 3v18M15 3v18"/>"#,
        "sun" => r#"<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M2 12h2M20 12h2M5 5l1.5 1.5M17.5 17.5 19 19M5 19l1.5-1.5M17.5 6.5 19 5"/>"#,
        "orbit" => r#"<circle cx="12" cy="12" r="3"/><ellipse cx="12" cy="12" rx="10" ry="4"/>"#,
        "pan" => r#"<path d="M12 2v20M2 12h20M12 2l-3 3m3-3 3 3M12 22l-3-3m3 3 3-3M2 12l3-3m-3 3 3 3M22 12l-3-3m3 3-3 3"/>"#,
        "zoom" => r#"<circle cx="11" cy="11" r="7"/><path d="m20 20-4-4M8 11h6M11 8v6"/>"#,
        "first" => r#"<path d="M6 5v14M18 5l-9 7 9 7z"/>"#,
        "prev" => r#"<path d="M15 5l-7 7 7 7"/>"#,
        "back" => r#"<path d="M17 5 7 12l10 7z"/>"#,
        "play" => r#"<path d="M7 5l11 7-11 7z" fill="currentColor"/>"#,
        "pause" => r#"<path d="M8 5v14M16 5v14"/>"#,
        "next" => r#"<path d="M9 5l7 7-7 7"/>"#,
        "last" => r#"<path d="M18 5v14M6 5l9 7-9 7z"/>"#,
        "eye" => r#"<path d="M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12z"/><circle cx="12" cy="12" r="3"/>"#,
        "trash" => r#"<path d="M4 7h16M10 11v6M14 11v6M5 7l1 13h12l1-13M9 7V4h6v3"/>"#,
        "copy" => r#"<rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V5a1 1 0 0 0-1-1H5a1 1 0 0 0-1 1v10a1 1 0 0 0 1 1h3"/>"#,
        "paste" => r#"<rect x="5" y="4" width="14" height="17" rx="2"/><path d="M9 4V3h6v1M9 10h6M9 14h6"/>"#,
        "cut" => r#"<circle cx="6" cy="18" r="3"/><circle cx="18" cy="18" r="3"/><path d="M8 16 19 4M16 16 5 4"/>"#,
        "duplicate" => r#"<rect x="3" y="3" width="12" height="12" rx="2"/><rect x="9" y="9" width="12" height="12" rx="2"/>"#,
        "group" => r#"<rect x="3" y="3" width="18" height="18" rx="3" stroke-dasharray="3 2"/><rect x="7" y="7" width="5" height="5" rx="1"/><rect x="12" y="12" width="5" height="5" rx="1"/>"#,
        "new" => r#"<path d="M6 3h8l5 5v13H6z"/><path d="M14 3v5h5M12 11v6M9 14h6"/>"#,
        "open" => r#"<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v1H8l-3 9H3z"/><path d="M5 19 8 10h14l-3 9z"/>"#,
        "save" => r#"<path d="M5 3h11l4 4v14H5z"/><path d="M8 3v5h7V3M8 21v-7h8v7"/>"#,
        "search" => r#"<circle cx="11" cy="11" r="7"/><path d="m20 20-4-4"/>"#,
        "console" => r#"<rect x="3" y="4" width="18" height="16" rx="2"/><path d="m7 9 3 3-3 3M13 15h4"/>"#,
        "assets" => r#"<rect x="3" y="3" width="8" height="8" rx="2"/><rect x="13" y="3" width="8" height="8" rx="2"/><rect x="3" y="13" width="8" height="8" rx="2"/><rect x="13" y="13" width="8" height="8" rx="2"/>"#,
        "check" => r#"<path d="m5 12 5 5 9-10"/>"#,
        "chev-right" => r#"<path d="m9 6 6 6-6 6"/>"#,
        "chev-down" => r#"<path d="m6 9 6 6 6-6"/>"#,
        "layout" => r#"<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M12 3v18M3 12h18"/>"#,
        "info" => r#"<circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7.5v.5"/>"#,
        "power" => r#"<path d="M12 3v8"/><path d="M6.3 7a8 8 0 1 0 11.4 0"/>"#,
        _ => r#"<circle cx="12" cy="12" r="7"/>"#,
    }
}

/// An inline SVG icon.
pub fn icon(name: &'static str) -> impl IntoView {
    view! { <svg class="i" viewBox="0 0 24 24" inner_html=path(name)></svg> }
}

/// A smaller inline SVG icon.
pub fn icon_s(name: &'static str) -> impl IntoView {
    view! { <svg class="i s" viewBox="0 0 24 24" inner_html=path(name)></svg> }
}

/// The icon for an object kind.
pub fn for_kind(kind: &forma3d_core::scene::ObjectKind) -> &'static str {
    use forma3d_core::primitive::PrimitiveKind as P;
    use forma3d_core::scene::ObjectKind as K;
    match kind {
        K::Null => "null",
        K::Primitive(p) => match p.kind() {
            P::Cube => "cube",
            P::Sphere => "sphere",
            P::Cylinder => "cylinder",
            P::Cone => "cone",
            P::Torus => "torus",
            P::Plane => "plane",
            P::Pyramid => "pyramid",
            P::Tube => "tube",
            P::Capsule => "capsule",
            P::Landscape => "landscape",
            P::Figure => "figure",
        },
        K::Generator(_) => "generator",
        K::Deformer(_) => "deformer",
        K::Polygon(_) => "polygon",
        K::Spline(_) => "spline",
        K::Light(_) => "light",
    }
}

pub fn for_primitive(k: forma3d_core::primitive::PrimitiveKind) -> &'static str {
    for_kind(&forma3d_core::scene::ObjectKind::Primitive(k.default_primitive()))
}
