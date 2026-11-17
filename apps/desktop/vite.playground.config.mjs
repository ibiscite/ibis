// The design playground: the canvas boards live on a local dev server, with
// comments saved to docs/design/comments.md (docs/design.md, "Playground").
// Dev only: nothing here reaches the app or the canvas bundle.
import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";
import { nativeBoundary } from "./vite.canvas.config.ts";

const root = fileURLToPath(new URL(".", import.meta.url));
const file = path.resolve(root, "../../docs/design/comments.md");
// One comment per line, so an agent or a person can edit the file by hand:
// - [ ] 3 · results · light · macos · 100% · base · `#root button[aria-label="More"]` · The comment
const line = /^- \[([ x])\] (\d+) · ([a-z0-9-]+) · (light|dark) · (macos|windows|linux) · (\d+)% · ([\w-]+) · `([^`]*)` · (.*)$/;

const parse = (source) => source.split("\n").flatMap((text) => {
  const match = line.exec(text);
  if (!match) return [];
  const [, done, id, scenario, theme, platform, size, variant, selector, comment] = match;
  return [{ id: Number(id), done: done === "x", scenario, theme, platform, text: Number(size), variant, selector, comment }];
});

const format = (entry) => `- [${entry.done ? "x" : " "}] ${entry.id} · ${entry.scenario} · ${entry.theme} · ${entry.platform} · ${entry.text}% · ${entry.variant} · \`${entry.selector}\` · ${entry.comment}`;

/** A new comment from the page, checked field by field: only these words reach the file. */
function entry(body, id) {
  const one = (value, pattern) => {
    if (typeof value !== "string" || !pattern.test(value)) throw new Error(`Unexpected value: ${String(value).slice(0, 40)}`);
    return value;
  };
  const flat = (value, limit) => {
    const text = typeof value === "string" ? value.replace(/\s+/g, " ").trim() : "";
    if (!text || text.length > limit) throw new Error("Expected text");
    return text;
  };
  const text = Number(body.text);
  if (!Number.isInteger(text) || text < 50 || text > 200) throw new Error("Unexpected text size");
  return {
    id, done: false, text,
    scenario: one(body.scenario, /^[a-z0-9-]+$/), theme: one(body.theme, /^(light|dark)$/),
    platform: one(body.platform, /^(macos|windows|linux)$/), variant: one(body.variant, /^[\w-]+$/),
    // A backtick would end the selector's code span; CSS reads \60 as one.
    selector: flat(body.selector, 2000).replaceAll("`", "\\60 "), comment: flat(body.comment, 4000),
  };
}

async function body(request) {
  let text = "";
  for await (const chunk of request) {
    text += chunk;
    if (text.length > 65_536) throw new Error("Comment too long");
  }
  return JSON.parse(text);
}

function comments() {
  return {
    name: "playground-comments",
    apply: "serve",
    configureServer(server) {
      // The page redraws its pins whenever the file changes, by the page or by hand.
      server.watcher.add(file);
      server.watcher.on("change", (changed) => {
        if (path.resolve(changed) === file) server.ws.send({ type: "custom", event: "playground:comments" });
      });
      server.middlewares.use("/__playground/comments", async (request, response) => {
        const send = (status, value) => {
          response.statusCode = status;
          response.setHeader("content-type", "application/json");
          response.end(JSON.stringify(value));
        };
        try {
          // Read afresh each time: the file is also edited by hand.
          const source = await readFile(file, "utf8");
          if (request.method === "GET") return send(200, parse(source));
          // Only this page writes: another site open in the browser cannot.
          const own = request.headers.origin === `http://${request.headers.host}`;
          if (request.method !== "POST" || !own || !request.headers["content-type"]?.startsWith("application/json")) return send(403, { error: "Only the playground page may write comments" });
          const change = await body(request);
          let next;
          if (typeof change.id === "number" && typeof change.done === "boolean") {
            // Marking one comment touches only its line.
            next = source.split("\n").map((text) => {
              const match = line.exec(text);
              return match && Number(match[2]) === change.id ? format({ ...parse(text)[0], done: change.done }) : text;
            }).join("\n");
          } else {
            const id = Math.max(0, ...parse(source).map((comment) => comment.id)) + 1;
            next = `${source.replace(/\n*$/, "\n")}${format(entry(change, id))}\n`;
          }
          await writeFile(file, next);
          send(200, parse(next));
        } catch (error) {
          send(400, { error: error instanceof Error ? error.message : String(error) });
        }
      });
    },
  };
}

export default defineConfig({
  root,
  plugins: [react(), nativeBoundary, comments()],
  clearScreen: false,
  // The app's own dev server keeps 5173; locales live at the repository root.
  server: { port: 5180, fs: { allow: ["../.."] } },
  optimizeDeps: { entries: ["canvas/playground/*.html"] },
});
