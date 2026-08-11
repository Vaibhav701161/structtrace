import { defineConfig } from "astro/config";
import sitemap from "@astrojs/sitemap";

export default defineConfig({
  site: "https://structtrace.tech",
  output: "static",
  trailingSlash: "never",
  integrations: [sitemap()],
  build: { format: "directory" },
});
