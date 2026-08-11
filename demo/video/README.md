# StructTrace Demo Video

This directory contains the reproducible real-product launch walkthrough recorder.

Generated video files are written under `demo/video/generated/` and are intentionally ignored by
git because MP4 artifacts are large binary outputs.

The public-launch walkthrough uses the real release binary and browser product:

```bash
STRUCTTRACE_UI_URL='http://127.0.0.1:<port>/<capability>/' \
  python3 demo/video/render-launch-walkthrough.py --url "$STRUCTTRACE_UI_URL"
```

`record-launch-walkthrough.mjs` drives real product interactions at 2560x1440 and records the
actual browser surface. `render-launch-walkthrough.py` generates disclosed neural narration using
the settings in `launch-production-manifest.json`, normalizes spoken loudness, and creates an H.264
master. Generated media and render receipts remain ignored because they are build artifacts.
