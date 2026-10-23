import { afterEach, expect, test, vi } from "vitest";
import { installNativeAppearance, setNativeAppearance } from "./nativeAppearance";

afterEach(() => { delete document.documentElement.dataset.nativeAppearance; });

test("only explicit native success exposes a material", () => {
  for (const bad of [undefined, null, false, {}, "mica", "transparent", "vibrancy; color: red"]) {
    setNativeAppearance(bad);
    expect(document.documentElement.dataset.nativeAppearance).toBe("opaque");
  }
  for (const good of ["acrylic", "vibrancy", "liquid-glass"]) {
    setNativeAppearance(good);
    expect(document.documentElement.dataset.nativeAppearance).toBe(good);
  }
});

test("subscription is ready before initial state and newer events win", async () => {
  let resolve!: (value: unknown) => void;
  let changed!: (value: unknown) => void;
  const unsubscribe = vi.fn();
  const ready = installNativeAppearance(() => new Promise((done) => { resolve = done; }), async (handler) => {
    changed = handler;
    return unsubscribe;
  });
  expect(document.documentElement.dataset.nativeAppearance).toBe("opaque");
  await Promise.resolve();
  changed("opaque");
  resolve("acrylic");
  const dispose = await ready;
  expect(document.documentElement.dataset.nativeAppearance).toBe("opaque");
  dispose();
  expect(unsubscribe).toHaveBeenCalledOnce();
});

test("read and subscription failures keep opacity", async () => {
  const unsubscribe = vi.fn();
  await installNativeAppearance(async () => { throw new Error("synthetic"); }, async () => unsubscribe);
  expect(document.documentElement.dataset.nativeAppearance).toBe("opaque");
  const read = vi.fn(async () => "acrylic");
  await installNativeAppearance(read, async () => { throw new Error("synthetic"); });
  expect(document.documentElement.dataset.nativeAppearance).toBe("opaque");
  expect(read).not.toHaveBeenCalled();
});

test("live native changes replace startup state and reject invalid modes", async () => {
  let changed!: (value: unknown) => void;
  const dispose = await installNativeAppearance(async () => "acrylic", async (handler) => {
    changed = handler;
    return () => {};
  });
  expect(document.documentElement.dataset.nativeAppearance).toBe("acrylic");
  changed("opaque");
  expect(document.documentElement.dataset.nativeAppearance).toBe("opaque");
  changed("vibrancy");
  expect(document.documentElement.dataset.nativeAppearance).toBe("vibrancy");
  changed({ mode: "acrylic" });
  expect(document.documentElement.dataset.nativeAppearance).toBe("opaque");
  dispose();
});

test("a newer event survives a failed initial read and disposal rejects late callbacks", async () => {
  let changed!: (value: unknown) => void;
  let reject!: (error: Error) => void;
  const ready = installNativeAppearance(() => new Promise((_, fail) => { reject = fail; }), async (handler) => {
    changed = handler;
    return () => {};
  });
  await Promise.resolve();
  changed("vibrancy");
  reject(new Error("synthetic"));
  const dispose = await ready;
  expect(document.documentElement.dataset.nativeAppearance).toBe("vibrancy");
  dispose();
  changed("acrylic");
  expect(document.documentElement.dataset.nativeAppearance).toBe("opaque");
});
