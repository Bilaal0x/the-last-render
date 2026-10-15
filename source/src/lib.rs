//! Pass 4, Source: the Rust that generated the frame, typed back into existence.
//!
//! The text is the actual source of `beauty::scene` (the region between its
//! `the-last-render:source` markers), with its real line numbers. As the typing head
//! passes an `svgr!` call or a `tower` group, the line expands into the element it
//! produced, with the values the renderer uses at this very frame.
use beauty::{NODES, SCENE_SOURCE, source_excerpt};
use fframes::{Svgr, svgr};
use hud::camera::SOURCE_Z;
use hud::schedule::{self, CURSOR_CLICK, TYPE_WINDOW};
use hud::{Camera, NodeRecord, SceneLayer, TIMELINE, Vec3, mix, palette, smoothstep};
use std::sync::LazyLock;

pub const MONO: &str = "JetBrains Mono";
const FONT: f32 = 34.0;
const CHAR_W: f32 = FONT * 0.6;
const LINE_H: f32 = 46.0;
/// World position of the first column and the first line on the source plane.
const TEXT_X: f32 = 170.0;
const GUTTER_X: f32 = 60.0;
const TOP_Y: f32 = 150.0;
/// The typing head is kept at or above this world height by scrolling.
const HEAD_Y: f32 = 700.0;

#[derive(Clone, Copy, PartialEq)]
enum Expansion {
    Group,
    Circle,
    Path,
    Tower,
}

struct Line {
    number: usize,
    text: &'static str,
    /// Characters typed before this line starts.
    start: usize,
    expansion: Option<Expansion>,
}

static LINES: LazyLock<Vec<Line>> = LazyLock::new(|| {
    let excerpt = source_excerpt();
    let offset = SCENE_SOURCE.find(excerpt).unwrap_or(0);
    let first = SCENE_SOURCE[..offset].lines().count() + 1;
    let mut start = 0;
    excerpt
        .lines()
        .enumerate()
        .map(|(k, text)| {
            let expansion = if text.contains("svgr!(<g") {
                Some(Expansion::Group)
            } else if text.contains("<circle id=") {
                Some(Expansion::Circle)
            } else if text.contains("<path id=") {
                Some(Expansion::Path)
            } else if text.contains("Node::group(format!(\"tower-") {
                Some(Expansion::Tower)
            } else {
                None
            };
            let line = Line { number: first + k, text, start, expansion };
            start += text.chars().count();
            line
        })
        .collect()
});

/// Characters typed in total, for the keystroke schedule.
pub fn char_count() -> usize {
    LINES.iter().map(|l| l.text.chars().count()).sum()
}

fn record(name: &str) -> Option<&'static NodeRecord> {
    NODES.iter().map(|(_, r)| r).find(|r| r.name == name)
}

/// The element a line produced, as the renderer emits it at second `t`.
fn expand(kind: Expansion, t: f32) -> String {
    match kind {
        Expansion::Group => {
            let (node, r) = NODES.iter().find(|(_, r)| r.name == "reflection").expect("reflection");
            format!("→ <g id=\"{}\" transform=\"{}\" opacity=\"{}\">", r.name, node.transform.to_svg(), node.opacity)
        }
        Expansion::Circle => {
            let r = record("sky/sun").expect("sun");
            let c = r.bbox.center();
            format!("→ <circle id=\"{}\" cx=\"{:.0}\" cy=\"{:.0}\" r=\"{:.0}\" fill=\"{}\"/>", r.name, c[0], c[1], r.bbox.w / 2.0, r.paint)
        }
        Expansion::Path => {
            let (node, r) = NODES.iter().find(|(_, r)| r.name == "tower-03/windows").expect("windows");
            let opacity = node.opacity * node.flicker.map_or(1.0, |s| beauty::scene::flicker(s, t));
            let d: String = node.path_data().chars().take(26).collect();
            format!("→ <path id=\"{}\" d=\"{d}…\" opacity=\"{opacity:.3}\"/>", r.name)
        }
        Expansion::Tower => {
            let r = record("tower-03").expect("tower");
            format!("→ {} {}  bbox {:.1}×{:.1}", r.name, r.transform.to_svg(), r.bbox.w, r.bbox.h)
        }
    }
}

const KEYWORDS: [&str; 12] = ["pub", "fn", "let", "mut", "match", "if", "else", "for", "in", "as", "move", "return"];

