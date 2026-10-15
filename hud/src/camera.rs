//! A pinhole camera that only ever pulls back.
//!
//! World units are pixels of the finished frame: the beauty frame is the plane `z = 0`
//! spanning `0..1920 x 0..1080`, `+z` points away from the viewer, `y` points down like SVG.
//! Every later pipeline stage sits on planes closer to the viewer (more negative `z`), so as
//! the camera retreats each new layer arrives in front of the previous one.
use crate::timeline::{SceneLayer, TIMELINE, mix, smoothstep};
use serde::{Deserialize, Serialize};

/// Focal length in pixels: a plane `FOCAL` in front of the camera maps 1:1 onto the screen.
pub const FOCAL: f32 = 1400.0;
/// Nothing closer than this is drawn.
pub const NEAR: f32 = 60.0;
pub const CENTER: [f32; 2] = [960.0, 540.0];

pub const BEAUTY_Z: f32 = 0.0;
/// The command buffer pulls triangles off the frame up to this far toward the camera.
pub const COMMANDS_DEPTH: f32 = 520.0;
/// Tree level `d` floats at `TREE_Z0 - d * TREE_STEP`.
pub const TREE_Z0: f32 = -700.0;
pub const TREE_STEP: f32 = 170.0;
pub const SOURCE_Z: f32 = -1950.0;
pub const CURSOR_Z: f32 = -2900.0;

/// The camera's distance-defining keys `(second, z)`: a slow, never-stopping pull-back.
pub const DOLLY_KEYS: [(f32, f32); 6] = [
    (0.0, -1400.0),
    (3.0, -1640.0),
    (5.5, -2350.0),
    (8.0, -3250.0),
    (10.5, -4050.0),
    (12.0, -4400.0),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
}

/// A point after projection.
#[derive(Clone, Copy, Debug)]
pub struct Projected {
    pub x: f32,
    pub y: f32,
    /// Screen pixels per world unit at this depth.
    pub scale: f32,
    /// Distance from the camera along the view axis.
    pub distance: f32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Camera {
    pub t: f32,
    pub position: Vec3,
    pub roll: f32,
    /// Distance in focus.
    pub focus: f32,
    /// Lens opening; 0 = pinhole (everything sharp).
    pub aperture: f32,
    /// Chromatic aberration in screen pixels at the frame edge.
    pub aberration: f32,
    /// Film grain amplitude 0..1.
    pub grain: f32,
}

impl Camera {
    /// The camera rig at second `t` of the film.
    pub fn at(t: f32) -> Self {
        let t = t.clamp(0.0, TIMELINE.duration());
        // Lateral drift dies out before the end so the cursor lands dead centre.
        let settle = 1.0 - smoothstep(9.6, 11.2, t);
        let tau = std::f32::consts::TAU;
        let position = Vec3::new(
            CENTER[0] + 70.0 * (tau * t / 14.0).sin() * settle,
            CENTER[1] - 38.0 * (tau * t / 17.0 + 0.6).sin() * settle,
            dolly_z(t),
        );

        let w = TIMELINE.weights(t);
        let planes = layer_planes();
        let total: f32 = w.iter().sum::<f32>().max(1e-4);
        let focus_z = w.iter().zip(planes).map(|(w, z)| w * z).sum::<f32>() / total;

        let depth = TIMELINE.depth(t);
        // Depth of field opens up the deeper we dig, then closes for the clean cursor.
        let aperture = mix(0.0, 34.0, smoothstep(0.8, 4.0, depth)) * (1.0 - smoothstep(10.6, 11.0, t));
        let end = smoothstep(10.55, 11.0, t);
        let ramp = (t / TIMELINE.marks[4]).clamp(0.0, 1.0);
        Self {
            t,
            position,
            roll: 0.006 * (tau * t / 19.0).sin() * settle,
            focus: (focus_z - position.z).max(NEAR),
            aperture,
            aberration: mix(0.6, 5.0, ramp * ramp) * (1.0 - end),
            grain: mix(0.025, 0.12, ramp.powf(1.5)) * (1.0 - end),
        }
    }

    pub fn project(&self, p: Vec3) -> Option<Projected> {
        let distance = p.z - self.position.z;
        if distance < NEAR {
            return None;
        }
        let scale = FOCAL / distance;
        let (dx, dy) = ((p.x - self.position.x) * scale, (p.y - self.position.y) * scale);
        let (s, c) = self.roll.sin_cos();
        Some(Projected {
            x: CENTER[0] + dx * c - dy * s,
            y: CENTER[1] + dx * s + dy * c,
            scale,
            distance,
        })
    }

    /// Circle of confusion in screen pixels for something at `distance`.
    pub fn blur(&self, distance: f32) -> f32 {
        let defocus = (distance - self.focus).abs() / distance.max(NEAR);
        (self.aperture * defocus * FOCAL / self.focus).min(28.0)
    }

    /// An SVG transform placing a camera-facing plane at depth `z`, whose own
    /// coordinates are world units with the origin at world `(ox, oy)`.
    pub fn plane_matrix(&self, ox: f32, oy: f32, z: f32) -> Option<[f32; 6]> {
        let p = self.project(Vec3::new(ox, oy, z))?;
        let (s, c) = self.roll.sin_cos();
        Some([c * p.scale, s * p.scale, -s * p.scale, c * p.scale, p.x, p.y])
    }
}

/// The plane each layer lives on (used for focus pulling).
pub fn layer_planes() -> [f32; 5] {
    [
        BEAUTY_Z,
        BEAUTY_Z - COMMANDS_DEPTH * 0.5,
        TREE_Z0 - TREE_STEP * 1.5,
        SOURCE_Z,
        CURSOR_Z,
    ]
}

pub fn plane_z(layer: SceneLayer) -> f32 {
    layer_planes()[layer.index()]
}

/// Monotone cubic (Fritsch-Carlson) through `DOLLY_KEYS`: velocity never jumps and the
/// camera never creeps forward, so there is no visible key at any transition.
pub fn dolly_z(t: f32) -> f32 {
    let k = &DOLLY_KEYS;
    let n = k.len();
    let slopes: Vec<f32> = (0..n - 1).map(|i| (k[i + 1].1 - k[i].1) / (k[i + 1].0 - k[i].0)).collect();
    let mut m = vec![0.0; n];
    m[0] = slopes[0];
    m[n - 1] = slopes[n - 2];
    for i in 1..n - 1 {
        m[i] = if slopes[i - 1] * slopes[i] <= 0.0 {
            0.0
        } else {
            // Weighted harmonic mean of the neighbouring slopes (Brodlie).
            let (a, b) = (slopes[i - 1], slopes[i]);
            let (h0, h1) = (k[i].0 - k[i - 1].0, k[i + 1].0 - k[i].0);
            let (w1, w2) = (2.0 * h1 + h0, h1 + 2.0 * h0);
            (w1 + w2) / (w1 / a + w2 / b)
        };
    }
    let i = (0..n - 1).find(|&i| t <= k[i + 1].0).unwrap_or(n - 2);
    let h = k[i + 1].0 - k[i].0;
    let s = ((t - k[i].0) / h).clamp(0.0, 1.0);
    let (s2, s3) = (s * s, s * s * s);
    (2.0 * s3 - 3.0 * s2 + 1.0) * k[i].1
        + (s3 - 2.0 * s2 + s) * h * m[i]
        + (-2.0 * s3 + 3.0 * s2) * k[i + 1].1
        + (s3 - s2) * h * m[i + 1]
}
