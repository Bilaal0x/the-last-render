//! The soundtrack, synthesized from `tracks.json`. No samples, no files in.
//!
//! Beds follow the per-frame `AudioControls` (interpolated to the sample):
//!   Beauty   sub-bass + warm detuned-saw pad, low-pass closing as the camera pulls away
//!   Commands a faint crystalline shimmer
//!   SvgTree  a band-passed data stream gated by the nodes' path hashes
//!   Source   pure sine tones
//! Events are one-shots placed at their exact seconds, with their own pitch, pan,
//! gain and brightness: metallic pings per draw call, digital clicks per node,
//! soft key clicks per keystroke, and one last click for the cursor.
use hud::{EventKind, Tracks};
use std::f32::consts::{PI, TAU};
use std::path::Path;

pub const SAMPLE_RATE: u32 = 48_000;
const SR: f32 = SAMPLE_RATE as f32;
/// Controls are re-read every this many samples.
const BLOCK: usize = 32;
/// Loudness target of the mix as plain RMS (dBFS), and the ceiling for peaks.
const TARGET_RMS_DBFS: f32 = -18.2;
const CEILING: f32 = 0.85;

pub struct Stereo {
    pub left: Vec<f32>,
    pub right: Vec<f32>,
}

impl Stereo {
    fn new(n: usize) -> Self {
        Self { left: vec![0.0; n], right: vec![0.0; n] }
    }

    fn add(&mut self, i: usize, v: f32, pan: f32) {
        if i < self.left.len() {
            let (l, r) = equal_power(pan);
            self.left[i] += v * l;
            self.right[i] += v * r;
        }
    }
}

fn equal_power(pan: f32) -> (f32, f32) {
    let a = (pan.clamp(-1.0, 1.0) + 1.0) * PI / 4.0;
    (a.cos(), a.sin())
}

/// Small deterministic noise source.
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f32 / u32::MAX as f32 * 2.0 - 1.0
    }
}

/// Chamberlin state-variable filter.
#[derive(Default, Clone, Copy)]
struct Svf {
    low: f32,
    band: f32,
}

impl Svf {
    /// Returns (low, band, high).
    fn tick(&mut self, x: f32, cutoff: f32, q: f32) -> (f32, f32, f32) {
        let f = 2.0 * (PI * cutoff.clamp(20.0, SR * 0.2) / SR).sin();
        let damp = 1.0 / q.max(0.5);
        self.low += f * self.band;
        let high = x - self.low - damp * self.band;
        self.band += f * high;
        (self.low, self.band, high)
    }
}

/// Band-limited sawtooth (polyBLEP).
struct Saw {
    phase: f32,
}

impl Saw {
    fn tick(&mut self, hz: f32) -> f32 {
        let dt = hz / SR;
        self.phase = (self.phase + dt).fract();
        let t = self.phase;
        let mut v = 2.0 * t - 1.0;
        if t < dt {
            let x = t / dt;
            v -= x + x - x * x - 1.0;
        } else if t > 1.0 - dt {
            let x = (t - 1.0) / dt;
            v -= x * x + x + x + 1.0;
        }
        v
    }
}

/// Freeverb-style stereo reverb.
struct Reverb {
    combs: Vec<(Vec<f32>, usize, f32)>,
    allpasses: Vec<(Vec<f32>, usize)>,
}

impl Reverb {
    fn new(spread: usize) -> Self {
        let scale = SR / 44_100.0;
        let combs = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617]
            .map(|n| (vec![0.0; ((n + spread) as f32 * scale) as usize], 0, 0.0))
            .to_vec();
        let allpasses = [556, 441, 341, 225]
            .map(|n| (vec![0.0; ((n + spread) as f32 * scale) as usize], 0))
            .to_vec();
        Self { combs, allpasses }
    }

    fn tick(&mut self, x: f32, room: f32, damp: f32) -> f32 {
        let mut out = 0.0;
        for (buf, pos, store) in &mut self.combs {
            let y = buf[*pos];
            *store = y * (1.0 - damp) + *store * damp;
            buf[*pos] = x + *store * room;
            *pos = (*pos + 1) % buf.len();
            out += y;
        }
        for (buf, pos) in &mut self.allpasses {
            let b = buf[*pos];
            buf[*pos] = out + b * 0.5;
            out = b - out;
            *pos = (*pos + 1) % buf.len();
        }
        out * 0.12
    }
}