/// Splits a (possibly partially typed) line into coloured tokens.
fn highlight(text: &str) -> Vec<(String, &'static str)> {
    if text.trim_start().starts_with("//") {
        return vec![(text.to_owned(), palette::DIM)];
    }
    let mut out: Vec<(String, &'static str)> = Vec::new();
    let mut word = String::new();
    let mut in_string = false;
    let flush = |word: &mut String, out: &mut Vec<(String, &'static str)>| {
        if word.is_empty() {
            return;
        }
        let color = if KEYWORDS.contains(&word.as_str()) {
            palette::MAGENTA
        } else if word == "svgr!" || word == "vec!" || word == "format!" {
            palette::CYAN
        } else if word.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            "#ffd6f6"
        } else {
            "#cfe8ff"
        };
        out.push((std::mem::take(word), color));
    };
    for ch in text.chars() {
        if in_string {
            word.push(ch);
            if ch == '"' {
                out.push((std::mem::take(&mut word), palette::WHITE));
                in_string = false;
            }
        } else if ch == '"' {
            flush(&mut word, &mut out);
            word.push(ch);
            in_string = true;
        } else if ch.is_alphanumeric() || ch == '_' || ch == '!' || ch == '.' && word.chars().all(|c| c.is_ascii_digit()) && !word.is_empty() {
            word.push(ch);
        } else {
            flush(&mut word, &mut out);
            out.push((ch.to_string(), "#7f95b8"));
        }
    }
    if in_string {
        out.push((word, palette::WHITE));
    } else {
        flush(&mut word, &mut out);
    }
    out
}

fn text_row(x: f32, y: f32, tokens: Vec<(String, &'static str)>, opacity: f32) -> Svgr<'static> {
    let spans: Vec<Svgr> = tokens
        .into_iter()
        .map(|(text, fill)| svgr!(<tspan fill={fill}>{text}</tspan>))
        .collect();
    svgr!(
        <text x={x} y={y} font-family={MONO} font-size={FONT} font-weight="400"
            opacity={opacity}>{spans}</text>
    )
}

/// Where the typing head is: (line index, column) as a continuous position.
fn head(t: f32) -> (usize, usize) {
    let typed = schedule::typed_chars(t, char_count());
    let lines = &*LINES;
    let i = lines.iter().rposition(|l| l.start <= typed).unwrap_or(0);
    let col = (typed - lines[i].start).min(lines[i].text.chars().count());
    (i, col)
}

/// World y of each row, with expansions taking an extra row once they appear.
fn row_ys(t: f32) -> Vec<f32> {
    let mut y = TOP_Y;
    let (head_line, _) = head(t);
    LINES
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let row = y;
            y += LINE_H;
            if l.expansion.is_some() && i < head_line {
                y += LINE_H * expansion_open(i, t);
            }
            row
        })
        .collect()
}

/// When line `i` is complete, plus a beat.
fn line_done_at(i: usize) -> f32 {
    let end = LINES[i].start + LINES[i].text.chars().count();
    let total = char_count();
    // Invert the typing schedule numerically (it is monotonic).
    let steps = 400;
    (0..=steps)
        .map(|s| TYPE_WINDOW.0 + (TYPE_WINDOW.1 - TYPE_WINDOW.0) * s as f32 / steps as f32)
        .find(|&t| schedule::typed_chars(t, total) >= end)
        .unwrap_or(TYPE_WINDOW.1)
}

fn expansion_open(i: usize, t: f32) -> f32 {
    let at = line_done_at(i) + 0.05;
    smoothstep(at, at + 0.3, t)
}

/// Collapse progress into the cursor.
fn collapse(t: f32) -> f32 {
    smoothstep(TIMELINE.start(SceneLayer::Cursor), CURSOR_CLICK, t)
}

/// World-space scroll so the head stays in view.
fn scroll(t: f32, ys: &[f32]) -> f32 {
    let (line, col) = head(t);
    let len = LINES[line].text.chars().count().max(1) as f32;
    let next = ys.get(line + 1).copied().unwrap_or(ys[line] + LINE_H);
    let y = mix(ys[line], next, col as f32 / len);
    (y - HEAD_Y).max(0.0)
}

