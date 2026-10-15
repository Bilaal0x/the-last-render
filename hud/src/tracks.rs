//! `tracks.json`: the single source of truth read by the composite pass and the synthesizer.
use crate::camera::Camera;
use crate::node::NodeRecord;
use crate::schedule;
use crate::timeline::{FPS, HEIGHT, SceneLayer, TIMELINE, Timeline, WIDTH, mix};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tracks {
    pub fps: usize,
    pub width: usize,
    pub height: usize,
    pub timeline: Timeline,
    pub frames: Vec<FrameTrack>,
    pub nodes: Vec<NodeRecord>,
    pub events: Vec<Event>,
}

/// Per-frame state, sampled at the start of each video frame.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FrameTrack {
    pub frame: usize,
    pub t: f32,
    pub layer: SceneLayer,
    /// 0..5, see `Timeline::depth`.
    pub depth: f32,
    /// Visibility of Beauty, Commands, SvgTree, Source, Cursor.
    pub weights: [f32; 5],
    pub camera: Camera,
    /// Continuous synth controls derived from the camera and the layer weights.
    pub audio: AudioControls,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct AudioControls {
    /// Sub-bass + warm pad (Beauty).
    pub pad_gain: f32,
    /// Pad low-pass cutoff in Hz: closes as the camera pulls away.
    pub pad_cutoff: f32,
    /// Crystalline shimmer bed (Commands).
    pub shimmer_gain: f32,
    /// Data-stream texture (SvgTree).
    pub stream_gain: f32,
    /// Stream band-pass centre in Hz.
    pub stream_hz: f32,
    /// Pure sine drone (Source).
    pub sine_gain: f32,
    pub sine_hz: f32,
    /// Stereo position of the beds, follows the camera's lateral drift (-1..1).
    pub pan: f32,
    /// Camera distance from the beauty frame in world pixels.
    pub distance: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub t: f32,
    pub kind: EventKind,
    /// -1 (left) .. 1 (right).
    pub pan: f32,
    pub pitch_hz: f32,
    /// Linear gain before the master bus.
    pub gain: f32,
    /// Filter cutoff / brightness in Hz.
    pub cutoff_hz: f32,
    /// The node this event belongs to, if any.
    pub node: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    /// A draw call executes: metallic, crystalline ping.
    DrawCall,
    /// A node detaches into the 3-D tree: soft digital click.
    NodeReveal,
    /// A key of the source is typed: soft typewriter click.
    KeyStroke,
    /// The cursor lands: the last sound of the film.
    CursorClick,
}

/// Minor-ninth-free pentatonic in E: the pitches the crystalline tones may take.
const SCALE: [f32; 10] = [
    659.26, 739.99, 830.61, 987.77, 1108.73, 1318.51, 1479.98, 1661.22, 1975.53, 2217.46,
];

fn pan_of(x: f32) -> f32 {
    ((x / WIDTH as f32) * 2.0 - 1.0).clamp(-1.0, 1.0) * 0.8
}

