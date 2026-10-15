//! Pass 3, SvgTree: the beauty frame's SVG tree, every element floating in 3-D.
//!
//! Each node starts flat on the frame (z = 0) and lifts to the plane of its tree depth at
//! the second given by `hud::schedule::tree_reveal_times`. What is drawn is the node's
//! real geometry under its real accumulated matrix, its bounding box, and an edge to its
//! parent. Each depth level gets the depth of field of its distance from the camera.
use beauty::NODES;
use beauty::scene::Shape;
use fframes::{Svgr, svgr};
use hud::camera::{TREE_STEP, TREE_Z0};
use hud::schedule::tree_reveal_times;
use hud::{Camera, Matrix, NodeRecord, SceneLayer, TIMELINE, Vec3, mix, palette, smoothstep};
use std::sync::LazyLock;

pub const MONO: &str = "JetBrains Mono";
/// Seconds a node takes to float from the frame to its level.
const LIFT: f32 = 1.1;

static REVEAL: LazyLock<Vec<f32>> = LazyLock::new(|| {
    let records: Vec<NodeRecord> = NODES.iter().map(|(_, r)| r.clone()).collect();
    tree_reveal_times(&records)
});

static MAX_DEPTH: LazyLock<u32> = LazyLock::new(|| NODES.iter().map(|(_, r)| r.depth).max().unwrap_or(0));

pub fn level_z(depth: u32) -> f32 {
    TREE_Z0 - depth as f32 * TREE_STEP
}

fn lift(i: usize, t: f32) -> f32 {
    smoothstep(REVEAL[i], REVEAL[i] + LIFT, t)
}

fn node_z(i: usize, t: f32) -> f32 {
    mix(0.0, level_z(NODES[i].1.depth), lift(i, t))
}

fn anchor(r: &NodeRecord) -> [f32; 2] {
    [r.bbox.x + r.bbox.w * 0.5, r.bbox.y]
}

pub fn layer(cam: &Camera) -> Svgr<'static> {
    let t = cam.t;
    let weight = TIMELINE.weight(SceneLayer::SvgTree, t);
    if weight < 0.01 {
        return Svgr::empty();
    }

    let levels: Vec<Svgr> = (0..=*MAX_DEPTH).map(|depth| level(cam, depth)).collect();
    let spotlight = spotlight(cam);
    let callouts = callouts(cam);

    svgr!(<g opacity={weight}>{levels}{callouts}{spotlight}</g>)
}

/// All nodes of one depth, with that depth's blur.
fn level(cam: &Camera, depth: u32) -> Svgr<'static> {
    let t = cam.t;
    let distance = level_z(depth) - cam.position.z;
    let blur = cam.blur(distance) * 0.5;
    let filter_id = format!("tree-dof-{depth}");
    let items: Vec<Svgr> = NODES
        .iter()
        .enumerate()
        .filter(|(_, (_, r))| r.depth == depth)
        .map(|(i, _)| node(cam, i, t))
        .collect();

    if blur < 0.35 {
        return svgr!(<g>{items}</g>);
    }
    svgr!(
        <g>
            <filter id={filter_id.clone()} x="-10%" y="-10%" width="120%" height="120%">
                <feGaussianBlur stdDeviation={blur} />
            </filter>
            <g filter={format!("url(#{filter_id})")}>{items}</g>
        </g>
    )
}

