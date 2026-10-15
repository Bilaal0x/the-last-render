//! The finished frame: a neon city on a black horizon.
//! Every element is plain data first, so the later passes can take it apart.
use fframes::{Svgr, Transform, svgr};
use hud::Matrix;

pub const HORIZON: f32 = 640.0;

pub enum Shape {
    Group,
    Polygon(Vec<[f32; 2]>),
    Rects(Vec<[f32; 4]>),
    Lines(Vec<[f32; 4]>),
    Circle { cx: f32, cy: f32, r: f32 },
}

pub struct Node {
    pub id: String,
    pub shape: Shape,
    pub transform: Matrix,
    pub paint: &'static str,
    pub stroke: Option<(&'static str, f32)>,
    pub opacity: f32,
    pub glow: bool,
    pub flicker: Option<f32>,
    pub drift: f32,
    pub children: Vec<Node>,
}

// the-last-render:source-begin
pub fn render_node<'a>(node: &'a Node, t: f32) -> Svgr<'a> {
    let opacity = node.opacity * node.flicker.map_or(1.0, |s| flicker(s, t));
    let filter = if node.glow { "url(#glow)" } else { "none" };
    let (stroke, width) = node.stroke.unwrap_or(("none", 0.0));
    let travel = (t * node.drift).rem_euclid(2400.0) - 240.0;
    let shift = if node.drift == 0.0 { 0.0 } else { travel };
    let id = node.id.as_str();
    match &node.shape {
        Shape::Group => {
            let children: Vec<Svgr> =
                node.children.iter().map(|c| render_node(c, t)).collect();
            svgr!(<g id={id} transform={node.transform.to_svg()}
                opacity={opacity}>{children}</g>)
        }
        Shape::Circle { cx, cy, r } => svgr!(
            <circle id={id} cx={*cx} cy={*cy} r={*r} fill={node.paint}
                stroke={stroke} stroke-width={width}
                opacity={opacity} filter={filter} />
        ),
        _ => svgr!(
            <path id={id} d={node.path_data()} fill={node.paint}
                transform={Transform::translate(shift, 0.0)}
                stroke={stroke} stroke-width={width}
                opacity={opacity} filter={filter} />
        ),
    }
}

pub fn tower(i: usize, x: f32, w: f32, h: f32, near: bool) -> Node {
    let top = match i % 4 {
        0 => vec![[0.0, -h], [w, -h]],
        1 => vec![[0.0, -h * 0.9], [w * 0.5, -h], [w, -h * 0.9]],
        2 => vec![
            [0.0, -h * 0.86], [w * 0.2, -h * 0.86], [w * 0.2, -h],
            [w * 0.8, -h], [w * 0.8, -h * 0.86], [w, -h * 0.86],
        ],
        _ => vec![[0.0, -h * 0.94], [w, -h]],
    };
    let outline = [vec![[0.0, 0.0]], top, vec![[w, 0.0]]].concat();
    let body = Node::shape(format!("tower-{i:02}/body"), Shape::Polygon(outline))
        .paint(if near { "#03040b" } else { "#0a0c1e" })
        .stroke(if near { "#0f4a63" } else { "#1a1640" }, 1.5);
    let mut parts = vec![body];
    if near {
        parts.push(windows(i, w, h));
    }
    Node::group(format!("tower-{i:02}"), Matrix::translate(x, HORIZON)).with(parts)
}
// the-last-render:source-end

pub fn windows(i: usize, w: f32, h: f32) -> Node {
    let (cols, rows) = (((w - 16.0) / 13.0) as usize, ((h * 0.8 - 20.0) / 21.0) as usize);
    let lit = (0..cols * rows)
        .filter(|k| rand(i * 31 + k / cols, 4) > 0.45 && rand(i * 997 + k, 3) > 0.35)
        .map(|k| [10.0 + (k % cols) as f32 * 13.0, -h * 0.82 + 14.0 + (k / cols) as f32 * 21.0, 5.0, 8.0])
        .collect();
    let paint = ["#19f0ff", "#19f0ff", "#ff4fd8", "#ffffff"][i % 4];
    Node::shape(format!("tower-{i:02}/windows"), Shape::Rects(lit))
        .paint(paint)
        .glow()
        .flicker(i as f32 * 1.37)
}

