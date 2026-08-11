# Product screenshot provenance

Every screenshot in `public/screenshots/` is captured from the real embedded
StructTrace interface while it is served by the release Rust binary. The
screens use the checked-in `invoice-structured-extraction` example and its
actual recorded evidence. No product screen is a design mockup or generated
illustration.

Regenerate the PNG sources and optimized WebP derivatives from the repository
root:

```bash
cargo build --release --locked
npm --prefix site ci
node scripts/capture-public-product-screenshots.mjs
```

The capture script starts `target/release/structtrace open --no-browser`, waits
for the local application health check, loads the bundled example through the
UI, and captures each named workflow state with Playwright.
