//! One record per element of the beauty pass's SVG tree.
use serde::{Deserialize, Serialize};

/// Axis-aligned bounding box in frame pixels (after all transforms).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BBox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl BBox {
    pub fn from_points(points: impl IntoIterator<Item = [f32; 2]>) -> Self {
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for [x, y] in points {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        if x0 > x1 {
            return Self::default();
        }
        Self { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
    }

    pub fn union(self, other: BBox) -> BBox {
        if self.w <= 0.0 && self.h <= 0.0 {
            return other;
        }
        if other.w <= 0.0 && other.h <= 0.0 {
            return self;
        }
        BBox::from_points([
            [self.x, self.y],
            [self.x + self.w, self.y + self.h],
            [other.x, other.y],
            [other.x + other.w, other.y + other.h],
        ])
    }

    pub fn center(&self) -> [f32; 2] {
        [self.x + self.w * 0.5, self.y + self.h * 0.5]
    }
}

/// A 2-D affine matrix in SVG order `matrix(a b c d e f)`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Matrix(pub [f32; 6]);

impl Matrix {
    pub const IDENTITY: Matrix = Matrix([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    pub const fn translate(x: f32, y: f32) -> Self {
        Matrix([1.0, 0.0, 0.0, 1.0, x, y])
    }

    pub const fn scale(sx: f32, sy: f32) -> Self {
        Matrix([sx, 0.0, 0.0, sy, 0.0, 0.0])
    }

    /// `self` applied after `inner`: `self * inner`.
    pub fn then(self, inner: Matrix) -> Matrix {
        let [a, b, c, d, e, f] = self.0;
        let [a2, b2, c2, d2, e2, f2] = inner.0;
        Matrix([
            a * a2 + c * b2,
            b * a2 + d * b2,
            a * c2 + c * d2,
            b * c2 + d * d2,
            a * e2 + c * f2 + e,
            b * e2 + d * f2 + f,
        ])
    }

    pub fn apply(&self, [x, y]: [f32; 2]) -> [f32; 2] {
        let [a, b, c, d, e, f] = self.0;
        [a * x + c * y + e, b * x + d * y + f]
    }

    pub fn is_identity(&self) -> bool {
        *self == Matrix::IDENTITY
    }

    /// `matrix(1 0 0 -0.42 0 1471.2)` with the values actually used by the renderer.
    pub fn to_svg(&self) -> String {
        let fmt = |v: f32| {
            let s = format!("{v:.4}");
            let s = s.trim_end_matches('0').trim_end_matches('.');
            if s == "-0" { "0".to_owned() } else { s.to_owned() }
        };
        let v = self.0.map(fmt);
        format!("matrix({} {} {} {} {} {})", v[0], v[1], v[2], v[3], v[4], v[5])
    }
}

/// Everything the later passes and the synthesizer need to know about one SVG element.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeRecord {
    /// Document order index (also the draw-call order).
    pub id: u32,
    pub parent: Option<u32>,
    /// `g`, `path`, `rect`, `circle`, ...
    pub tag: String,
    /// The element's `id` attribute.
    pub name: String,
    /// Bounding box in frame pixels.
    pub bbox: BBox,
    /// Accumulated transform from the root to this element.
    pub transform: Matrix,
    /// FNV-1a 64 of the element's path data (or its geometry attributes).
    pub path_hash: u64,
    /// 0 for the root `<svg>`.
    pub depth: u32,
    /// Main paint, `#rrggbb` or `url(#id)`.
    pub paint: String,
    /// Number of path segments: the size of the draw call.
    pub segments: u32,
}

impl NodeRecord {
    pub fn short_hash(&self) -> String {
        format!("{:08x}", (self.path_hash >> 32) as u32)
    }

    pub fn is_drawable(&self) -> bool {
        self.tag != "g" && self.tag != "svg"
    }
}

/// FNV-1a 64: stable across runs and platforms.
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}
