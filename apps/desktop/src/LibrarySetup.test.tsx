import { afterEach, expect, test, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import axe from "axe-core";
import { LibrarySetup } from "./LibrarySetup";

afterEach(cleanup);
const perform = async (_action: string, run: () => Promise<unknown>) => { await run(); };

test("initial setup never searches and manual source choices are immediately usable", async () => {
  const chooseLibrary = vi.fn(async () => undefined), useZotero = vi.fn(async () => undefined);
  const findSources = vi.fn(async () => ({ zotero: "absent" as const, list: 1, files: [], cut_short: false }));
  render(<LibrarySetup api={{ chooseLibrary, useZotero, findSources }} perform={perform} />);
  await act(async () => {});
  expect(findSources).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Choose library to get started" }));
  fireEvent.click(screen.getByRole("button", { name: "Use Zotero" }));
  expect(chooseLibrary).toHaveBeenCalledOnce();
  expect(useZotero).toHaveBeenCalledOnce();
  expect(findSources).not.toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "Find libraries" }).getAttribute("aria-disabled")).toBeNull();
});

test.each(["available", "absent", "access_off"] as const)("first-run has one suggested library action when Zotero is %s", async (zotero) => {
  const choose = vi.fn(async () => undefined), useZotero = vi.fn(async () => undefined);
  const { container } = render(<LibrarySetup api={{ chooseLibrary: choose, useZotero,
    findSources: async () => ({ zotero, list: 1, files: [], cut_short: false }) }} perform={perform}
    writing={{ format: "pandoc", formatted: true, choose: vi.fn() }} />);
  fireEvent.click(screen.getByRole("button", { name: "Find libraries" }));
  await waitFor(() => {
    const primary = container.querySelectorAll<HTMLButtonElement>(".setup-choices > [data-variant=\"primary\"]");
    expect(primary).toHaveLength(1);
    expect(primary.item(0).textContent).toBe(zotero === "available" ? "Use Zotero" : "Choose library…");
  });
  expect(screen.getByRole("button", { name: "Use Zotero" }).title).toBe("Read your Zotero library while Zotero is open");
  fireEvent.click(container.querySelector<HTMLButtonElement>('[data-variant="primary"]')!);
  expect(zotero === "available" ? useZotero : choose).toHaveBeenCalledOnce();
  expect(zotero === "available" ? choose : useZotero).not.toHaveBeenCalled();
});

const found = { zotero: "available" as const, list: 7, files: [{ name: "研究<literal>.bib", folder: "مراجع" }], cut_short: false };
const press = (button: HTMLElement, key = "Enter") => {
  button.focus();
  const code = key === " " ? "Space" : "Enter";
  fireEvent.keyDown(button, { key, code });
  fireEvent.keyDown(button, { key, code, repeat: true });
  fireEvent.keyUp(button, { key, code });
};

test("Find libraries retains focus while pending and rejects repeated scan presses", async () => {
  let resolve!: (value: typeof found) => void;
  const findSources = vi.fn().mockResolvedValue(found)
    .mockImplementationOnce(() => new Promise<typeof found>((done) => { resolve = done; }));
  const { unmount } = render(<LibrarySetup api={{ chooseLibrary: async () => undefined, useZotero: async () => undefined, findSources }} perform={perform} />);
  const again = screen.getByRole("button", { name: "Find libraries" });
  press(again);
  await waitFor(() => expect(findSources).toHaveBeenCalledTimes(1));
  expect(again.getAttribute("data-variant")).toBe("secondary");
  expect(again.getAttribute("aria-disabled")).toBe("true");
  expect((again as HTMLButtonElement).disabled).toBe(false);
  expect(document.activeElement).toBe(again);
  expect(screen.getByRole("button", { name: "Choose library to get started" }).getAttribute("aria-disabled")).toBeNull();
  expect(screen.getByRole("button", { name: "Use Zotero" }).getAttribute("aria-disabled")).toBeNull();
  fireEvent.click(again);
  press(again, " ");
  expect(findSources).toHaveBeenCalledTimes(1);
  await act(async () => resolve(found));
  expect(again.getAttribute("aria-disabled")).toBeNull();
  expect(document.activeElement).toBe(again);
  press(again, " ");
  await waitFor(() => expect(findSources).toHaveBeenCalledTimes(2));
  unmount();
  const accessibility = await axe.run(document.body, { runOnly: { type: "rule", values: ["role-img-alt"] } });
  expect(accessibility.violations.map((violation) => violation.id)).toEqual([]);
});

