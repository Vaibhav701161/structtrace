#!/usr/bin/env python3
"""Capture the real product, synthesize disclosed narration, and render launch deliverables."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
from pathlib import Path


def run(command: list[str]) -> None:
    subprocess.run(command, check=True)


def duration_seconds(path: Path) -> float:
    return float(subprocess.check_output([
        "ffprobe", "-v", "error", "-show_entries", "format=duration",
        "-of", "default=noprint_wrappers=1:nokey=1", str(path),
    ], text=True).strip())


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--url", required=True, help="Capability-protected StructTrace Local URL")
    parser.add_argument("--output", default="demo/video/generated/launch")
    parser.add_argument("--skip-capture", action="store_true")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    output = (root / args.output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    if not args.skip_capture:
        os.environ["STRUCTTRACE_UI_URL"] = args.url
        run([
            "node", str(root / "demo/video/record-launch-walkthrough.mjs"), str(output)
        ])
    for executable in ["ffmpeg", "ffprobe", "edge-tts"]:
        if shutil.which(executable) is None:
            raise SystemExit(f"required executable is unavailable: {executable}")
    manifest = json.loads((root / "demo/video/launch-production-manifest.json").read_text())
    narration = manifest["narration"]
    audio = output / "narration.mp3"
    captions = output / "structtrace-launch-walkthrough.vtt"
    run([
        "edge-tts", "--voice", narration["voice"], "--rate", narration["rate"],
        "--pitch", narration["pitch"], "--file", str(root / narration["source"]),
        "--write-media", str(audio), "--write-subtitles", str(captions),
    ])
    raw = output / "structtrace-launch-walkthrough.webm"
    master = output / "structtrace-launch-walkthrough-master.mp4"
    raw_duration = duration_seconds(raw)
    narration_duration = duration_seconds(audio)
    video_stretch = narration_duration / raw_duration
    run([
        "ffmpeg", "-y", "-i", str(raw), "-i", str(audio),
        "-filter_complex", f"[0:v]setpts={video_stretch:.8f}*PTS[v];[1:a]loudnorm=I=-16:TP=-1.5:LRA=9[a]",
        "-map", "[v]", "-map", "[a]", "-c:v", "libx264", "-preset", "slow",
        "-crf", "16", "-pix_fmt", "yuv420p", "-r", "30", "-c:a", "aac",
        "-b:a", "192k", "-ar", "48000", "-shortest", "-movflags", "+faststart",
        str(master),
    ])
    launch_cut = output / "structtrace-launch-walkthrough-captioned.mp4"
    subtitle_filter = f"subtitles=filename='{captions.as_posix()}':force_style='FontName=DejaVu Sans,FontSize=8,PrimaryColour=&H00FFFDF8,OutlineColour=&H00181A17,BorderStyle=3,Outline=1,Shadow=0,MarginV=24'"
    run([
        "ffmpeg", "-y", "-i", str(master), "-vf", subtitle_filter,
        "-c:v", "libx264", "-preset", "slow", "-crf", "18", "-pix_fmt", "yuv420p",
        "-c:a", "copy", "-movflags", "+faststart", str(launch_cut),
    ])
    duration = duration_seconds(master)
    (output / "render-receipt.json").write_text(json.dumps({
        "video": master.name, "captioned_video": launch_cut.name, "captions": captions.name,
        "duration_seconds": duration,
        "raw_capture_duration_seconds": raw_duration,
        "narration_duration_seconds": narration_duration,
        "video_stretch_factor": video_stretch,
        "production_manifest": manifest,
    }, indent=2) + "\n")


if __name__ == "__main__":
    main()