pub fn skyline(id: &str, seed: usize, count: usize, near: bool) -> Node {
    let mut x: f32 = if near { -30.0 } else { 20.0 };
    let mut towers = Vec::new();
    for i in seed..seed + count {
        if x > 1920.0 {
            break;
        }
        let w = if near { 95.0 + rand(i, 1) * 80.0 } else { 70.0 + rand(i, 1) * 70.0 };
        let h = if near { 230.0 + rand(i, 2) * 260.0 } else { (120.0 + rand(i, 2) * 190.0) * (0.35 + 0.65 * ((x - 960.0) / 960.0).abs()) };
        towers.push(tower(i, x, w, h, near));
        // The near row parts around the sun.
        let gap = if near && (520.0..1100.0).contains(&(x + w)) { 1360.0 - x - w } else { rand(i, 5) * 40.0 };
        x += w + gap;
    }
    let fit = if near { Matrix::IDENTITY } else { Matrix([0.92, 0.0, 0.0, 0.92, 76.8, 51.2]) };
    Node::group(id.to_owned(), fit).with(towers)
}

pub fn floor_grid() -> Node {
    let vanish = [960.0, HORIZON];
    let rays = (0..=24).map(|k| {
        let x = -1400.0 + k as f32 * 197.0;
        [vanish[0], vanish[1], x, 1080.0]
    });
    let rungs = (1..=9).map(|k| {
        let y = HORIZON + 440.0 * (k as f32 / 9.0).powf(2.2);
        [0.0, y, 1920.0, y]
    });
    Node::shape("floor/grid".into(), Shape::Lines(rays.chain(rungs).collect()))
        .paint("none")
        .stroke("#19f0ff", 1.4)
        .opacity(0.42)
        .glow()
}

pub fn city() -> Node {
    let reflection = Matrix([1.0, 0.0, 0.0, -0.45, 0.0, HORIZON * 1.45]);
    Node::group("frame".into(), Matrix::IDENTITY).with(vec![
        Node::group("sky".into(), Matrix::IDENTITY).with(vec![
            Node::shape("sky/gradient".into(), Shape::Rects(vec![[0.0, 0.0, 1920.0, HORIZON]])).paint("url(#grad-sky)"),
            Node::shape("sky/stars".into(), Shape::Rects(stars())).paint("#ffffff").opacity(0.7),
            Node::shape("sky/sun".into(), Shape::Circle { cx: 960.0, cy: 500.0, r: 210.0 }).paint("url(#grad-sun)"),
            Node::shape("sky/halo".into(), Shape::Circle { cx: 960.0, cy: 500.0, r: 268.0 })
                .paint("none").stroke("#19f0ff", 2.0).opacity(0.55).glow(),
        ]),
        Node::group("mountains".into(), Matrix::IDENTITY).with(vec![
            Node::shape("mountains/far".into(), Shape::Polygon(ridge(7, 590.0, 70.0))).paint("#120a2c").stroke("#3c1658", 1.2),
            Node::shape("mountains/near".into(), Shape::Polygon(ridge(19, 622.0, 34.0))).paint("#0b0619").stroke("#5a1a6e", 1.2),
        ]),
        skyline("skyline-far", 100, 18, false),
        Node::group("haze".into(), Matrix([3.4, 0.0, 0.0, 0.5, -2304.0, 320.0])).with(vec![
            Node::shape("haze/bloom".into(), Shape::Circle { cx: 960.0, cy: 640.0, r: 260.0 }).paint("url(#grad-haze)"),
        ]),
        Node::shape("horizon".into(), Shape::Rects(vec![[0.0, HORIZON - 2.0, 1920.0, 3.0]])).paint("#bff9ff").glow(),
        Node::shape("floor".into(), Shape::Rects(vec![[0.0, HORIZON + 1.0, 1920.0, 440.0]])).paint("url(#grad-floor)"),
        Node::group("reflection".into(), reflection).opacity(0.34).with(vec![
            skyline("reflection/skyline", 1, 11, true),
        ]),
        floor_grid(),
        skyline("skyline-near", 1, 11, true),
        Node::group("traffic".into(), Matrix::IDENTITY).with(vec![
            Node::shape("traffic/east".into(), Shape::Rects(vec![[0.0, 618.0, 220.0, 3.0]])).paint("url(#grad-trail)").glow().drift(420.0),
            Node::shape("traffic/west".into(), Shape::Rects(vec![[0.0, 628.0, 160.0, 2.0]])).paint("url(#grad-trail)").glow().drift(-300.0),
        ]),
        Node::shape("vignette".into(), Shape::Rects(vec![[0.0, 0.0, 1920.0, 1080.0]])).paint("url(#grad-vignette)"),
    ])
}

impl Node {
    pub fn group(id: String, transform: Matrix) -> Self {
        Node { transform, ..Node::shape(id, Shape::Group) }
    }

