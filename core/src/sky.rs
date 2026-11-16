//! Sky objects (Light > Sky, Physical Sky): the environment around the
//! scene. A Sky is a colour gradient from horizon to zenith; a Physical Sky
//! is daylight from the sun's position at a place, date and time, and brings
//! the sun with it. Both light the scene and show behind it in renders.

use crate::mesh::{Mesh, Vertex};
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::f32::consts::{PI, TAU};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SkyKind {
    /// A colour gradient.
    Gradient,
    /// Daylight for a place and time, with a sun.
    Physical,
}

impl SkyKind {
    pub const ALL: [SkyKind; 2] = [SkyKind::Gradient, SkyKind::Physical];

    pub fn name(self) -> &'static str {
        match self {
            SkyKind::Gradient => "Sky",
            SkyKind::Physical => "Physical Sky",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            SkyKind::Gradient => "sky",
            SkyKind::Physical => "physical_sky",
        }
    }

    pub fn keywords(self) -> &'static str {
        match self {
            SkyKind::Gradient => "sky environment background gradient horizon",
            SkyKind::Physical => "sky daylight sun time of day weather atmosphere",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sky {
    pub kind: SkyKind,
    /// How strongly the sky lights the scene (percent).
    pub intensity: f32,
    /// Gradient sky colours.
    pub zenith_color: [f32; 3],
    pub horizon_color: [f32; 3],
    pub ground_color: [f32; 3],
    /// Physical sky: place (degrees), day of the year (1-365) and time (hours).
    pub latitude: f32,
    pub day_of_year: f32,
    pub time_of_day: f32,
    /// Haze: 2 is a clear day, 10 is hazy.
    pub turbidity: f32,
    /// The physical sky's sun lights the scene and casts shadows.
    pub sun: bool,
    pub sun_intensity: f32,
    /// Which way north is (degrees around Y; 0 = +Z).
    pub north: f32,
}

impl Default for Sky {
    fn default() -> Self {
        Sky {
            kind: SkyKind::Gradient,
            intensity: 100.0,
            zenith_color: [0.32, 0.50, 0.85],
            horizon_color: [0.80, 0.86, 0.94],
            ground_color: [0.30, 0.28, 0.26],
            latitude: 45.0,
            day_of_year: 172.0,
            time_of_day: 14.0,
            turbidity: 3.0,
            sun: true,
            sun_intensity: 100.0,
            north: 0.0,
        }
    }
}

impl Sky {
    pub fn new(kind: SkyKind) -> Self {
        Sky { kind, ..Sky::default() }
    }

    pub fn sanitized(&self) -> Sky {
        let f = |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        let c = |a: [f32; 3]| a.map(|x| f(x, 0.0, 1.0, 0.5));
        Sky {
            kind: self.kind,
            intensity: f(self.intensity, 0.0, 10_000.0, 100.0),
            zenith_color: c(self.zenith_color),
            horizon_color: c(self.horizon_color),
            ground_color: c(self.ground_color),
            latitude: f(self.latitude, -90.0, 90.0, 45.0),
            day_of_year: f(self.day_of_year, 1.0, 365.0, 172.0),
            time_of_day: f(self.time_of_day, 0.0, 24.0, 14.0),
            turbidity: f(self.turbidity, 1.5, 12.0, 3.0),
            sun: self.sun,
            sun_intensity: f(self.sun_intensity, 0.0, 10_000.0, 100.0),
            north: f(self.north, -360.0, 360.0, 0.0),
        }
    }

    pub fn hidden_fields(&self) -> &'static [&'static str] {
        match self.kind {
            SkyKind::Gradient => &["latitude", "day_of_year", "time_of_day", "turbidity", "sun", "sun_intensity", "north"],
            SkyKind::Physical => &["zenith_color", "horizon_color", "ground_color"],
        }
    }

    /// Unit vector towards the sun (physical sky), from a simple solar
    /// position model: declination by date, hour angle by time.
    pub fn sun_direction(&self) -> Vec3 {
        let s = self.sanitized();
        let decl = (23.44f32).to_radians() * (TAU * (284.0 + s.day_of_year) / 365.0).sin();
        let lat = s.latitude.to_radians();
        let hour = (s.time_of_day - 12.0) * 15f32.to_radians();
        let alt = (lat.sin() * decl.sin() + lat.cos() * decl.cos() * hour.cos()).clamp(-1.0, 1.0).asin();
        // Azimuth from north, clockwise (east = 90°).
        let az = {
            let y = -hour.sin() * decl.cos();
            let x = decl.sin() * lat.cos() - decl.cos() * lat.sin() * hour.cos();
            y.atan2(x)
        };
        // North is +Z (turned by `north`), east is +X.
        let a = az + s.north.to_radians();
        Vec3::new(a.sin() * alt.cos(), alt.sin(), a.cos() * alt.cos()).normalize_or(Vec3::Y)
    }

    /// Light from the sky in direction `dir` (linear RGB, before intensity).
    pub fn radiance(&self, dir: Vec3) -> Vec3 {
        let d = dir.normalize_or(Vec3::Y);
        match self.kind {
            SkyKind::Gradient => {
                let z = Vec3::from(self.zenith_color);
                let h = Vec3::from(self.horizon_color);
                let g = Vec3::from(self.ground_color);
                if d.y >= 0.0 { h.lerp(z, d.y.sqrt()) } else { h.lerp(g, (-d.y * 4.0).min(1.0)) }
            }
            SkyKind::Physical => {
                let sun = self.sun_direction();
                let elev = sun.y;
                // Daylight fades towards dusk and is near black at night.
                let day = ((elev + 0.08) * 6.0).clamp(0.0, 1.0);
                let haze = ((self.turbidity - 1.5) / 10.5).clamp(0.0, 1.0);
                let zenith = Vec3::new(0.18, 0.36, 0.85).lerp(Vec3::new(0.55, 0.62, 0.75), haze);
                let warm = (1.0 - elev.max(0.0)).powf(6.0);
                let horizon = Vec3::new(0.85, 0.88, 0.95).lerp(Vec3::new(1.0, 0.62, 0.38), warm * 0.8);
                let ground = Vec3::new(0.28, 0.26, 0.24);
                let base = if d.y >= 0.0 { horizon.lerp(zenith, d.y.powf(0.45)) } else { horizon.lerp(ground, (-d.y * 4.0).min(1.0)) };
                // Glow around the sun.
                let cos = d.dot(sun).max(0.0);
                let glow = Vec3::new(1.0, 0.85, 0.6) * (cos.powf(64.0) * 2.0 + cos.powf(6.0) * 0.25 * (1.0 + haze));
                let night = Vec3::new(0.01, 0.012, 0.025);
                night.lerp(base + glow, day)
            }
        }
    }

    /// The average light from the sky above (for ambient lighting).
    pub fn ambient(&self) -> Vec3 {
        let mut sum = Vec3::ZERO;
        let n = 24;
        for k in 0..n {
            let a = TAU * k as f32 / n as f32;
            for e in [0.15f32, 0.5, 0.85] {
                let el = e * PI * 0.5;
                sum += self.radiance(Vec3::new(a.cos() * el.cos(), el.sin(), a.sin() * el.cos()));
            }
        }
        sum / (n * 3) as f32
    }

    /// The physical sky's sun colour (white at noon, orange low down).
    pub fn sun_color(&self) -> Vec3 {
        Vec3::from(crate::light::Light::sun_color(-self.sun_direction()))
    }

    /// A wire dome for the viewport.
    pub fn gizmo(&self) -> Mesh {
        let r = 70.0;
        let mut lines = Vec::new();
        for ring in 0..3 {
            let el = ring as f32 * PI / 7.0;
            let n = 32;
            for k in 0..n {
                let a0 = TAU * k as f32 / n as f32;
                let a1 = TAU * (k + 1) as f32 / n as f32;
                let p = |a: f32| Vec3::new(a.cos() * el.cos(), el.sin(), a.sin() * el.cos()) * r;
                lines.push((p(a0), p(a1)));
            }
        }
        for k in 0..6 {
            let a = PI * k as f32 / 6.0;
            for j in 0..12 {
                let t0 = PI * j as f32 / 12.0;
                let t1 = PI * (j + 1) as f32 / 12.0;
                let p = |t: f32| Vec3::new(a.cos() * t.cos(), t.sin(), a.sin() * t.cos()) * r;
                lines.push((p(t0), p(t1)));
            }
        }
        if self.kind == SkyKind::Physical {
            let s = self.sun_direction() * r;
            lines.push((Vec3::ZERO, s));
            for k in 0..8 {
                let a = TAU * k as f32 / 8.0;
                lines.push((s, s + Vec3::new(a.cos(), a.sin() * 0.3, a.sin()) * 8.0));
            }
        }
        let mut vertices = Vec::new();
        let mut edges = Vec::new();
        for (a, b) in lines {
            for p in [a, b] {
                edges.push(vertices.len() as u32);
                vertices.push(Vertex { pos: p.to_array(), normal: [0.0, 1.0, 0.0] });
            }
        }
        Mesh::from_parts(vertices, Vec::new(), edges)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noon_sun_is_high_midnight_below() {
        let mut s = Sky::new(SkyKind::Physical);
        s.time_of_day = 12.0;
        s.latitude = 0.0;
        s.day_of_year = 80.0;
        assert!(s.sun_direction().y > 0.95, "{:?}", s.sun_direction());
        s.time_of_day = 0.0;
        assert!(s.sun_direction().y < -0.9);
        assert!(s.radiance(Vec3::Y).length() < 0.1, "night is dark");
        s.time_of_day = 9.0;
        assert!(s.sun_direction().x > 0.3, "morning sun in the east (+X)");
    }

    #[test]
    fn gradient_sky_blends_colours() {
        let s = Sky::default();
        assert!((s.radiance(Vec3::Y) - Vec3::from(s.zenith_color)).length() < 1e-4);
        assert!((s.radiance(Vec3::X) - Vec3::from(s.horizon_color)).length() < 1e-4);
        assert!(s.ambient().length() > 0.3);
        let back: Sky = serde_json::from_str(r#"{"kind":"Physical"}"#).unwrap();
        assert_eq!(back.kind, SkyKind::Physical);
    }
}
