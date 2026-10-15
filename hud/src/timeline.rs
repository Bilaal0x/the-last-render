//! The five layers of the film and the exact seconds where one hands over to the next.
use serde::{Deserialize, Serialize};

/// One stage of the render pipeline, in the order the film peels them away.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SceneLayer {
    Beauty,
    Commands,
    SvgTree,
    Source,
    Cursor,
}

impl SceneLayer {
    pub const ALL: [SceneLayer; 5] = [
        SceneLayer::Beauty,
        SceneLayer::Commands,
        SceneLayer::SvgTree,
        SceneLayer::Source,
        SceneLayer::Cursor,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    /// Scene name used by the CLI: `strip Commands`, `frame SvgTree@50%`.
    pub const fn name(self) -> &'static str {
        match self {
            SceneLayer::Beauty => "Beauty",
            SceneLayer::Commands => "Commands",
            SceneLayer::SvgTree => "SvgTree",
            SceneLayer::Source => "Source",
            SceneLayer::Cursor => "Cursor",
        }
    }
}

/// Transition points of the film in seconds: `marks[i]..marks[i + 1]` is layer `i`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Timeline {
    pub marks: [f32; 6],
    /// How long layer `i + 1` takes to dissolve in over layer `i`, starting at `marks[i + 1]`.
    pub dissolve: [f32; 4],
}

pub const TIMELINE: Timeline = Timeline {
    marks: [0.0, 3.0, 5.5, 8.0, 10.5, 12.0],
    dissolve: [1.6, 1.3, 1.2, 0.45],
};

pub const FPS: usize = 30;
pub const WIDTH: usize = 1920;
pub const HEIGHT: usize = 1080;

impl Timeline {
    pub fn duration(&self) -> f32 {
        self.marks[5]
    }

    pub fn total_frames(&self) -> usize {
        (self.duration() * FPS as f32).round() as usize
    }

    pub fn span(&self, layer: SceneLayer) -> (f32, f32) {
        let i = layer.index();
        (self.marks[i], self.marks[i + 1])
    }

    pub fn start(&self, layer: SceneLayer) -> f32 {
        self.marks[layer.index()]
    }

    pub fn layer_at(&self, t: f32) -> SceneLayer {
        SceneLayer::ALL
            .into_iter()
            .rev()
            .find(|l| t >= self.start(*l))
            .unwrap_or(SceneLayer::Beauty)
    }

    /// 0..1 through a layer's own span, clamped outside it.
    pub fn progress(&self, layer: SceneLayer, t: f32) -> f32 {
        let (a, b) = self.span(layer);
        ((t - a) / (b - a)).clamp(0.0, 1.0)
    }

    /// Seconds since a layer started (negative before it).
    pub fn local(&self, layer: SceneLayer, t: f32) -> f32 {
        t - self.start(layer)
    }

    /// How far the dissolve into `layer` has gone (Beauty is always fully in).
    pub fn arrival(&self, layer: SceneLayer, t: f32) -> f32 {
        let i = layer.index();
        if i == 0 {
            return 1.0;
        }
        smoothstep(0.0, self.dissolve[i - 1], t - self.marks[i])
    }

    /// Visibility of a layer: in with its own dissolve, out with the next one's.
    pub fn weight(&self, layer: SceneLayer, t: f32) -> f32 {
        let next = SceneLayer::ALL.get(layer.index() + 1).copied();
        let leave = next.map_or(0.0, |n| self.arrival(n, t));
        self.arrival(layer, t) * (1.0 - leave)
    }

    pub fn weights(&self, t: f32) -> [f32; 5] {
        SceneLayer::ALL.map(|l| self.weight(l, t))
    }

    /// Continuous pipeline depth: 0 = finished frame, 4 = the first keystroke, 5 = the end.
    pub fn depth(&self, t: f32) -> f32 {
        let layer = self.layer_at(t);
        layer.index() as f32 + self.progress(layer, t)
    }
}

pub fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
