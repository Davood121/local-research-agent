import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri drives this dev server; the port is fixed because tauri.conf.json points at it.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    // WebKit on macOS/Linux, Chromium (WebView2) on Windows.
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari13",
    minify: "esbuild",
    sourcemap: false,
  },
});
