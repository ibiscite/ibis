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
      // The Copied notice is its own small window (#496).
      input: {
        main: "index.html",
        notice: "notice.html",
        ...(mode === "evidence" ? { gallery: "gallery.html" } : {}),
      } as Record<string, string>,
    },
  },
  test: { environment: "jsdom", globals: true },
}));
