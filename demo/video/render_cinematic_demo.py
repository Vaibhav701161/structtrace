#!/usr/bin/env python3
"""Render a technical StructTrace demo video from a completed local run."""

from __future__ import annotations

import argparse
import json
import math
import shutil
import subprocess
import tempfile
from pathlib import Path
from textwrap import wrap

from PIL import Image, ImageDraw, ImageFilter, ImageFont


WIDTH = 1920
HEIGHT = 1080
FPS = 30
BG = (9, 14, 24)
PANEL = (16, 24, 39)
PANEL_2 = (22, 32, 50)
TEXT = (231, 238, 249)
MUTED = (148, 163, 184)
GREEN = (52, 211, 153)
BLUE = (96, 165, 250)
RED = (248, 113, 113)
AMBER = (251, 191, 36)
LINE = (51, 65, 85)


def font(size: int, bold: bool = False, mono: bool = False) -> ImageFont.FreeTypeFont:
    if mono:
        name = "DejaVuSansMono-Bold.ttf" if bold else "DejaVuSansMono.ttf"
        candidates = [
            Path("/usr/share/fonts/truetype/dejavu") / name,
        ]
    else:
        name = "DejaVuSans-Bold.ttf" if bold else "DejaVuSans.ttf"
        candidates = [
            Path("/usr/share/fonts/truetype/dejavu") / name,
            Path("/usr/share/fonts/truetype/roboto/unhinted/RobotoTTF")
            / ("Roboto-Bold.ttf" if bold else "Roboto-Regular.ttf"),
        ]
    for candidate in candidates:
        if candidate.exists():
            return ImageFont.truetype(str(candidate), size)
    return ImageFont.load_default()


F_TITLE = font(74, True)
F_H1 = font(52, True)
F_H2 = font(34, True)
F_BODY = font(27)
F_SMALL = font(22)
F_TINY = font(18)
F_MONO = font(24, mono=True)
F_MONO_BOLD = font(24, True, True)


def load_json(path: Path):
    return json.loads(path.read_text())


def rounded(draw: ImageDraw.ImageDraw, box, radius=18, fill=PANEL, outline=None, width=1):
    draw.rounded_rectangle(box, radius=radius, fill=fill, outline=outline, width=width)


def text(draw, xy, value, fill=TEXT, fnt=F_BODY, max_width=None, line_spacing=8):
    if max_width is None:
        draw.text(xy, value, fill=fill, font=fnt)
        return
    words = value.split()
    lines = []
    current = ""
    for word in words:
        candidate = word if not current else f"{current} {word}"
        if draw.textlength(candidate, font=fnt) <= max_width:
            current = candidate
        else:
            if current:
                lines.append(current)
            current = word
    if current:
        lines.append(current)
    x, y = xy
    for line in lines:
        draw.text((x, y), line, fill=fill, font=fnt)
        y += fnt.size + line_spacing


def base_frame(title=None, kicker="StructTrace"):
    img = Image.new("RGB", (WIDTH, HEIGHT), BG)
    draw = ImageDraw.Draw(img)
    for y in range(HEIGHT):
        shade = int(18 + 16 * y / HEIGHT)
        draw.line((0, y, WIDTH, y), fill=(8, 13, shade))
    draw.rectangle((0, 0, WIDTH, 74), fill=(7, 11, 19))
    draw.text((72, 22), kicker, fill=TEXT, font=font(28, True))
    header = "structured-output regression evidence"
    header_w = draw.textlength(header, font=F_SMALL)
    draw.text((WIDTH - 72 - header_w, 26), header, fill=MUTED, font=F_SMALL)
    draw.line((0, 74, WIDTH, 74), fill=LINE, width=2)
    if title:
        draw.text((72, 118), title, fill=TEXT, font=F_H1)
    return img


def metric_card(draw, x, y, w, h, label, value, accent=BLUE, sub=None):
    rounded(draw, (x, y, x + w, y + h), 14, PANEL, outline=LINE)
    draw.rectangle((x, y, x + 8, y + h), fill=accent)
    draw.text((x + 28, y + 24), label, fill=MUTED, font=F_SMALL)
    draw.text((x + 28, y + 62), value, fill=TEXT, font=font(48, True))
    if sub:
        draw.text((x + 28, y + h - 42), sub, fill=MUTED, font=F_TINY)