/// Warm E major add9 voicing for the pad: E2 B2 G#3 D#4 F#4.
const PAD: [f32; 5] = [82.41, 123.47, 207.65, 311.13, 369.99];

pub fn synthesize(tracks: &Tracks) -> Stereo {
    let n = (tracks.duration() * SR) as usize;
    let mut dry = Stereo::new(n);
    let mut send = Stereo::new(n);
    beds(tracks, &mut dry, &mut send);
    events(tracks, &mut dry, &mut send);
    master(tracks, dry, send)
}

fn beds(tracks: &Tracks, dry: &mut Stereo, send: &mut Stereo) {
    let n = dry.left.len();
    let mut saws: Vec<Saw> = (0..PAD.len() * 3).map(|k| Saw { phase: k as f32 * 0.137 % 1.0 }).collect();
    let mut pad_filter = [Svf::default(); 2];
    let mut stream_filter = Svf::default();
    let mut air_filter = Svf::default();
    let mut noise = Noise(0x5eed_1234);
    let mut held = 0.0f32;
    let mut sub_phase = 0.0f32;
    let mut sine_phase = [0.0f32; 3];
    let mut shimmer_phase = [0.0f32; 3];

    // The data stream's gate pattern: one bit per node hash, 24 steps per second.
    let bits: Vec<bool> = tracks.nodes.iter().flat_map(|r| (0..8).map(move |b| (r.path_hash >> (b * 7)) & 1 == 1)).collect();

    let mut c = tracks.audio_at(0.0);
    for i in 0..n {
        let t = i as f32 / SR;
        if i % BLOCK == 0 {
            c = tracks.audio_at(t);
        }

        // Beauty: sub-bass and pad.
        if c.pad_gain > 1e-4 {
            sub_phase = (sub_phase + 41.2 / SR).fract();
            let swell = 0.85 + 0.15 * (TAU * t / 6.0).sin();
            let sub = ((TAU * sub_phase).sin() + 0.18 * (2.0 * TAU * sub_phase).sin()) * 0.42 * swell;

            let mut l = 0.0;
            let mut r = 0.0;
            for (v, hz) in PAD.iter().enumerate() {
                for d in 0..3 {
                    let cents = (d as f32 - 1.0) * 7.0;
                    let s = saws[v * 3 + d].tick(hz * 2f32.powf(cents / 1200.0));
                    let pan = ((v * 3 + d) as f32 / 14.0) * 1.4 - 0.7;
                    let (pl, pr) = equal_power(pan);
                    l += s * pl;
                    r += s * pr;
                }
            }
            let wobble = 1.0 + 0.12 * (TAU * t * 0.23).sin();
            let (lo_l, _, _) = pad_filter[0].tick(l, c.pad_cutoff * wobble, 0.8);
            let (lo_r, _, _) = pad_filter[1].tick(r, c.pad_cutoff * wobble, 0.8);
            let g = c.pad_gain;
            let (pl, pr) = equal_power(c.pan);
            dry.left[i] += (sub + lo_l * 0.055 * pl * 1.4) * g;
            dry.right[i] += (sub + lo_r * 0.055 * pr * 1.4) * g;
            send.left[i] += lo_l * 0.04 * g;
            send.right[i] += lo_r * 0.04 * g;
        }

        // Commands: crystalline shimmer, inharmonic partials of E6 over thin air.
        if c.shimmer_gain > 1e-4 {
            let mut s = 0.0;
            for (k, ratio) in [1.0f32, 2.76, 5.40].iter().enumerate() {
                shimmer_phase[k] = (shimmer_phase[k] + 1318.51 * ratio / SR).fract();
                let lfo = 0.5 + 0.5 * (TAU * t * (0.31 + k as f32 * 0.17)).sin();
                s += (TAU * shimmer_phase[k]).sin() * lfo / (1.0 + k as f32);
            }
            let (_, _, air) = air_filter.tick(noise.next(), 7000.0, 0.7);
            let v = (s * 0.018 + air * 0.012) * c.shimmer_gain;
            send.add(i, v * 2.0, c.pan);
            dry.add(i, v, -c.pan);
        }

        // SvgTree: digital data stream.
        if c.stream_gain > 1e-4 && !bits.is_empty() {
            let step = (t * 24.0) as usize;
            let gate = if bits[step % bits.len()] { 1.0 } else { 0.15 };
            let sub_step = (t * 24.0).fract();
            let env = gate * (1.0 - sub_step).powf(3.0);
            // Sample-and-hold every 6 samples: a crushed, digital grain.
            if i % 6 == 0 {
                held = noise.next();
            }
            let (_, band, _) = stream_filter.tick(held, c.stream_hz, 4.0);
            let v = band * env * 0.45 * c.stream_gain;
            let pan = if step % 2 == 0 { -0.45 } else { 0.45 };
            dry.add(i, v, pan);
            send.add(i, v * 0.5, -pan);
        }

        // Source: pure sine tones, root, fifth and the octave below.
        if c.sine_gain > 1e-4 {
            let mut s = 0.0;
            for (k, (ratio, amp)) in [(1.0f32, 0.5f32), (1.5, 0.22), (0.5, 0.35)].iter().enumerate() {
                sine_phase[k] = (sine_phase[k] + c.sine_hz * ratio / SR).fract();
                s += (TAU * sine_phase[k]).sin() * amp;
            }
            let v = s * 0.26 * c.sine_gain;
            dry.add(i, v, 0.0);
            send.add(i, v * 0.6, 0.0);
        }
    }
}

