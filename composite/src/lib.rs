//! The final pass: one continuous camera through the four layers of the pipeline.
//!
//! The film has no cuts. The five scenes below only name the spans of the timeline for
//! the CLI (`strip Commands`, `frame SvgTree@50%`); every frame is drawn here from the
//! global second, with each pass blended in by `hud::TIMELINE.weights`.
use fframes::{
    AudioMap, AudioTimestamp::*, AudioTrack, Color, Duration, FFramesContext, Frame, Scene, Scenes,
    Svgr, Video, include_media_dir, svgr,
};
use hud::{Camera, HEIGHT, NodeRecord, SceneLayer, TIMELINE, WIDTH};

// Fonts compiled into the binary (JetBrains Mono, OFL).
include_media_dir!(pub struct StaticMedia, "composite/media");

/// The passes written to disk before rendering, loaded at runtime.
pub const PASS_DIR: &str = "out/passes";
pub const TRACKS_PATH: &str = "out/tracks.json";
pub const BEAUTY_PNG: &str = "beauty.png";
pub const SCORE_WAV: &str = "score.wav";
/// The second of the beauty pass that the command buffer takes apart.
pub const BAKE_AT: f32 = 3.0;

#[derive(Debug)]
pub struct Shot {
    pub layer: SceneLayer,
}

impl Scene for Shot {
    fn duration(&self) -> Duration<'_> {
        let (a, b) = TIMELINE.span(self.layer);
        Duration::Frames(((b - a) * hud::FPS as f32).round() as usize)
    }

    fn render_frame<'a>(&'a self, _frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        Svgr::empty()
    }

    fn name(&self) -> &'static str {
        self.layer.name()
    }
}

pub struct TheLastRender {
    pipeline: commands::Pipeline,
    nodes: Vec<NodeRecord>,
    shots: [Shot; 5],
    with_audio: bool,
}

impl std::fmt::Debug for TheLastRender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TheLastRender").field("nodes", &self.nodes.len()).finish()
    }
}

impl TheLastRender {
    pub fn new(nodes: Vec<NodeRecord>, with_audio: bool) -> Self {
        Self {
            pipeline: commands::Pipeline::default(),
            nodes,
            shots: SceneLayer::ALL.map(|layer| Shot { layer }),
            with_audio,
        }
    }
}

impl Video for TheLastRender {
    const FPS: usize = hud::FPS;
    const WIDTH: usize = WIDTH;
    const HEIGHT: usize = HEIGHT;
    const BACKGROUND_COLOR: Color = Color::BLACK;

    fn duration(&self) -> Duration<'_> {
        Duration::Auto
    }

    fn audio(&self) -> AudioMap<'_> {
        if self.with_audio {
            AudioMap::from([AudioTrack::new(SCORE_WAV, Second(0.0)..Eof)])
        } else {
            AudioMap::none()
        }
    }

    fn define_scenes(&self) -> Scenes<'_> {
        Scenes::from(self.shots.iter().map(|s| s as &dyn Scene).collect::<Vec<_>>())
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let t = frame.global_index as f32 / hud::FPS as f32;
        let cam = Camera::at(t);
        let w = TIMELINE.weights(t);

        let (backdrop, post) = match ctx.get_image(BEAUTY_PNG) {
            Some(beauty) => {
                let scene = self.pipeline.scene(&frame, &cam, beauty);
                let post = self.pipeline.post(&frame, &cam, beauty);
                (
                    svgr!(<image href={scene.href()} x="0" y="0" width={WIDTH} height={HEIGHT} />),
                    svgr!(<image href={post.href()} x="0" y="0" width={WIDTH} height={HEIGHT} />),
                )
            }
            None => (Svgr::empty(), Svgr::empty()),
        };

        // The live SVG beauty pass, hung on the z = 0 plane of the camera.
        let live = match cam.plane_matrix(0.0, 0.0, hud::camera::BEAUTY_Z) {
            Some(m) if w[0] > 0.001 => {
                let matrix = hud::Matrix(m).to_svg();
                svgr!(<g transform={matrix} opacity={w[0]}>{beauty::render(t)}</g>)
            }
            _ => Svgr::empty(),
        };

        // Chromatic aberration for the vector layers: red and blue pulled apart sideways.
        let ca = cam.aberration * 0.4;
        let overlays = svgr!(
            <g>
                {commands::command_list(&self.nodes, &cam)}
                {svg_tree::layer(&cam)}
                {source::layer(&cam)}
            </g>
        );
        let overlays = if ca > 0.3 {
            svgr!(
                <g>
                    <filter id="aberration" x="0" y="0" width="1920" height="1080" filterUnits="userSpaceOnUse">
                        <feOffset in="SourceGraphic" dx={-ca} dy="0" result="shift-r" />
                        <feColorMatrix in="shift-r" type="matrix" result="ch-r"
                            values="1 0 0 0 0  0 0 0 0 0  0 0 0 0 0  0 0 0 1 0" />
                        <feColorMatrix in="SourceGraphic" type="matrix" result="ch-g"
                            values="0 0 0 0 0  0 1 0 0 0  0 0 0 0 0  0 0 0 1 0" />
                        <feOffset in="SourceGraphic" dx={ca} dy="0" result="shift-b" />
                        <feColorMatrix in="shift-b" type="matrix" result="ch-b"
                            values="0 0 0 0 0  0 0 0 0 0  0 0 1 0 0  0 0 0 1 0" />
                        <feBlend in="ch-r" in2="ch-g" mode="screen" result="rg" />
                        <feBlend in="rg" in2="ch-b" mode="screen" />
                    </filter>
                    <g filter="url(#aberration)">{overlays}</g>
                </g>
            )
        } else {
            overlays
        };

        svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" width={WIDTH} height={HEIGHT}>
                <rect width={WIDTH} height={HEIGHT} fill="#000000" />
                {backdrop}
                {live}
                {overlays}
                {post}
                {source::cursor(&cam)}
            </svg>
        )
    }
}