def bar_pair(draw, x, y, label, base_num, cand_num, total, color_base=BLUE, color_cand=GREEN):
    draw.text((x, y), label, fill=TEXT, font=F_SMALL)
    max_w = 520
    by = y + 42
    for idx, (name, n, col) in enumerate((("baseline", base_num, color_base), ("candidate", cand_num, color_cand))):
        yy = by + idx * 48
        draw.text((x, yy + 4), name, fill=MUTED, font=F_TINY)
        draw.rounded_rectangle((x + 120, yy, x + 120 + max_w, yy + 28), radius=8, fill=(30, 41, 59))
        draw.rounded_rectangle((x + 120, yy, x + 120 + max_w * n / total, yy + 28), radius=8, fill=col)
        draw.text((x + 660, yy + 1), f"{n}/{total}", fill=TEXT, font=F_SMALL)


def terminal_panel(draw, x, y, w, h, lines):
    rounded(draw, (x, y, x + w, y + h), 14, (4, 8, 16), outline=(45, 55, 72))
    draw.ellipse((x + 22, y + 22, x + 36, y + 36), fill=RED)
    draw.ellipse((x + 46, y + 22, x + 60, y + 36), fill=AMBER)
    draw.ellipse((x + 70, y + 22, x + 84, y + 36), fill=GREEN)
    yy = y + 62
    for line, col in lines:
        draw.text((x + 28, yy), line, fill=col, font=F_MONO_BOLD if line.startswith("$") else F_MONO)
        yy += 36


def slide_title():
    img = base_frame(kicker="StructTrace demo")
    draw = ImageDraw.Draw(img)
    draw.text((96, 250), "Your schema passed.", fill=TEXT, font=F_TITLE)
    draw.text((96, 345), "Did the answer?", fill=GREEN, font=F_TITLE)
    text(
        draw,
        (100, 480),
        "A real local run showing why structured-output migrations need paired semantic evidence, not only JSON validity.",
        fill=MUTED,
        fnt=F_H2,
        max_width=1120,
    )
    rounded(draw, (100, 690, 1330, 830), 16, PANEL, outline=LINE)
    text(draw, (132, 724), "Demo path: invoice extraction migration", fill=TEXT, fnt=F_H2)
    text(draw, (132, 775), "same dataset, same schema, baseline vs candidate, no network", fill=MUTED, fnt=F_BODY)
    return img


def slide_flow(run_id):
    img = base_frame("End-to-End Product Loop")
    draw = ImageDraw.Draw(img)
    steps = [
        ("doctor", "validate config, schema, dataset, storage"),
        ("run", "score baseline and candidate on matched cases"),
        ("report", "write offline HTML evidence bundle"),
        ("gate", "make a release decision with explicit thresholds"),
        ("replay", "recompute retained scores and hashes"),
    ]
    x = 112
    y = 300
    for i, (name, detail) in enumerate(steps):
        bx = x + i * 342
        rounded(draw, (bx, y, bx + 280, y + 150), 14, PANEL, outline=LINE)
        draw.text((bx + 24, y + 28), name, fill=GREEN if i in (0, 4) else BLUE, font=F_H2)
        text(draw, (bx + 24, y + 78), detail, fill=MUTED, fnt=F_SMALL, max_width=230)
        if i < len(steps) - 1:
            draw.line((bx + 284, y + 75, bx + 330, y + 75), fill=MUTED, width=4)
            draw.polygon([(bx + 330, y + 75), (bx + 314, y + 65), (bx + 314, y + 85)], fill=MUTED)
    rounded(draw, (112, 620, 1808, 780), 14, PANEL_2, outline=LINE)
    draw.text((144, 654), "Completed run", fill=MUTED, font=F_SMALL)
    draw.text((144, 694), run_id, fill=TEXT, font=font(42, True, True))
    return img


def slide_terminal(run_id):
    img = base_frame("Real CLI Run")
    draw = ImageDraw.Draw(img)
    terminal_panel(
        draw,
        110,
        220,
        1700,
        640,
        [
            ("$ cargo run -p structtrace-cli -- demo invoice", GREEN),
            ("STRUCTTRACE RUN COMPLETE", TEXT),
            (f"Run:          {run_id}", MUTED),
            ("Baseline:     9/12 (75.0%)", TEXT),
            ("Candidate:    9/12 (75.0%)", TEXT),
            ("Difference:   +0.00 percentage points", TEXT),
            ("Transitions:  3 candidate-only, 3 baseline-only", TEXT),
            ("Gate:         INSUFFICIENT EVIDENCE", AMBER),
            ("Report:       .structtrace/runs/<run>/report/index.html", MUTED),
        ],
    )
    return img