fn events(tracks: &Tracks, dry: &mut Stereo, send: &mut Stereo) {
    let mut noise = Noise(0xc11c_4a11);
    for e in &tracks.events {
        let start = (e.t * SR) as usize;
        match e.kind {
            EventKind::DrawCall => {
                // FM bell with an inharmonic modulator: metallic, glassy.
                let len = (1.4 * SR) as usize;
                let mut lp = Svf::default();
                for k in 0..len {
                    let t = k as f32 / SR;
                    let index = 3.2 * (-t / 0.12).exp();
                    let m = (TAU * e.pitch_hz * 3.53 * t).sin() * index;
                    let bell = (TAU * e.pitch_hz * t + m).sin() + 0.35 * (TAU * e.pitch_hz * 2.76 * t).sin() * (-t / 0.2).exp();
                    let env = (1.0 - (-t / 0.002).exp()) * (-t / 0.38).exp();
                    let (lo, _, _) = lp.tick(bell * env, e.cutoff_hz, 0.7);
                    let v = lo * e.gain * 0.17;
                    dry.add(start + k, v, e.pan);
                    send.add(start + k, v * 1.6, e.pan);
                }
            }
            EventKind::NodeReveal => {
                let len = (0.05 * SR) as usize;
                let mut bp = Svf::default();
                for k in 0..len {
                    let t = k as f32 / SR;
                    let burst = noise.next() * (-t / 0.0025).exp();
                    let (_, band, _) = bp.tick(burst, e.cutoff_hz, 3.0);
                    let blip = (TAU * e.pitch_hz * t).sin() * (-t / 0.012).exp() * 0.4;
                    let v = (band * 1.4 + blip) * e.gain * 1.1;
                    dry.add(start + k, v, e.pan);
                    send.add(start + k, v * 0.4, e.pan);
                }
            }
            EventKind::KeyStroke => {
                // Key down: a short bright tick; body: a soft low thock.
                let len = (0.09 * SR) as usize;
                let mut hp = Svf::default();
                for k in 0..len {
                    let t = k as f32 / SR;
                    let (_, _, tick) = hp.tick(noise.next() * (-t / 0.0018).exp(), e.cutoff_hz, 0.8);
                    let thock = (TAU * 170.0 * t).sin() * (-t / 0.03).exp();
                    let ring = (TAU * e.pitch_hz * t).sin() * (-t / 0.008).exp() * 0.25;
                    let v = (tick * 0.8 + thock * 0.5 + ring) * e.gain * 0.8;
                    dry.add(start + k, v, e.pan);
                    send.add(start + k, v * 0.3, e.pan);
                }
            }
            EventKind::CursorClick => {
                let len = (0.25 * SR) as usize;
                let mut lp = Svf::default();
                for k in 0..len {
                    let t = k as f32 / SR;
                    let tick = noise.next() * (-t / 0.0012).exp();
                    let (lo, _, _) = lp.tick(tick, e.cutoff_hz, 0.7);
                    let tone = (TAU * e.pitch_hz * t).sin() * (-t / 0.018).exp() * 0.5;
                    let thump = (TAU * 92.0 * t).sin() * (-t / 0.06).exp() * 0.7;
                    let v = (lo * 1.2 + tone + thump) * e.gain;
                    dry.add(start + k, v, e.pan);
                    send.add(start + k, v * 0.25, e.pan);
                }
            }
        }
    }
}