    pub fn shape(id: String, shape: Shape) -> Self {
        Node {
            id,
            shape,
            transform: Matrix::IDENTITY,
            paint: "none",
            stroke: None,
            opacity: 1.0,
            glow: false,
            flicker: None,
            drift: 0.0,
            children: Vec::new(),
        }
    }

    pub fn with(mut self, children: Vec<Node>) -> Self {
        self.children = children;
        self
    }

    pub fn paint(mut self, paint: &'static str) -> Self {
        self.paint = paint;
        self
    }

    pub fn stroke(mut self, color: &'static str, width: f32) -> Self {
        self.stroke = Some((color, width));
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    pub fn glow(mut self) -> Self {
        self.glow = true;
        self
    }

    pub fn drift(mut self, speed: f32) -> Self {
        self.drift = speed;
        self
    }

    pub fn flicker(mut self, seed: f32) -> Self {
        self.flicker = Some(seed);
        self
    }

    pub fn tag(&self) -> &'static str {
        match self.shape {
            Shape::Group => "g",
            Shape::Circle { .. } => "circle",
            _ => "path",
        }
    }

    /// The `d` attribute exactly as rendered.
    pub fn path_data(&self) -> String {
        use std::fmt::Write;
        let mut d = String::new();
        match &self.shape {
            Shape::Group => {}
            Shape::Polygon(points) => {
                for (k, [x, y]) in points.iter().enumerate() {
                    let _ = write!(d, "{}{x:.1} {y:.1} ", if k == 0 { "M" } else { "L" });
                }
                d.push('Z');
            }
            Shape::Rects(rects) => {
                for [x, y, w, h] in rects {
                    let _ = write!(d, "M{x:.1} {y:.1}h{w:.1}v{h:.1}h{:.1}Z", -w);
                }
            }
            Shape::Lines(lines) => {
                for [x0, y0, x1, y1] in lines {
                    let _ = write!(d, "M{x0:.1} {y0:.1}L{x1:.1} {y1:.1}");
                }
            }
            Shape::Circle { cx, cy, r } => {
                let _ = write!(d, "circle({cx} {cy} {r})");
            }
        }
        d
    }

    /// Geometry points in local coordinates, for bounding boxes.
    pub fn points(&self) -> Vec<[f32; 2]> {
        match &self.shape {
            Shape::Group => Vec::new(),
            Shape::Polygon(points) => points.clone(),
            Shape::Rects(rects) => rects.iter().flat_map(|[x, y, w, h]| [[*x, *y], [x + w, y + h]]).collect(),
            Shape::Lines(lines) => lines.iter().flat_map(|[a, b, c, d]| [[*a, *b], [*c, *d]]).collect(),
            Shape::Circle { cx, cy, r } => (0..32)
                .map(|k| {
                    let a = k as f32 / 32.0 * std::f32::consts::TAU;
                    [cx + r * a.cos(), cy + r * a.sin()]
                })
                .collect(),
        }
    }

    /// Path segments: the size of this element's draw call.
    pub fn segments(&self) -> u32 {
        match &self.shape {
            Shape::Group => 0,
            Shape::Polygon(p) => p.len() as u32,
            Shape::Rects(r) => r.len() as u32 * 4,
            Shape::Lines(l) => l.len() as u32,
            Shape::Circle { .. } => 4,
        }
    }
}

/// Deterministic 0..1 noise.
pub fn rand(i: usize, salt: usize) -> f32 {
    let mut h = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (salt as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 31;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 29;
    (h % 10_000) as f32 / 10_000.0
}

/// Window lights: mostly steady, with an occasional soft dip.
pub fn flicker(seed: f32, t: f32) -> f32 {
    let wave = (t * 1.7 + seed).sin() * 0.5 + (t * 4.3 + seed * 2.1).sin() * 0.25;
    (0.86 + 0.14 * wave).clamp(0.55, 1.0)
}

fn ridge(seed: usize, base: f32, amp: f32) -> Vec<[f32; 2]> {
    let mut pts = vec![[0.0, HORIZON]];
    for k in 0..=24 {
        let x = k as f32 * 80.0;
        let y = base - amp * (0.5 + 0.5 * (k as f32 * 0.55 + seed as f32).sin()) - rand(seed * 31 + k, 9) * amp * 0.6;
        pts.push([x, y]);
    }
    pts.push([1920.0, HORIZON]);
    pts
}

fn stars() -> Vec<[f32; 4]> {
    (0..90)
        .map(|k| {
            let s = if rand(k, 13) > 0.85 { 3.0 } else { 1.6 };
            [rand(k, 11) * 1920.0, rand(k, 12).powf(1.6) * 470.0, s, s]
        })
        .collect()
}
