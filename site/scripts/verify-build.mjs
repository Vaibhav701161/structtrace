import fs from "node:fs";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");
const dist = path.join(root, "dist");
const routes = ["", "product", "use-cases", "try", "research", "docs", "docs/install", "docs/first-comparison", "docs/concepts", "docs/evidence", "docs/cli", "security", "download", "privacy", "404"];
const failures = [];

for (const route of routes) {
  const file = route === "404" ? path.join(dist, "404.html") : path.join(dist, route, "index.html");
  if (!fs.existsSync(file)) failures.push(`missing route output: /${route}`);
}

for (const relative of ["_headers", "_redirects", "robots.txt", "favicon.svg", "site.webmanifest", "sitemap-index.xml", "screenshots/structtrace-results.webp", "social/structtrace-og.png"]) {
  if (!fs.existsSync(path.join(dist, relative))) failures.push(`missing public asset: ${relative}`);
}

const htmlFiles = routes.map((route) => route === "404" ? path.join(dist, "404.html") : path.join(dist, route, "index.html")).filter(fs.existsSync);
for (const file of htmlFiles) {
  const html = fs.readFileSync(file, "utf8");
  if (!html.includes('rel="canonical"')) failures.push(`${file} has no canonical URL`);
  if (!html.includes('name="description"')) failures.push(`${file} has no description`);
  for (const match of html.matchAll(/href="#([^"]+)"/g)) {
    if (!html.includes(`id="${match[1]}"`)) failures.push(`${file} links to missing fragment #${match[1]}`);
  }
  for (const match of html.matchAll(/<a[^>]+href="(\/[^"]*)"/g)) {
    const target = match[1].split(/[?#]/, 1)[0];
    if (target.startsWith("/_astro/") || target.startsWith("/assets/") || target.startsWith("/screenshots/") || target.startsWith("/social/") || target.startsWith("/scripts/")) continue;
    const expected = target === "/" ? path.join(dist, "index.html") : path.join(dist, target.slice(1), "index.html");
    if (!fs.existsSync(expected)) failures.push(`${file} links to missing internal route ${target}`);
  }
  if (/\u2014/.test(html)) failures.push(`${file} contains an em dash`);
}

if (failures.length) {
  console.error(failures.join("\n"));
  process.exit(1);
}
console.log(`Verified ${htmlFiles.length} static routes and required deployment assets.`);