fn master(tracks: &Tracks, dry: Stereo, send: Stereo) -> Stereo {
    let n = dry.left.len();
    let mut rev_l = Reverb::new(0);
    let mut rev_r = Reverb::new(23);
    // After the cursor's click the room empties: the tail is gated to silence.
    let click = tracks
        .events
        .iter()
        .find(|e| e.kind == EventKind::CursorClick)
        .map_or(tracks.duration(), |e| e.t);
    let mut out = Stereo::new(n);
    let mut hp = [Svf::default(); 2];
    for i in 0..n {
        let t = i as f32 / SR;
        let wet_l = rev_l.tick(send.left[i] + send.right[i] * 0.2, 0.86, 0.35);
        let wet_r = rev_r.tick(send.right[i] + send.left[i] * 0.2, 0.86, 0.35);
        let tail = 1.0 - hud::smoothstep(click + 0.25, click + 0.75, t);
        let fade_in = hud::smoothstep(0.0, 0.6, t);
        let mut l = (dry.left[i] + wet_l * tail) * fade_in;
        let mut r = (dry.right[i] + wet_r * tail) * fade_in;
        // DC / rumble guard below the sub-bass.
        l = hp[0].tick(l, 24.0, 0.7).2;
        r = hp[1].tick(r, 24.0, 0.7).2;
        out.left[i] = l;
        out.right[i] = r;
    }
    // Level by RMS over the film, then round off the few peaks above the ceiling.
    let energy: f32 = out.left.iter().chain(&out.right).map(|v| v * v).sum();
    let rms = (energy / (2 * n) as f32).sqrt().max(1e-6);
    let gain = 10f32.powf(TARGET_RMS_DBFS / 20.0) / rms;
    out.left.iter_mut().chain(out.right.iter_mut()).for_each(|v| *v = soft_clip(*v * gain));
    out
}

fn soft_clip(x: f32) -> f32 {
    let knee = CEILING * 0.8;
    if x.abs() < knee { x } else { x.signum() * (knee + (CEILING - knee) * ((x.abs() - knee) / (CEILING - knee)).tanh()) }
}

/// 16-bit PCM stereo WAV.
pub fn write_wav(audio: &Stereo, path: impl AsRef<Path>) -> std::io::Result<()> {
    let frames = audio.left.len();
    let data_len = (frames * 4) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..frames {
        for s in [audio.left[i], audio.right[i]] {
            let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    if let Some(dir) = path.as_ref().parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, out)
}
