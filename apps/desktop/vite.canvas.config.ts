import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  define: { "process.env.NODE_ENV": JSON.stringify("production") },
  oxc: { jsx: { development: false } },
  plugins: [react(), {
    name: "canvas-native-boundary",
    enforce: "pre",
    resolveId(id) { if (id.startsWith("@tauri-apps/api/")) return "\0canvas-native"; },
    load(id) {
      if (id !== "\0canvas-native") return;
      // The picker preference queue issues revisions outside its injected API.
      // Everything else must use the canvas fake; unexpected native I/O fails.
      return `let revision = 0;
        export async function invoke(command) {
          if (command === "preference_revision") return ++revision;
          throw new Error("Native command unavailable on canvas: " + command);
        }
        export function listen() { throw new Error("Native events unavailable on canvas"); }
        export function getCurrentWindow() { throw new Error("Native window unavailable on canvas"); }`;
    },
  }],
  base: "./",
  build: {
    target: "es2022",
    outDir: "dist-canvas",
    lib: { entry: "canvas/main.tsx", name: "IbisCanvas", formats: ["iife"], fileName: () => "ibis-canvas.js", cssFileName: "ibis-canvas" },
  },
});
