//! `the-last-render`: writes the passes, then hands the film to the fframes CLI.
//!
//! 1. `out/tracks.json`       timeline, camera, NodeRecords and sound events
//! 2. `out/passes/beauty.png` the beauty pass baked to pixels (the shader's iChannel0)
//! 3. `out/passes/score.wav`  the soundtrack, synthesized from tracks.json
//! 4. the fframes CLI: render, preview, frame, strip, inspect, audio ...
use composite::{BAKE_AT, BEAUTY_PNG, PASS_DIR, SCORE_WAV, StaticMedia, TRACKS_PATH, TheLastRender};
use fframes::{
    CombinedMediaProvider, CpuFrameRenderer, EncoderOptions, MediaDirectory, MediaProvider, Previewer,
    RenderOptions, StaticMediaProvider, cli,
};
use fframes_skia_renderer::{SkiaFFramesRenderer, SkiaPipelineConcurrencyPolicy, SkiaPipelineConfig};
use hud::Tracks;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("the-last-render: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let passes = Path::new(PASS_DIR);
    std::fs::create_dir_all(passes)?;

    // 1. One source of truth for picture and sound.
    let nodes = beauty::records();
    Tracks::build(nodes.clone(), source::char_count()).write(TRACKS_PATH)?;

    // 2. Pass 1 to pixels, so the command-buffer shader can take it apart.
    bake_beauty(&passes.join(BEAUTY_PNG))?;

    // 3. The score reads the file back, like any other consumer of the tracks.
    let tracks = Tracks::read(TRACKS_PATH)?;
    sfx::write_wav(&sfx::synthesize(&tracks), passes.join(SCORE_WAV))?;

    let static_media = StaticMedia::prepare()?;
    let pass_dir = MediaDirectory::read_folder(passes)?;
    let pass_media = pass_dir.process_media_source()?;
    let media = CombinedMediaProvider::from([&static_media as &dyn MediaProvider, &pass_media]);

    let video = TheLastRender::new(nodes, true);
    let options = RenderOptions {
        media: Some(&media),
        default_font: commands::MONO,
        video_encoder_options: EncoderOptions {
            preferred_encoder: Some("libx264"),
            codec_params: Some(&[("crf", "16"), ("preset", "slow"), ("tune", "film")]),
            ..Default::default()
        },
        ..Default::default()
    };
    let config = SkiaPipelineConfig {
        concurrency_policy: SkiaPipelineConcurrencyPolicy::MaxPerformance,
        ..Default::default()
    };

    #[cfg(target_os = "macos")]
    let gpu = fframes_skia_renderer::metal::SkiaMetalCtx::new(hud::WIDTH, hud::HEIGHT)?;
    #[cfg(target_os = "macos")]
    let backend = SkiaFFramesRenderer::new_metal(&gpu, config)?;
    #[cfg(not(target_os = "macos"))]
    let gpu = fframes_skia_renderer::vulkan::SkiaVulkanCtx::new(hud::WIDTH, hud::HEIGHT)?;
    #[cfg(not(target_os = "macos"))]
    let backend = SkiaFFramesRenderer::new_vulkan(&gpu, config)?;

    Ok(cli::new(&video, options)
        .backend(backend)
        .preview(fframes_native_player::cli_preview)
        .default_output("out/the-last-render.mp4")
        .run())
}

/// Renders the beauty pass at `BAKE_AT` on the CPU at half size.
fn bake_beauty(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let still = beauty::BeautyStill { t: BAKE_AT };
    let options = RenderOptions { scale_resolution: 0.5, ..Default::default() };
    let mut previewer = Previewer::new(&still, &options)?;
    let (pixels, _) = previewer.render_inspected(0, &mut CpuFrameRenderer::default())?;
    pixels.save_png(path)?;
    Ok(())
}
