#!/usr/bin/env bash
# The Last Render, start to finish:
#   out/tracks.json        timeline, camera, NodeRecords, sound events
#   out/passes/beauty.png  pass 1 baked to pixels (iChannel0 of the SkSL pipeline)
#   out/passes/score.wav   the synthesized soundtrack
#   out/the-last-render.mp4
set -euo pipefail
cd "$(dirname "$0")/.."

OUT="${1:-out/the-last-render.mp4}"
run() { cargo run --release --quiet --bin the-last-render -- "$@"; }

cargo build --release --bin the-last-render
run timeline
run inspect --fail-on warning
run audio analyze
run render -o "$OUT"

if command -v ffprobe >/dev/null 2>&1; then
  ffprobe -v error -count_frames \
    -show_entries stream=codec_type,codec_name,width,height,r_frame_rate,nb_read_frames,duration \
    -of compact=p=0 "$OUT"
  frames=$(ffprobe -v error -select_streams v:0 -count_frames \
    -show_entries stream=nb_read_frames -of csv=p=0 "$OUT")
  if [ "$frames" != "360" ]; then
    echo "expected 360 frames, got $frames" >&2
    exit 1
  fi
fi
echo "-> $OUT"
