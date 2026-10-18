import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed dev port and serves the built files from `dist`.
export default defineConfig(({ mode }) => ({
  plugins: [react()],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: {
    target: "es2022",
    outDir: "dist",
    rollupOptions: {
      input: mode === "evidence"
        ? {
          main: "index.html",
          gallery: "gallery.html",
        }
        : "index.html",
    },
  },
  test: { environment: "jsdom", globals: true },
}));