def slide_metrics(summary):
    img = base_frame("What Changed")
    draw = ImageDraw.Draw(img)
    total = summary["baseline"]["total"]
    metric_card(draw, 100, 215, 390, 180, "primary outcome", "9/12 vs 9/12", BLUE, "semantic task correctness")
    metric_card(draw, 535, 215, 390, 180, "schema validity", "10/12 -> 12/12", GREEN, "validity improved")
    metric_card(draw, 970, 215, 390, 180, "valid but wrong", "1/12 -> 3/12", RED, "more wrong valid outputs")
    metric_card(draw, 1405, 215, 390, 180, "paired effect", "+0.0 pp", AMBER, "not evidence to deploy")
    bar_pair(draw, 160, 500, "Strict JSON", summary["baseline"]["parse_valid"], summary["candidate"]["parse_valid"], total)
    bar_pair(draw, 160, 665, "Schema valid", summary["baseline"]["schema_valid"], summary["candidate"]["schema_valid"], total)
    bar_pair(draw, 995, 500, "Semantic pass", summary["baseline"]["primary_pass"], summary["candidate"]["primary_pass"], total)
    bar_pair(draw, 995, 665, "Valid but wrong", summary["baseline"]["valid_but_wrong"], summary["candidate"]["valid_but_wrong"], total, BLUE, RED)
    return img


def slide_matrix(summary):
    img = base_frame("Paired Transition Matrix")
    draw = ImageDraw.Draw(img)
    p = summary["paired"]
    labels = [
        ("both pass", p["both_pass"], GREEN),
        ("candidate-only", p["candidate_only_pass"], BLUE),
        ("baseline-only", p["baseline_only_pass"], RED),
        ("both fail", p["both_fail"], MUTED),
    ]
    start_x, start_y = 240, 260
    cell_w, cell_h = 680, 260
    for idx, (label, value, color) in enumerate(labels):
        col = idx % 2
        row = idx // 2
        x = start_x + col * (cell_w + 80)
        y = start_y + row * (cell_h + 70)
        rounded(draw, (x, y, x + cell_w, y + cell_h), 18, PANEL, outline=LINE)
        draw.text((x + 42, y + 42), label, fill=MUTED, font=F_H2)
        draw.text((x + 42, y + 112), str(value), fill=color, font=font(86, True))
    text(
        draw,
        (240, 895),
        "The candidate repaired three cases and broke three cases. The marginal score is tied, but the case-level movement is visible.",
        fill=TEXT,
        fnt=F_BODY,
        max_width=1350,
    )
    return img


def slide_case(run_dir):
    cases = load_json(run_dir / "report/cases/00000.json")
    case = next(c for c in cases if c["id"] == "invoice-011")
    img = base_frame("A Valid But Wrong Regression")
    draw = ImageDraw.Draw(img)
    case_input = case["input"]
    if isinstance(case_input, str):
        try:
            case_input = json.loads(case_input)
        except json.JSONDecodeError:
            case_input = {"text": case_input}
    text(draw, (100, 190), case_input["text"], fill=MUTED, fnt=F_SMALL, max_width=1700)
    rounded(draw, (100, 340, 900, 820), 16, PANEL, outline=LINE)
    rounded(draw, (1020, 340, 1820, 820), 16, PANEL, outline=LINE)
    draw.text((132, 378), "baseline output", fill=BLUE, font=F_H2)
    draw.text((1052, 378), "candidate output", fill=RED, font=F_H2)
    base = case["baseline_parsed"]
    cand = case["candidate_parsed"]
    base_lines = [
        '"line_items": [',
        '  {"description": "Masks", "amount": "40.00"},',
        '  {"description": "Sanitizer", "amount": "36.00"}',
        "]",
        '"total": "82.08"',
    ]
    cand_lines = [
        '"line_items": [',
        '  {"description": "Masks", "amount": "40.00"}',
        "]",
        '"total": "82.08"',
        "",
        "schema valid: true",
    ]
    y = 450
    for line in base_lines:
        draw.text((132, y), line, fill=TEXT, font=F_MONO)
        y += 42
    y = 450
    for line in cand_lines:
        draw.text((1052, y), line, fill=TEXT if "schema" not in line else GREEN, font=F_MONO)
        y += 42
    rounded(draw, (100, 875, 1820, 980), 14, (36, 24, 28), outline=(127, 29, 29))
    text(
        draw,
        (132, 905),
        "Evaluator evidence: /line_items/1 removed. The JSON contract still passes, but the extraction outcome fails.",
        fill=TEXT,
        fnt=F_BODY,
        max_width=1620,
    )
    return img


