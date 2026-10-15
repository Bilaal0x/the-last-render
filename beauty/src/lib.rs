//! Pass 1, Beauty: the finished frame, pure SVG.
//!
//! The scene is described as data in [`scene`]; this module turns it into the rendered tree,
//! one [`NodeRecord`] per element for the later passes, and a one-frame [`BeautyStill`]
//! video that the composite bakes to `out/passes/beauty.png` for the shader's `iChannel0`.
pub mod scene;

use fframes::{AudioMap, Duration, FFramesContext, Frame, Svgr, Video, svgr};
use hud::{BBox, HEIGHT, Matrix, NodeRecord, WIDTH, fnv1a64};
use scene::{Node, Shape};
use std::sync::LazyLock;

pub use scene::HORIZON;

/// The scene, built once.
pub static SCENE: LazyLock<Node> = LazyLock::new(scene::city);

/// The source of [`scene`], as it was compiled.
pub const SCENE_SOURCE: &str = include_str!("scene.rs");

/// The part of the scene source shown by the Source pass (between its markers).
pub fn source_excerpt() -> &'static str {
    let begin = SCENE_SOURCE.find("// the-last-render:source-begin").map_or(0, |i| {
        i + SCENE_SOURCE[i..].find('\n').map_or(0, |n| n + 1)
    });
    let end = SCENE_SOURCE.find("// the-last-render:source-end").unwrap_or(SCENE_SOURCE.len());
    SCENE_SOURCE[begin..end].trim_end()
}

/// Every node in document order (= draw order), paired with its record.
pub static NODES: LazyLock<Vec<(&'static Node, NodeRecord)>> = LazyLock::new(|| {
    let mut out = Vec::new();
    let root_box = BBox { x: 0.0, y: 0.0, w: WIDTH as f32, h: HEIGHT as f32 };
    out.push((
        &*SCENE,
        NodeRecord {
            id: 0,
            parent: None,
            tag: "svg".into(),
            name: "the-last-render".into(),
            bbox: root_box,
            transform: Matrix::IDENTITY,
            path_hash: fnv1a64(b"svg 1920 1080"),
            depth: 0,
            paint: "#000000".into(),
            segments: 0,
        },
    ));
    walk(&SCENE, Some(0), Matrix::IDENTITY, 1, &mut out);
    out
});

fn walk(
    node: &'static Node,
    parent: Option<u32>,
    ctm: Matrix,
    depth: u32,
    out: &mut Vec<(&'static Node, NodeRecord)>,
) -> BBox {
    let ctm = ctm.then(node.transform);
    let id = out.len() as u32;
    let hash_source = match node.shape {
        Shape::Group => format!("g {} {}", node.id, node.transform.to_svg()),
        _ => node.path_data(),
    };
    out.push((
        node,
        NodeRecord {
            id,
            parent,
            tag: node.tag().into(),
            name: node.id.clone(),
            bbox: BBox::from_points(node.points().into_iter().map(|p| ctm.apply(p))),
            transform: ctm,
            path_hash: fnv1a64(hash_source.as_bytes()),
            depth,
            paint: node.paint.into(),
            segments: node.segments(),
        },
    ));
    let mut bbox = out[id as usize].1.bbox;
    for child in &node.children {
        bbox = bbox.union(walk(child, Some(id), ctm, depth + 1, out));
    }
    out[id as usize].1.bbox = bbox;
    bbox
}

pub fn records() -> Vec<NodeRecord> {
    NODES.iter().map(|(_, r)| r.clone()).collect()
}

/// Gradients and filters of the frame.
pub fn defs() -> Svgr<'static> {
    svgr!(
        <defs>
            <linearGradient id="grad-sky" x1="0" y1="0" x2="0" y2="640" gradientUnits="userSpaceOnUse">
                <stop offset="0" stop-color="#000000" />
                <stop offset="0.5" stop-color="#05071a" />
                <stop offset="0.8" stop-color="#1d0a35" />
                <stop offset="0.95" stop-color="#6a1863" />
                <stop offset="1" stop-color="#ff4fd8" />
            </linearGradient>
            <linearGradient id="grad-sun" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0" stop-color="#ffffff" />
                <stop offset="0.3" stop-color="#ffd6f6" />
                <stop offset="0.62" stop-color="#ff4fd8" />
                <stop offset="1" stop-color="#4a0f52" />
            </linearGradient>
            <radialGradient id="grad-haze" cx="0.5" cy="0.5" r="0.5">
                <stop offset="0" stop-color="#ff4fd8" stop-opacity="0.75" />
                <stop offset="0.4" stop-color="#b02a92" stop-opacity="0.32" />
                <stop offset="1" stop-color="#7a1a6e" stop-opacity="0" />
            </radialGradient>
            <linearGradient id="grad-floor" x1="0" y1="641" x2="0" y2="1080" gradientUnits="userSpaceOnUse">
                <stop offset="0" stop-color="#1a0726" />
                <stop offset="0.25" stop-color="#06030e" />
                <stop offset="1" stop-color="#000000" />
            </linearGradient>
            <linearGradient id="grad-trail" x1="0" y1="0" x2="1" y2="0">
                <stop offset="0" stop-color="#ff4fd8" stop-opacity="0" />
                <stop offset="0.75" stop-color="#ff4fd8" stop-opacity="0.85" />
                <stop offset="1" stop-color="#ffffff" />
            </linearGradient>
            <radialGradient id="grad-vignette" cx="0.5" cy="0.52" r="0.72">
                <stop offset="0.55" stop-color="#000000" stop-opacity="0" />
                <stop offset="1" stop-color="#000000" stop-opacity="0.82" />
            </radialGradient>
            <filter id="glow" x="-50%" y="-50%" width="200%" height="200%">
                <feGaussianBlur stdDeviation="5" result="blur" />
                <feMerge>
                    <feMergeNode in="blur" />
                    <feMergeNode in="blur" />
                    <feMergeNode in="SourceGraphic" />
                </feMerge>
            </filter>
        </defs>
    )
}

/// The finished frame at second `t`, drawn on the 1920x1080 grid.
pub fn render(t: f32) -> Svgr<'static> {
    let tree = scene::render_node(&SCENE, t);
    svgr!(
        <g>
            {defs()}
            <clipPath id="frame-clip"><rect width="1920" height="1080" /></clipPath>
            <g clip-path="url(#frame-clip)">{tree}</g>
        </g>
    )
}

/// A one-frame video of the beauty pass at a given second, for baking to pixels.
#[derive(Debug)]
pub struct BeautyStill {
    pub t: f32,
}

impl Video for BeautyStill {
    const FPS: usize = hud::FPS;
    const WIDTH: usize = WIDTH;
    const HEIGHT: usize = HEIGHT;

    fn duration(&self) -> Duration<'_> {
        Duration::Frames(1)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    fn render_frame<'a>(&'a self, _frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" width={WIDTH} height={HEIGHT}>
                {render(self.t)}
            </svg>
        )
    }
}