pub fn layer(cam: &Camera) -> Svgr<'static> {
    let t = cam.t;
    let weight = TIMELINE.weight(SceneLayer::Source, t).max(if t >= TIMELINE.start(SceneLayer::Cursor) { 1.0 - collapse(t) } else { 0.0 });
    if weight < 0.01 || t < TYPE_WINDOW.0 - 0.6 {
        return Svgr::empty();
    }
    let ys = row_ys(t);
    let scroll = scroll(t, &ys);
    let Some(plane) = cam.plane_matrix(0.0, -scroll, SOURCE_Z) else {
        return Svgr::empty();
    };
    let typed = schedule::typed_chars(t, char_count());
    let (head_line, head_col) = head(t);

    let mut rows = Vec::new();
    for (i, line) in LINES.iter().enumerate() {
        let y = ys[i];
        let shown = typed.saturating_sub(line.start).min(line.text.chars().count());
        if typed < line.start {
            break;
        }
        // Lines scrolling off the top fade out instead of being cut by the frame.
        let visible = smoothstep(TOP_Y - 40.0, TOP_Y + 160.0, y - scroll);
        if visible <= 0.0 {
            continue;
        }
        rows.push(svgr!(
            <text x={GUTTER_X} y={y} font-family={MONO} font-size="26" font-weight="300"
                fill={palette::DIM} opacity={0.7 * visible}>{format!("{:>3}", line.number)}</text>
        ));
        // Indentation becomes position: SVG collapses leading spaces.
        let partial: String = line.text.chars().take(shown).collect();
        let indent = partial.len() - partial.trim_start().len();
        rows.push(text_row(TEXT_X + indent as f32 * CHAR_W, y, highlight(partial.trim_start()), visible));

        if let Some(kind) = line.expansion {
            let open = expansion_open(i, t);
            if open > 0.0 {
                let full = expand(kind, t);
                let indent = line.text.len() - line.text.trim_start().len();
                let n = (full.chars().count() as f32 * open).ceil() as usize;
                let text: String = full.chars().take(n).collect();
                // Slides out from under its line as the rows below make room.
                rows.push(text_row(
                    TEXT_X + indent as f32 * CHAR_W,
                    y + LINE_H * open,
                    vec![(text, palette::MAGENTA)],
                    open * open * visible,
                ));
            }
        }
    }

    // Caret position in world units on the plane, then on screen.
    let caret_world = Vec3::new(TEXT_X + head_col as f32 * CHAR_W, ys[head_line] - scroll - FONT * 0.78, SOURCE_Z);
    let caret = cam.project(caret_world);
    let k = collapse(t);
    let (cx, cy, cs) = caret.map_or((960.0, 540.0, 0.8), |p| (p.x, p.y, p.scale));
    let pivot = (mix(cx, 960.0, k), mix(cy, 540.0, k));
    let shrink = 1.0 - k * 0.995;
    let collapse_matrix = format!(
        "matrix({shrink} 0 0 {shrink} {} {})",
        pivot.0 - cx * shrink,
        pivot.1 - cy * shrink
    );
    let plane_matrix = hud::Matrix(plane).to_svg();
    let caret_h = mix(FONT * 1.05 * cs, 44.0, k);
    let caret_w = mix(FONT * 0.6 * cs, 22.0, k);
    let caret_on = t < TYPE_WINDOW.1 || ((t * 3.0).fract() < 0.6);

    svgr!(
        <g opacity={weight}>
            <g transform={collapse_matrix} opacity={(1.0 - k).powi(2)}>
                <g transform={plane_matrix}>{rows}</g>
            </g>
            <rect x={pivot.0 - 11.0 * k} y={pivot.1 - 22.0 * k} width={caret_w} height={caret_h}
                fill={palette::CYAN} opacity={if caret_on && t < CURSOR_CLICK { 0.9 } else { 0.0 }} />
        </g>
    )
}

/// The last thing left: a blinking cursor on black.
pub fn cursor(cam: &Camera) -> Svgr<'static> {
    let t = cam.t;
    if t < CURSOR_CLICK {
        return Svgr::empty();
    }
    let on = schedule::cursor_on(t);
    // The click lands with a short flare that settles to the cursor's steady glow.
    let flare = 1.0 - smoothstep(CURSOR_CLICK, CURSOR_CLICK + 0.35, t);
    svgr!(
        <g opacity={if on { 1.0 } else { 0.0 }}>
            <rect x="949" y="518" width="22" height="44" fill={palette::CYAN} opacity={0.35 + 0.4 * flare}
                filter="url(#cursor-glow)" />
            <rect x="949" y="518" width="22" height="44" fill={palette::WHITE} />
            <filter id="cursor-glow" x="-200%" y="-100%" width="500%" height="300%">
                <feGaussianBlur stdDeviation={6.0 + 10.0 * flare} />
            </filter>
        </g>
    )
}
