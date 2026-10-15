//! Pass 2, Commands: the GPU command buffer that drew the beauty frame.
//!
//! [`Pipeline`] owns the one SkSL program of the film (`shaders/pipeline.sksl`). It reads
//! the baked beauty pass as `iChannel0` and runs in four modes, blended by the layer
//! weights, plus a post mode for grain and vignette. [`command_list`] is the readable
//! side: the actual draw calls, in order, with their real boxes and path hashes.
use fframes::media::ImageData;
use fframes::{Frame, Shader, ShaderUniforms, Svgr, svgr};
use hud::camera::{self, Camera, FOCAL};
use hud::schedule::{self, DRAW_WINDOW};
use hud::{NodeRecord, SceneLayer, TIMELINE, palette, smoothstep};

pub const PIPELINE_SKSL: &str = include_str!("../shaders/pipeline.sksl");

pub const MONO: &str = "JetBrains Mono";

pub struct Pipeline {
    shader: Shader,
}

impl Default for Pipeline {
    fn default() -> Self {
        Self { shader: Shader::sksl(PIPELINE_SKSL) }
    }
}

/// How far the draw calls have run, 0..1, matching `schedule::draw_call_time`.
pub fn scan(t: f32) -> f32 {
    let p = ((t - DRAW_WINDOW.0) / (DRAW_WINDOW.1 - DRAW_WINDOW.0)).clamp(0.0, 1.0);
    p.powf(1.0 / 0.85)
}

/// 0..1 of the final collapse into the cursor.
pub fn collapse(t: f32) -> f32 {
    smoothstep(TIMELINE.start(SceneLayer::Cursor), schedule::CURSOR_CLICK, t)
}

impl Pipeline {
    pub fn shader(&self) -> &Shader {
        &self.shader
    }

    fn uniforms(&self, cam: &Camera, beauty: &ImageData<'_>) -> ShaderUniforms {
        let t = cam.t;
        let w = TIMELINE.weights(t);
        ShaderUniforms::new()
            .image("iChannel0", beauty)
            .float2("uImageSize", beauty.metadata.width as f32, beauty.metadata.height as f32)
            .float4("uMode", w[0], w[1], w[2], w[3])
            .float3("uCam", cam.position.x, cam.position.y, cam.position.z)
            .float("uRoll", cam.roll)
            .float("uFocal", FOCAL)
            .float("uFocus", cam.focus)
            .float("uAperture", cam.aperture)
            .float("uAberration", cam.aberration)
            .float("uGrain", cam.grain)
            .float("uSplit", smoothstep(3.15, 5.9, t))
            .float("uScan", scan(t))
            .float("uCollapse", collapse(t))
            .float("uClock", t)
            .float("uCmdDepth", camera::COMMANDS_DEPTH)
            .float("uTreeZ0", camera::TREE_Z0)
            .float("uTreeStep", camera::TREE_STEP)
            .float("uSourceZ", camera::SOURCE_Z)
    }

    /// The 3-D backdrop of every layer: beauty plane, command slices, tree space, particles.
    pub fn scene(&self, frame: &Frame, cam: &Camera, beauty: &ImageData<'_>) -> ImageData<'static> {
        self.shader.draw(frame, self.uniforms(cam, beauty).float("uPost", 0.0))
    }

    /// Grain and vignette over everything.
    pub fn post(&self, frame: &Frame, cam: &Camera, beauty: &ImageData<'_>) -> ImageData<'static> {
        self.shader.draw(frame, self.uniforms(cam, beauty).float("uPost", 1.0))
    }
}

const ROWS: usize = 7;
const ROW_H: f32 = 40.0;
const PANEL_X: f32 = 150.0;
const PANEL_Y: f32 = 590.0;

/// The command buffer as text, scrolling with execution, plus the box of the current call
/// projected onto the frame through `cam`.
pub fn command_list<'a>(nodes: &'a [NodeRecord], cam: &Camera) -> Svgr<'a> {
    let t = cam.t;
    let weight = TIMELINE.weight(SceneLayer::Commands, t);
    if weight < 0.01 {
        return Svgr::empty();
    }
    let calls = schedule::draw_calls(nodes);
    let n = calls.len();
    let current = (0..n).rev().find(|&k| schedule::draw_call_time(k, n) <= t);
    let head = current.map_or(0, |k| k + 1);
    let first = head.saturating_sub(ROWS - 3).min(n.saturating_sub(ROWS));

