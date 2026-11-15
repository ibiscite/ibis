import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  define: { "process.env.NODE_ENV": JSON.stringify("production") },
  oxc: { jsx: { development: false } },
  plugins: [react(), {
    name: "canvas-native-boundary",
    enforce: "pre",
    resolveId(id) {
      if (id.startsWith("@tauri-apps/api/") && !["core", "mocks"].some((part) => id === `@tauri-apps/api/${part}`)) return "\0canvas-native";
    },
    load(id) {
      if (id !== "\0canvas-native") return;
      // Route events to mock IPC and keep native window management out of
      // the bundle. Visibility is the canvas's only window call.
      return `export function getCurrentWindow() {
          return { isVisible: () => window.__TAURI_INTERNALS__.invoke("plugin:window|is_visible") };
        }
        export function getVersion() { throw new Error("Native version unavailable on canvas"); }
        export async function listen(event, handler) {
          const { invoke, transformCallback, unregisterCallback } = window.__TAURI_INTERNALS__;
          const id = transformCallback(handler);
          await invoke("plugin:event|listen", { event, handler: id });
          return async () => { await invoke("plugin:event|unlisten", { event, id }); unregisterCallback(id); };
        }
        export function emit(event, payload) { return window.__TAURI_INTERNALS__.invoke("plugin:event|emit", { event, payload }); }`;
    },
  }],
  base: "./",
  build: {
    target: "es2022",
    outDir: "dist-canvas",
    lib: { entry: "canvas/main.tsx", name: "IbisCanvas", formats: ["iife"], fileName: () => "ibis-canvas.js", cssFileName: "ibis-canvas" },
  },
});
