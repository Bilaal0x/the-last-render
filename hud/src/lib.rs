//! Shared data of The Last Render: the timeline, the camera, the records of every SVG
//! node and the `tracks.json` that drives both the picture and the sound.
pub mod camera;
pub mod node;
pub mod schedule;
pub mod timeline;
pub mod tracks;

pub use camera::{Camera, Projected, Vec3};
pub use node::{BBox, Matrix, NodeRecord, fnv1a64};
pub use timeline::{FPS, HEIGHT, SceneLayer, TIMELINE, Timeline, WIDTH, mix, smoothstep};
pub use tracks::{AudioControls, Event, EventKind, FrameTrack, Tracks};

/// Palette: deep blacks, electric cyan, soft magenta, pure white highlights.
pub mod palette {
    pub const BLACK: &str = "#000000";
    pub const INK: &str = "#04050a";
    pub const NIGHT: &str = "#070a1a";
    pub const CYAN: &str = "#19f0ff";
    pub const CYAN_DEEP: &str = "#0a7f99";
    pub const MAGENTA: &str = "#ff4fd8";
    pub const MAGENTA_DEEP: &str = "#7a1a6e";
    pub const WHITE: &str = "#ffffff";
    pub const DIM: &str = "#5b6b84";
}