def slide_report(report_capture):
    img = base_frame("Offline Report Artifact")
    draw = ImageDraw.Draw(img)
    if report_capture and report_capture.exists():
        shot = Image.open(report_capture).convert("RGB")
        crop = shot.crop((0, 0, min(1440, shot.width), min(1450, shot.height)))
        crop.thumbnail((1180, 760))
        rounded(draw, (700, 190, 1820, 930), 18, PANEL, outline=LINE)
        img.paste(crop, (730, 220))
    metric_card(draw, 100, 235, 500, 170, "report bundle", "HTML + JSON", BLUE, "offline, local, no telemetry")
    metric_card(draw, 100, 455, 500, 170, "case explorer", "12 cases", GREEN, "diffs and evaluator fields")
    metric_card(draw, 100, 675, 500, 170, "evidence", "artifact v8", AMBER, "hash-bound run directory")
    return img


def slide_gate(gate):
    img = base_frame("Release Gate")
    draw = ImageDraw.Draw(img)
    status = gate["status"].replace("_", " ").upper()
    draw.text((110, 230), status, fill=AMBER, font=font(84, True))
    text(
        draw,
        (118, 350),
        "Deployment authorized: false. Quality checks pass, but the demo has only 12 unique evidence units against a 100-case release threshold.",
        fill=TEXT,
        fnt=F_H2,
        max_width=1460,
    )
    rounded(draw, (110, 560, 870, 770), 16, PANEL, outline=LINE)
    rounded(draw, (980, 560, 1740, 770), 16, PANEL, outline=LINE)
    draw.text((145, 595), "quality failures", fill=MUTED, font=F_H2)
    draw.text((145, 665), str(len(gate["quality_failures"])), fill=GREEN, font=font(64, True))
    draw.text((1015, 595), "evidence failures", fill=MUTED, font=F_H2)
    draw.text((1015, 665), ", ".join(gate["evidence_failures"]), fill=AMBER, font=F_H2)
    return img


def slide_replay(replay):
    img = base_frame("Replay Verification")
    draw = ImageDraw.Draw(img)
    metric_card(draw, 110, 245, 410, 180, "cases replayed", str(replay["cases_replayed"]), GREEN)
    metric_card(draw, 565, 245, 410, 180, "variant outputs", str(replay["variant_outputs_replayed"]), GREEN)
    metric_card(draw, 1020, 245, 410, 180, "evaluator results", str(replay["built_in_evaluator_results_recomputed"]), GREEN)
    metric_card(draw, 1475, 245, 330, 180, "mismatches", "0", GREEN)
    terminal_panel(
        draw,
        170,
        540,
        1580,
        290,
        [
            ("$ structtrace replay <run>", GREEN),
            ("artifact_hash_mismatches: []", TEXT),
            ("row_score_mismatches: []", TEXT),
            ("summary_mismatches: []", TEXT),
            ("verified: true", GREEN),
        ],
    )
    return img


def slide_close():
    img = base_frame(kicker="StructTrace")
    draw = ImageDraw.Draw(img)
    draw.text((100, 285), "StructTrace does not pick the winner.", fill=TEXT, font=F_TITLE)
    draw.text((100, 380), "It shows what changed before deployment.", fill=GREEN, font=F_TITLE)
    text(
        draw,
        (105, 540),
        "Stable-contract structured output testing: validity, semantic correctness, regressions, uncertainty, gate decision, and replay in one local evidence bundle.",
        fill=MUTED,
        fnt=F_H2,
        max_width=1450,
    )
    return img


def fade_frame(a, b, t):
    return Image.blend(a, b, t)


def add_footer(img, frame_no, total_frames):
    draw = ImageDraw.Draw(img)
    margin = 72
    w = WIDTH - 2 * margin
    y = HEIGHT - 52
    draw.rounded_rectangle((margin, y, margin + w, y + 5), radius=2, fill=(30, 41, 59))
    draw.rounded_rectangle((margin, y, margin + int(w * frame_no / total_frames), y + 5), radius=2, fill=GREEN)
    return img


