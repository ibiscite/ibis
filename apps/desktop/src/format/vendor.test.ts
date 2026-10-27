import manifest from "../../vendor/csl/manifest.json";

// Every vendored file as text, by its path inside vendor/csl.
const vendored = import.meta.glob<string>("../../vendor/csl/**/*.{mjs,csl,xml}", {
  query: "?raw",
  import: "default",
  eager: true,
});

async function sha256(text: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

describe("vendored engine and styles", () => {
  it("match the hashes pinned in the manifest", async () => {
    for (const file of manifest.files) {
      let text = vendored[`../../vendor/csl/${file.out}`];
      expect(text, file.out).toBeDefined();
      if ("module" in file && file.module) {
        expect(text?.endsWith(manifest.moduleSuffix)).toBe(true);
        text = text?.slice(0, -manifest.moduleSuffix.length);
      }
      expect(await sha256(text ?? ""), file.out).toBe(file.sha256);
    }
  });
});