fn node(cam: &Camera, i: usize, t: f32) -> Svgr<'static> {
    let (shape_node, record) = &NODES[i];
    let z = node_z(i, t);
    let up = lift(i, t);
    let b = record.bbox;
    let corners = [[b.x, b.y], [b.x + b.w, b.y], [b.x + b.w, b.y + b.h], [b.x, b.y + b.h]];
    let Some(projected) = corners
        .iter()
        .map(|[x, y]| cam.project(Vec3::new(*x, *y, z)))
        .collect::<Option<Vec<_>>>()
    else {
        return Svgr::empty();
    };
    let points = projected.iter().map(|p| format!("{:.1},{:.1}", p.x, p.y)).collect::<Vec<_>>().join(" ");
    let presence = mix(0.18, 1.0, up);

    let is_group = !record.is_drawable();
    let box_color = if is_group { palette::MAGENTA } else { palette::CYAN };
    let outline = svgr!(
        <polygon points={points} fill="none" stroke={box_color} stroke-width="1.2"
            stroke-opacity={presence * if is_group { 0.5 } else { 0.32 }}
            stroke-dasharray={if is_group { "6 5" } else { "none" }} />
    );

    // The element's own geometry, under its real matrix, on its plane.
    let geometry = match cam.plane_matrix(0.0, 0.0, z) {
        Some(plane) if !is_group => {
            let m = Matrix(plane).then(record.transform);
            let scale = (m.0[0] * m.0[3] - m.0[1] * m.0[2]).abs().sqrt().max(1e-3);
            let width = 1.3 / scale;
            match shape_node.shape {
                Shape::Circle { cx, cy, r } => svgr!(
                    <circle cx={cx} cy={cy} r={r} transform={m.to_svg()} fill="none"
                        stroke={palette::WHITE} stroke-width={width} stroke-opacity={presence * 0.75} />
                ),
                _ => svgr!(
                    <path d={shape_node.path_data()} transform={m.to_svg()} fill="none"
                        stroke={palette::WHITE} stroke-width={width} stroke-opacity={presence * 0.6} />
                ),
            }
        }
        _ => Svgr::empty(),
    };

    // Edge to the parent, a real 3-D line between the two planes.
    let edge = record
        .parent
        .and_then(|p| {
            let parent = &NODES[p as usize].1;
            let [px, py] = anchor(parent);
            let [cx, cy] = anchor(record);
            let a = cam.project(Vec3::new(px, py, node_z(p as usize, t)))?;
            let c = cam.project(Vec3::new(cx, cy, z))?;
            Some(svgr!(
                <line x1={a.x} y1={a.y} x2={c.x} y2={c.y} stroke={palette::MAGENTA}
                    stroke-width="1" stroke-opacity={0.38 * up} />
            ))
        })
        .unwrap_or_else(Svgr::empty);

    svgr!(<g>{edge}{outline}{geometry}</g>)
}

/// The top-level groups as a column of labels with leader lines to their boxes.
fn callouts(cam: &Camera) -> Svgr<'static> {
    let t = cam.t;
    let rows: Vec<Svgr> = NODES
        .iter()
        .enumerate()
        .filter(|(_, (_, r))| r.depth == 2 && !r.is_drawable())
        .enumerate()
        .filter_map(|(row, (i, (_, r)))| {
            let up = lift(i, t);
            if up < 0.02 {
                return None;
            }
            let [ax, ay] = anchor(r);
            let a = cam.project(Vec3::new(ax, ay, node_z(i, t)))?;
            let (x, y) = (1840.0, 140.0 + row as f32 * 44.0);
            let text = format!("<g#{}> {}", r.name, r.transform.to_svg());
            let start = x - text.chars().count() as f32 * 16.8;
            Some(svgr!(
                <g opacity={up}>
                    <line x1={start - 14.0} y1={y - 9.0} x2={a.x} y2={a.y} stroke={palette::MAGENTA}
                        stroke-width="1" stroke-opacity="0.45" />
                    <circle cx={a.x} cy={a.y} r="3" fill={palette::MAGENTA} />
                    <text x={x} y={y} font-family={MONO} font-size="28" font-weight="400"
                        text-anchor="end" fill={palette::WHITE} fill-opacity="0.85">{text}</text>
                </g>
            ))
        })
        .collect();
    svgr!(<g>{rows}</g>)
}

/// The most recently detached drawable, with its full record.
fn spotlight(cam: &Camera) -> Svgr<'static> {
    let t = cam.t;
    let Some(i) = (0..NODES.len())
        .filter(|&i| NODES[i].1.is_drawable() && REVEAL[i] <= t)
        .max_by(|&a, &b| REVEAL[a].total_cmp(&REVEAL[b]))
    else {
        return Svgr::empty();
    };
    let r = &NODES[i].1;
    let b = r.bbox;
    let lines = [
        format!("<{}#{}>", r.tag, r.name),
        format!("bbox   {:.1}, {:.1}  {:.1} x {:.1}", b.x, b.y, b.w, b.h),
        format!("matrix {}", r.transform.to_svg()),
        format!("d#     {:016x}", r.path_hash),
        format!("depth  {}   segments {}   t {:.3}s", r.depth, r.segments, REVEAL[i]),
    ];
    let rows: Vec<Svgr> = lines
        .into_iter()
        .enumerate()
        .map(|(k, line)| {
            let fill = if k == 0 { palette::CYAN } else { palette::WHITE };
            svgr!(
                <text x="150" y={786.0 + k as f32 * 40.0} font-family={MONO} font-size="28"
                    font-weight="400" fill={fill} fill-opacity="0.9">{line}</text>
            )
        })
        .collect();
    let fade = 1.0 - smoothstep(7.6, 8.2, t);
    svgr!(
        <g opacity={fade}>
            <rect x="110" y="742" width="1000" height="230" rx="6" fill="#000000" fill-opacity="0.72"
                stroke="#ff4fd8" stroke-opacity="0.3" />
            {rows}
        </g>
    )
}