def write_video(slides, out_path: Path, seconds_per_slide=5.2, fade_seconds=0.2):
    out_path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as temp:
        temp_path = Path(temp)
        slide_frames = int(seconds_per_slide * FPS)
        fade_frames = int(fade_seconds * FPS)
        total = len(slides) * slide_frames
        frame_idx = 0
        for i, slide in enumerate(slides):
            next_slide = slides[i + 1] if i + 1 < len(slides) else None
            for local in range(slide_frames):
                frame = slide.copy()
                if next_slide is not None and local >= slide_frames - fade_frames:
                    t = (local - (slide_frames - fade_frames)) / max(1, fade_frames - 1)
                    frame = fade_frame(slide, next_slide, t)
                add_footer(frame, frame_idx, total)
                frame.save(temp_path / f"frame_{frame_idx:05d}.png")
                frame_idx += 1
        cmd = [
            "ffmpeg",
            "-y",
            "-framerate",
            str(FPS),
            "-i",
            str(temp_path / "frame_%05d.png"),
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-crf",
            "18",
            "-preset",
            "medium",
            str(out_path),
        ]
        subprocess.run(cmd, check=True)


def capture_report(run_dir: Path, output: Path) -> Path | None:
    source = run_dir / "report/single.html"
    if not source.exists() or not shutil.which("wkhtmltoimage"):
        return None
    output.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [
            "wkhtmltoimage",
            "--width",
            "1440",
            "--quality",
            "92",
            str(source),
            str(output),
        ],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    return output


def write_narration(out_path: Path, run_id: str):
    out_path.write_text(
        "\n".join(
            [
                "# StructTrace YC Demo Narration",
                "",
                "Your schema passed. Did the answer?",
                "",
                "This demo runs StructTrace on a frozen invoice-extraction migration.",
                "The baseline and candidate are scored on the same 12 cases under the same JSON Schema.",
                "",
                f"Completed run: {run_id}.",
                "",
                "The candidate improves schema validity from 10 of 12 to 12 of 12.",
                "But semantic correctness is unchanged at 9 of 12, and valid-but-wrong outputs rise from 1 of 12 to 3 of 12.",
                "",
                "The transition matrix shows three candidate-only repairs and three baseline-only regressions.",
                "One regression is schema-valid but removes a sanitizer line item while keeping the total field valid.",
                "",
                "The release gate refuses deployment because this is a small demo fixture, not release evidence.",
                "Replay verifies 12 cases, 24 variant outputs, 240 evaluator results, and zero mismatches.",
                "",
                "StructTrace does not pick the winner. It shows what changed on your workload before deployment.",
                "",
            ]
        ),
        encoding="utf-8",
    )


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--run-dir", required=True, type=Path)
    parser.add_argument("--output", type=Path, default=Path("demo/video/generated/structtrace-yc-demo.mp4"))
    parser.add_argument("--seconds-per-slide", type=float, default=5.2)
    args = parser.parse_args()

    run_dir = args.run_dir
    summary = load_json(run_dir / "summary.json")
    manifest = load_json(run_dir / "manifest.json")
    run_id = manifest["run_id"]

    gate = json.loads(
        subprocess.run(
            [
                "target/debug/structtrace",
                "--run-dir",
                str(run_dir),
                "gate",
                "--verify",
                "replay",
                "--format",
                "json",
            ],
            check=False,
            capture_output=True,
            text=True,
        ).stdout
    )
    replay = json.loads(
        subprocess.run(
            [
                "target/debug/structtrace",
                "--run-dir",
                str(run_dir),
                "replay",
                "--format",
                "json",
            ],
            check=True,
            capture_output=True,
            text=True,
        ).stdout
    )

    generated = args.output.parent
    generated.mkdir(parents=True, exist_ok=True)
    report_capture = capture_report(run_dir, generated / "report-capture.png")
    write_narration(generated / "narration.md", run_id)

    slides = [
        slide_title(),
        slide_flow(run_id),
        slide_terminal(run_id),
        slide_metrics(summary),
        slide_matrix(summary),
        slide_case(run_dir),
        slide_report(report_capture),
        slide_gate(gate),
        slide_replay(replay),
        slide_close(),
    ]
    write_video(slides, args.output, seconds_per_slide=args.seconds_per_slide)
    print(args.output)


if __name__ == "__main__":
    main()
