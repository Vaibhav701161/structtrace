# StructTrace public deployment

The public site is a static Astro application under `site/`. It explains the product, serves a
bundled-data guided demo, publishes documentation and research provenance, and routes users to
verified GitHub release artifacts. It has no evaluation backend. Full comparisons remain local in
the existing Rust executable and embedded React UI.

## Local build

Required: Node.js 22 and npm.

```bash
cd site
npm ci
npm run build
npm test
npx playwright install chromium
npm run test:e2e -- --project=chromium
```

The generated site is `site/dist/`. The Cloudflare `_headers` and `_redirects` files are copied
into that directory during the build.

## Why the existing release workflow is retained

The repository already had a custom cross-platform release workflow before the website was added.
It performs the key work cargo-dist would otherwise need to be extended to cover: exact-source
workspace tests, deterministic frontend build verification, archive extraction, real binary and
embedded-UI smoke tests, per-target evidence receipts, SPDX SBOMs, SHA-256 checksums, source
archives, and GitHub provenance attestations. Replacing that accepted path during a deployment
change would increase experimental surface without improving the current evidence boundary.

Release tags matching `v*` build:

- `structtrace-x86_64-unknown-linux-musl.tar.gz`
- `structtrace-x86_64-apple-darwin.tar.gz`
- `structtrace-aarch64-apple-darwin.tar.gz`
- `structtrace-x86_64-pc-windows-msvc.zip`

Each archive has a sibling `.sha256` file, target-specific SPDX SBOM, release evidence JSON, source
archive, and test logs. `install.sh` and `install.ps1` are published with checksums after all target
jobs pass.

## Cloudflare Pages project

Use Git integration so every `main` push receives a preview and the production branch deploys
automatically. Cloudflare's current Astro build configuration uses `npm run build` and `dist`.

In Cloudflare:

1. Open **Workers & Pages**.
2. Select **Create application**, then **Pages**, then **Connect to Git**.
3. Authorize the GitHub repository `Vaibhav701161/structtrace`.
4. Set the project name to `structtrace`.
5. Set production branch to `main`.
6. Set root directory to `site`.
7. Set build command to `npm run build`.
8. Set build output directory to `dist`.
9. Do not add runtime environment variables.
10. Save and deploy.

The repository uses no paid Pages capability, Pages Function, Worker backend, analytics product,
database, account system, or runtime secret.

Official references:

- [Cloudflare Pages Git integration](https://developers.cloudflare.com/pages/get-started/git-integration/)
- [Cloudflare Astro build settings](https://developers.cloudflare.com/pages/framework-guides/deploy-an-astro-site/)
- [Cloudflare Pages build configuration](https://developers.cloudflare.com/pages/configuration/build-configuration/)
- [Cloudflare Pages static headers](https://developers.cloudflare.com/pages/configuration/headers/)

## Domain setup for structtrace.tech

At the time this document was written, the registrar displayed these default nameservers:

```text
cont603385.earth.orderbox-dns.com
cont603385.mars.orderbox-dns.com
cont603385.mercury.orderbox-dns.com
cont603385.venus.orderbox-dns.com
```

An apex Pages domain requires the domain to be an active Cloudflare zone in the same account as the
Pages project. Do not copy nameservers from an example or another domain.

1. In Cloudflare, choose **Add a domain** and enter `structtrace.tech`.
2. Select the Free plan.
3. Review imported DNS records. Remove only records that are known registrar parking records.
4. Cloudflare will show two nameservers assigned specifically to this zone.
5. In the registrar DNS screen, replace all four Orderbox nameservers with those exact two
   Cloudflare nameservers.
6. Wait until Cloudflare marks the zone **Active**. Propagation can take time and must be verified,
   not assumed.
7. In the Pages project, open **Custom domains** and add `structtrace.tech`.
8. Wait for the custom domain and SSL status to become active.
9. Add `www.structtrace.tech` to the same Pages project.
10. Create a Cloudflare Redirect Rule from `https://www.structtrace.tech/*` to
    `https://structtrace.tech/$1` with status 301 and query-string preservation.
11. Optionally follow Cloudflare's documented Pages rule to redirect the generated
    `*.pages.dev` hostname to the canonical domain. Until then, route-specific canonical tags
    still identify `https://structtrace.tech`.

Official references:

- [Cloudflare Pages custom domains](https://developers.cloudflare.com/pages/configuration/custom-domains/)
- [Redirect a Pages hostname to a custom domain](https://developers.cloudflare.com/pages/how-to/redirect-to-custom-domain/)

## Security headers

`site/public/_headers` applies:

- a self-only Content Security Policy with forms, objects, framing, and unnecessary browser
  connections disabled;
- `Cross-Origin-Opener-Policy: same-origin`;
- `Cross-Origin-Resource-Policy: same-origin`;
- disabled camera, microphone, geolocation, sensors, payment, USB, and display capture;
- `Referrer-Policy: strict-origin-when-cross-origin`;
- MIME sniffing and framing protections;
- immutable caching for fingerprint-independent brand assets and bounded caching for captured
  product screenshots and the social image.

The production test must confirm that the headers are present and do not break navigation, the
guided demo, screenshots, sitemap, or social image.

## Post-deployment verification

Run these checks only after Cloudflare shows the domain and certificate as active:

```bash
curl -fsSI https://structtrace.tech/
curl -fsS https://structtrace.tech/robots.txt
curl -fsS https://structtrace.tech/sitemap-index.xml
curl -fsSI https://www.structtrace.tech/
```

Then use a clean browser to verify `/`, `/product`, `/use-cases`, `/try`, `/research`, `/docs`,
`/security`, `/download`, `/privacy`, an unknown route, desktop and mobile navigation, keyboard
focus, the complete invoice demo, all external evidence links, the favicon, and the social image.

The final recommendation is **DEPLOYMENT READY - MANUAL DOMAIN SETUP REMAINS** until the custom
domain, SSL, permanent redirects, and actual release downloads have been verified live.
