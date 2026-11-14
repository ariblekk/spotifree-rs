import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath, URL } from "node:url";

/// Dev only: the Rust server keeps the API. Proxy the REST endpoints and leave
/// everything else to Vite, so `/api` still resolves to the React page.
///
/// `BACKEND_ORIGIN` points the proxy at the Rust server, which may live on
/// another machine — the browser then only ever talks to Vite (same-origin, so
/// no CORS and no `ALLOWED_ORIGIN` needed):
///   BACKEND_ORIGIN=http://10.10.10.98:8080 npm run dev
const backend = process.env.BACKEND_ORIGIN ?? "http://127.0.0.1:8080";

const rest = [
  "/api/search",
  "/api/charts",
  "/api/track",
  "/api/lyrics",
  "/api/album",
  "/api/artist",
  "/api/playlist",
  "/healthz",
];

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
  server: {
    port: 5173,
    // Listen on all interfaces, so the dev server is reachable from a phone or
    // another machine on the LAN (`http://10.10.10.98:5173`). Vite's default is
    // `localhost`, which binds loopback only and refuses those connections.
    // Override with `--host <addr>` for a specific interface.
    host: true,
    proxy: Object.fromEntries(rest.map((path) => [path, backend])),
  },
});
