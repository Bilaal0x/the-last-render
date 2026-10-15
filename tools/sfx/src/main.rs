//! `cargo run --release -p sfx -- [tracks.json] [out.wav]`: the soundtrack on its own.
use hud::Tracks;

fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let input = args.next().unwrap_or_else(|| "out/tracks.json".into());
    let output = args.next().unwrap_or_else(|| "out/passes/score.wav".into());
    let tracks = Tracks::read(&input)?;
    sfx::write_wav(&sfx::synthesize(&tracks), &output)?;
    eprintln!("{input} -> {output} ({} events)", tracks.events.len());
    Ok(())
}
