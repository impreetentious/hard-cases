import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// The page is static and is served from a project path on Pages, so every
// asset is referenced relatively. A base of "/" would work in development and
// break the moment it was published, which is the worst order to find out in.
export default defineConfig({
  base: "./",
  plugins: [react()],
  build: {
    // The module is over a megabyte and is fetched rather than inlined, so the
    // page paints before it arrives. Everything the page can do without it is
    // available while it loads, and everything it cannot is disabled and says
    // why.
    assetsInlineLimit: 0,
    chunkSizeWarningLimit: 700,
  },
  server: { port: 5173 },
});
