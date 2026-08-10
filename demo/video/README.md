# StructTrace Demo Video

This directory contains the reproducible local demo-video renderer.

Generated video files are written under `demo/video/generated/` and are intentionally ignored by
git because MP4 artifacts are large binary outputs.

Default workflow from the repository root:

```bash
cargo run -p structtrace-cli -- demo invoice
python3 demo/video/render_cinematic_demo.py --run-dir .structtrace/runs/<run-id>
```

The renderer reads the completed StructTrace run artifacts, captures the offline report when
`wkhtmltoimage` is available, and produces a technical MP4 demo without external assets.