test("found-file presses preserve list identity and rescan a replaced list", async () => {
  const findSources = vi.fn().mockResolvedValueOnce(found).mockResolvedValue({ ...found, list: 8, files: [{ name: "new.bib", folder: "" }] });
  const openFoundLibrary = vi.fn().mockRejectedValueOnce(new Error("synthetic replaced list")).mockResolvedValue(null);
  const reporting = async (_action: string, run: () => Promise<unknown>) => { try { await run(); } catch { /* Reported by the caller. */ } };
  render(<LibrarySetup api={{ chooseLibrary: async () => undefined, useZotero: async () => undefined, findSources, openFoundLibrary }} perform={reporting} />);
  fireEvent.click(screen.getByRole("button", { name: "Find libraries" }));
  const file = await screen.findByRole("button", { name: /研究<literal>\.bib/ });
  expect(file.querySelectorAll("bdi")).toHaveLength(2);
  expect(file.title).toContain("研究<literal>.bib");
  expect(file.title).toContain("مراجع");
  press(file);
  await waitFor(() => expect(openFoundLibrary).toHaveBeenCalledExactlyOnceWith(7, 0));
  const next = await screen.findByRole("button", { name: "new.bib" });
  press(next, " ");
  await waitFor(() => expect(openFoundLibrary).toHaveBeenLastCalledWith(8, 0));
});

test("a refused discovery leaves manual choices and retry available", async () => {
  const chooseLibrary = vi.fn(async () => undefined), useZotero = vi.fn(async () => undefined);
  const findSources = vi.fn().mockRejectedValue(new Error("synthetic refusal"));
  const failed = vi.fn();
  const reporting = async (_action: string, run: () => Promise<unknown>) => { try { await run(); } catch { failed(); } };
  render(<LibrarySetup api={{ chooseLibrary, useZotero, findSources }} perform={reporting} />);
  press(screen.getByRole("button", { name: "Find libraries" }));
  await waitFor(() => expect(failed).toHaveBeenCalledOnce());
  expect(screen.getByRole("button", { name: "Look again" }).getAttribute("aria-disabled")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Choose library to get started" }));
  fireEvent.click(screen.getByRole("button", { name: "Use Zotero" }));
  expect(chooseLibrary).toHaveBeenCalledOnce();
  expect(useZotero).toHaveBeenCalledOnce();
});

test("a superseded discovery cannot replace the current list", async () => {
  let finish!: (value: typeof found) => void;
  const findSources = vi.fn(() => new Promise<typeof found>((resolve) => { finish = resolve; }));
  const api = { chooseLibrary: async () => undefined, useZotero: async () => undefined, findSources, openFoundLibrary: async () => null };
  const { rerender } = render(<LibrarySetup api={api} perform={perform} />);
  press(screen.getByRole("button", { name: "Find libraries" }));
  await waitFor(() => expect(findSources).toHaveBeenCalledOnce());
  rerender(<LibrarySetup api={{ ...api, findSources: async () => ({ ...found, list: 8, files: [{ name: "current.bib", folder: "" }] }) }} perform={perform} />);
  await screen.findByRole("button", { name: "current.bib" });
  await act(async () => finish(found));
  expect(screen.queryByRole("button", { name: /研究<literal>\.bib/ })).toBeNull();
  expect(screen.getByRole("button", { name: "current.bib" })).toBeTruthy();
});

test("writing choices remain controlled and pressing the selected format still reports it", () => {
  const choose = vi.fn();
  const api = { chooseLibrary: async () => undefined, useZotero: async () => undefined };
  const { rerender } = render(<LibrarySetup api={api} perform={perform} writing={{ format: "pandoc", formatted: true, choose }} />);
  const choices = screen.getAllByRole("button", { pressed: false });
  expect(choices).toHaveLength(3);
  const selected = screen.getByRole("button", { pressed: true });
  press(selected, " ");
  expect(choose).toHaveBeenCalledExactlyOnceWith("pandoc");
  press(choices[0]!);
  expect(choose).toHaveBeenLastCalledWith("cite");
  expect(selected.getAttribute("aria-pressed")).toBe("true");
  rerender(<LibrarySetup api={api} perform={perform} writing={{ format: "cite", formatted: false, choose }} />);
  expect(screen.getByRole("button", { pressed: true }).textContent).toBe("LaTeX");
  expect(screen.queryByRole("button", { name: "Word or Google Docs" })).toBeNull();
});
