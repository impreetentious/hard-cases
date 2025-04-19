// The situation room, driven in a real browser.
//
// Against the built page rather than the development server, because what
// ships is the build: a page that worked under a module graph and broke under
// a bundle would be a page nobody had tested. `npm run build` runs first, and
// the preview server serves exactly the bytes that would be published.
//
// One browser. These tests are about whether the page tells the truth, not
// about rendering differences between engines, and a matrix here would triple
// the time for nothing this project is trying to learn.

import { defineConfig, devices } from "@playwright/test";

const PORT = 4173;

export default defineConfig({
  testDir: "./tests",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: process.env.CI ? "list" : "line",
  use: {
    baseURL: `http://127.0.0.1:${PORT}`,
    trace: "off",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: {
    // Bound explicitly rather than left to resolve: `localhost` picks the
    // IPv6 loopback first on some machines and the IPv4 one on others, and a
    // suite that hangs for three minutes on one developer's laptop and not on
    // another's is worse than one that fails.
    command: `npm run build && npx vite preview --host 127.0.0.1 --port ${PORT} --strictPort`,
    url: `http://127.0.0.1:${PORT}`,
    reuseExistingServer: !process.env.CI,
    timeout: 180_000,
  },
});
