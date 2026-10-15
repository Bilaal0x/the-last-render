//! When each discrete event of the film happens. Both the visual passes and the
//! synthesizer derive their timing from these functions (through `tracks.json`).
use crate::node::NodeRecord;
use crate::timeline::smoothstep;

/// The command buffer executes between these seconds.
pub const DRAW_WINDOW: (f32, f32) = (3.1, 5.25);
/// The tree detaches level by level from here.
pub const TREE_START: f32 = 5.55;
pub const TREE_LEVEL_STAGGER: f32 = 0.34;
pub const TREE_SIBLING_SPREAD: f32 = 0.9;
/// Typing of the source pass.
pub const TYPE_WINDOW: (f32, f32) = (8.15, 10.3);
pub const KEYSTROKES: usize = 30;
/// The cursor lands with one click.
pub const CURSOR_CLICK: f32 = 10.92;
/// The cursor blinks with this period after it lands (on for the first half).
pub const BLINK_PERIOD: f32 = 0.72;

/// Second at which draw call `k` of `n` executes (accelerating like a warming GPU).
pub fn draw_call_time(k: usize, n: usize) -> f32 {
    let p = k as f32 / n.saturating_sub(1).max(1) as f32;
    DRAW_WINDOW.0 + (DRAW_WINDOW.1 - DRAW_WINDOW.0) * p.powf(0.85)
}

/// Drawable nodes in draw order: what the GPU command buffer contains.
pub fn draw_calls(nodes: &[NodeRecord]) -> Vec<&NodeRecord> {
    nodes.iter().filter(|n| n.is_drawable()).collect()
}

/// Second at which each node detaches from the flat frame into the 3-D tree, by id.
pub fn tree_reveal_times(nodes: &[NodeRecord]) -> Vec<f32> {
    let max_depth = nodes.iter().map(|n| n.depth).max().unwrap_or(0);
    let mut times = vec![TREE_START; nodes.len()];
    for depth in 0..=max_depth {
        let level: Vec<usize> = (0..nodes.len()).filter(|&i| nodes[i].depth == depth).collect();
        let count = level.len().max(1) as f32;
        for (rank, &i) in level.iter().enumerate() {
            times[i] = TREE_START
                + TREE_LEVEL_STAGGER * depth as f32
                + TREE_SIBLING_SPREAD * (rank as f32 / count) / (1.0 + depth as f32 * 0.35);
        }
    }
    times
}

/// How many characters of the source are on screen at `t`.
pub fn typed_chars(t: f32, total_chars: usize) -> usize {
    let p = smoothstep(TYPE_WINDOW.0, TYPE_WINDOW.1, t);
    // Mostly linear with a soft start and stop, like a fast typist.
    let p = 0.3 * p + 0.7 * ((t - TYPE_WINDOW.0) / (TYPE_WINDOW.1 - TYPE_WINDOW.0)).clamp(0.0, 1.0);
    (p * total_chars as f32).round() as usize
}

/// Seconds of the audible key clicks: one every time the typing head crosses a chunk.
pub fn keystroke_times(total_chars: usize, fps: usize) -> Vec<f32> {
    let chunk = (total_chars / KEYSTROKES).max(1);
    let steps = ((TYPE_WINDOW.1 - TYPE_WINDOW.0) * fps as f32 * 8.0) as usize;
    let mut times = Vec::new();
    let mut next = chunk;
    for s in 0..=steps {
        let t = TYPE_WINDOW.0 + s as f32 / (fps as f32 * 8.0);
        if typed_chars(t, total_chars) >= next {
            times.push(t);
            next += chunk;
        }
    }
    times
}

/// Cursor visibility after it lands.
pub fn cursor_on(t: f32) -> bool {
    t >= CURSOR_CLICK && ((t - CURSOR_CLICK) / BLINK_PERIOD).fract() < 0.5
}

