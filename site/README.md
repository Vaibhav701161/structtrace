# StructTrace public site

This directory contains the static public discovery and documentation site for
`https://structtrace.tech`. It is deliberately separate from the local Rust evaluation runtime.

```bash
npm ci
npm run build
npm test
npm run test:e2e -- --project=chromium
```

Cloudflare Pages settings:

| Setting | Value |
|---|---|
| Root directory | `site` |
| Build command | `npm run build` |
| Output directory | `dist` |
| Production branch | `main` |

The `/try` experience renders fixed results from the checked-in invoice fixture. It never accepts
user files and does not reproduce the Rust evaluator in JavaScript. Product screenshots are
captured from the real capability-protected local application with
`scripts/capture-public-product-screenshots.mjs`.

See the repository-level [`DEPLOYMENT.md`](../DEPLOYMENT.md) for release, Cloudflare, DNS, and
post-deployment verification instructions.
