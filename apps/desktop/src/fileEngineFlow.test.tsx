import { afterEach, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { App } from "./App";
import { backend, resetApp, result } from "./testing/app";
import type { FilePreview } from "./bindings";

afterEach(resetApp);

for (const [style, text] of [[null, "[@demo]"], ["apa", "(Researcher, 2026)"]] as const) {
  test(`engine file ${style ?? "Pandoc"} waits for native preview and copies only its receipt`, async () => {
    const id = { library: "opaque-library", key: "opaque-record" };
    const cite = "a".repeat(64);
    let finish: (preview: FilePreview) => void = () => {};
    const prepared = new Promise<FilePreview>((resolve) => { finish = resolve; });
    const preview = vi.fn((_args: unknown) => prepared);
    const copy = vi.fn(() => ({ text, warnings: [] }));
    const calls = backend({
      library_status: () => ({ state: "loaded", has_snapshot: true, file_engine: true, zotero: false, snapshot_zotero: false, name: "synthetic.bib", snapshot_name: "synthetic.bib", references: 1, warnings: 0, error: null, problem: null, note: null }),
      picker_preferences: () => ({ citation_style: style, density: "comfortable", larger_text: false, show_paste_preview: true }),
      search: ({ query }) => query.text ? [{ ...result("demo", "Synthetic research"), id, source: id.library, cite }] : [],
      preview_file_citation: preview,
      copy_file_citation: copy,
    });
    render(<App />);
    const input = screen.getByRole("combobox");
    fireEvent.change(input, { target: { value: "research" } });
    await screen.findByRole("option", { name: /Synthetic research/ });
    await waitFor(() => expect(preview).toHaveBeenCalled());
    fireEvent.keyDown(input, { key: "Enter" });
    expect(copy).not.toHaveBeenCalled();
    await act(async () => { finish({ ticket: "native-receipt", content_hash: "b".repeat(64), text, html: style ? "(Researcher, 2026)" : null, warnings: [] }); });
    await waitFor(() => expect(copy).toHaveBeenCalledWith({ ticket: "native-receipt", contentHash: "b".repeat(64) }));
    expect(preview).toHaveBeenCalledWith(expect.objectContaining({ items: [{ id, cite, locator: null }], style }));
    for (const legacy of ["citation_text", "csl_items", "copy_citation", "copy_formatted"]) expect(calls).not.toContain(legacy);
  });
}