impl Tracks {
    /// Builds the tracks from the beauty pass's nodes and the size of the source pass.
    pub fn build(nodes: Vec<NodeRecord>, source_chars: usize) -> Self {
        let frames = (0..TIMELINE.total_frames())
            .map(|frame| {
                let t = frame as f32 / FPS as f32;
                let camera = Camera::at(t);
                let weights = TIMELINE.weights(t);
                let distance = -camera.position.z;
                let depth = TIMELINE.depth(t);
                FrameTrack {
                    frame,
                    t,
                    layer: TIMELINE.layer_at(t),
                    depth,
                    weights,
                    camera,
                    audio: AudioControls {
                        pad_gain: weights[0].max(weights[1] * 0.35),
                        pad_cutoff: (2400.0 * (1400.0 / distance).powf(1.6)).max(180.0),
                        shimmer_gain: weights[1],
                        stream_gain: weights[2],
                        stream_hz: mix(900.0, 3200.0, ((distance - 2350.0) / 900.0).clamp(0.0, 1.0)),
                        sine_gain: weights[3],
                        // A fifth above the pad root, falling an octave as the source collapses.
                        sine_hz: 329.63 * mix(1.0, 0.5, TIMELINE.progress(SceneLayer::Source, t).powi(3)),
                        pan: ((camera.position.x - 960.0) / 70.0) * 0.35,
                        distance,
                    },
                }
            })
            .collect();

        let mut events = Vec::new();

        let calls = schedule::draw_calls(&nodes);
        let n = calls.len();
        for (k, node) in calls.iter().enumerate() {
            let area = (node.bbox.w * node.bbox.h).max(1.0);
            let size = (area / (WIDTH * HEIGHT) as f32).sqrt().clamp(0.0, 1.0);
            let step = (node.path_hash % SCALE.len() as u64) as usize;
            events.push(Event {
                t: schedule::draw_call_time(k, n),
                kind: EventKind::DrawCall,
                pan: pan_of(node.bbox.center()[0]),
                // Big shapes ring low, small ones high.
                pitch_hz: SCALE[step] * if size > 0.25 { 0.5 } else { 1.0 },
                gain: mix(0.10, 0.32, size.sqrt()) * mix(0.7, 1.0, (node.segments as f32 / 24.0).min(1.0)),
                cutoff_hz: mix(9000.0, 3500.0, node.depth as f32 / 5.0),
                node: Some(node.id),
            });
        }

        for (node, t) in nodes.iter().zip(schedule::tree_reveal_times(&nodes)) {
            events.push(Event {
                t,
                kind: EventKind::NodeReveal,
                pan: pan_of(node.bbox.center()[0]),
                pitch_hz: 1800.0 + 700.0 * node.depth as f32 + (node.path_hash % 400) as f32,
                gain: 0.22 / (1.0 + node.depth as f32 * 0.25),
                cutoff_hz: 2000.0 + 1500.0 * node.depth as f32,
                node: Some(node.id),
            });
        }

        for (i, t) in schedule::keystroke_times(source_chars, FPS).into_iter().enumerate() {
            let jitter = ((i * 2654435761usize) % 1000) as f32 / 1000.0;
            events.push(Event {
                t,
                kind: EventKind::KeyStroke,
                // The typing head moves left to right on every line.
                pan: mix(-0.35, 0.35, jitter),
                pitch_hz: 2600.0 + 900.0 * jitter,
                gain: 0.30 + 0.08 * jitter,
                cutoff_hz: 5200.0,
                node: None,
            });
        }

        events.push(Event {
            t: schedule::CURSOR_CLICK,
            kind: EventKind::CursorClick,
            pan: 0.0,
            pitch_hz: 1200.0,
            gain: 0.55,
            cutoff_hz: 3800.0,
            node: None,
        });

        events.sort_by(|a, b| a.t.total_cmp(&b.t));

        Tracks { fps: FPS, width: WIDTH, height: HEIGHT, timeline: TIMELINE, frames, nodes, events }
    }

    pub fn write(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        if let Some(dir) = path.as_ref().parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }

    pub fn read(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        serde_json::from_str(&json).map_err(std::io::Error::other)
    }

    pub fn duration(&self) -> f32 {
        self.timeline.duration()
    }

    /// The frame track at second `t` (clamped).
    pub fn frame_at(&self, t: f32) -> &FrameTrack {
        let i = ((t * self.fps as f32) as usize).min(self.frames.len() - 1);
        &self.frames[i]
    }

    /// Linearly interpolated audio controls at an exact second (for sample-rate use).
    pub fn audio_at(&self, t: f32) -> AudioControls {
        let x = (t * self.fps as f32).clamp(0.0, (self.frames.len() - 1) as f32);
        let (i, f) = (x.floor() as usize, x.fract());
        let a = self.frames[i].audio;
        let b = self.frames[(i + 1).min(self.frames.len() - 1)].audio;
        let l = |p: f32, q: f32| mix(p, q, f);
        AudioControls {
            pad_gain: l(a.pad_gain, b.pad_gain),
            pad_cutoff: l(a.pad_cutoff, b.pad_cutoff),
            shimmer_gain: l(a.shimmer_gain, b.shimmer_gain),
            stream_gain: l(a.stream_gain, b.stream_gain),
            stream_hz: l(a.stream_hz, b.stream_hz),
            sine_gain: l(a.sine_gain, b.sine_gain),
            sine_hz: l(a.sine_hz, b.sine_hz),
            pan: l(a.pan, b.pan),
            distance: l(a.distance, b.distance),
        }
    }
}