    let rows: Vec<Svgr> = (first..(first + ROWS).min(n))
        .map(|k| {
            let node = calls[k];
            let y = PANEL_Y + 92.0 + (k - first) as f32 * ROW_H;
            let is_current = Some(k) == current;
            let done = current.is_some_and(|c| k < c);
            let fill = if is_current { palette::WHITE } else if done { palette::CYAN } else { palette::DIM };
            let opacity = if is_current { 1.0 } else if done { 0.55 } else { 0.4 };
            let op = if node.tag == "circle" { "circle" } else { "path  " };
            let line = format!("{k:03} {op} {:<19} {}", truncate(&node.name, 19), node.short_hash());
            let bar = if is_current {
                svgr!(<rect x={PANEL_X - 18.0} y={y - 28.0} width="6" height="34" fill={palette::CYAN} />)
            } else {
                Svgr::empty()
            };
            svgr!(
                <g opacity={opacity}>
                    {bar}
                    <text x={PANEL_X} y={y} font-family={MONO} font-size="28" font-weight="400" fill={fill}>{line}</text>
                </g>
            )
        })
        .collect();

    let header = format!("cmdbuf  t={t:.3}s  {head:>3}/{n} draws");
    let focus = current.map(|k| calls[k]).map_or(Svgr::empty(), |node| highlight(node, cam));

    svgr!(
        <g opacity={weight}>
            {focus}
            <rect x={PANEL_X - 40.0} y={PANEL_Y} width="700" height={ROW_H * ROWS as f32 + 120.0} rx="6"
                fill="#000000" fill-opacity="0.72" stroke="#19f0ff" stroke-opacity="0.25" />
            <text x={PANEL_X} y={PANEL_Y + 46.0} font-family={MONO} font-size="28" font-weight="600" fill={palette::MAGENTA}>
                {header}
            </text>
            {rows}
        </g>
    )
}

/// The current draw call's bounding box on the frame plane, with its real coordinates.
fn highlight<'a>(node: &'a NodeRecord, cam: &Camera) -> Svgr<'a> {
    // The box as it lands on the canvas.
    let b = node.bbox;
    let (x0, y0) = (b.x.max(0.0), b.y.max(0.0));
    let (x1, y1) = ((b.x + b.w).min(hud::WIDTH as f32), (b.y + b.h).min(hud::HEIGHT as f32));
    let b = hud::BBox { x: x0, y: y0, w: (x1 - x0).max(1.0), h: (y1 - y0).max(1.0) };
    let corners = [[b.x, b.y], [b.x + b.w, b.y], [b.x + b.w, b.y + b.h], [b.x, b.y + b.h]];
    let projected: Option<Vec<_>> = corners
        .iter()
        .map(|[x, y]| cam.project(hud::Vec3::new(*x, *y, 0.0)))
        .collect();
    let Some(p) = projected else { return Svgr::empty() };
    let points = p.iter().map(|q| format!("{:.1},{:.1}", q.x, q.y)).collect::<Vec<_>>().join(" ");
    let label = format!("{:.0},{:.0}  {:.0}x{:.0}", b.x, b.y, b.w, b.h);
    let (lx, ly) = (p[0].x.clamp(40.0, 1500.0), (p[0].y - 14.0).clamp(60.0, 1040.0));
    svgr!(
        <g>
            <polygon points={points} fill="#19f0ff" fill-opacity="0.08" stroke="#19f0ff" stroke-width="2" />
            <text x={lx} y={ly} font-family={MONO} font-size="28" font-weight="400" fill={palette::CYAN}>{label}</text>
        </g>
    )
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_owned()
    } else {
        let mut out: String = s.chars().take(n - 1).collect();
        out.push('…');
        out
    }
}
