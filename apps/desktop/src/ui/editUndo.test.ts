import { expect, test, vi } from "vitest";
import { UndoOwner } from "./editUndo";

test("the keyboard uses the latest unpaused notice", () => {
  const owner = new UndoOwner();
  const a = vi.fn(), b = vi.fn();
  const removeA = owner.register(a);
  const removeB = owner.register(b);
  expect(owner.invoke()).toBe(true);
  expect(b).toHaveBeenCalledOnce();
  removeA.pause(true); removeB.pause(true);
  expect(owner.invoke()).toBe(false);
  removeA.pause(false); removeB.pause(false);
  removeB();
  expect(owner.invoke()).toBe(true);
  expect(a).toHaveBeenCalledOnce();
  removeA();
  expect(owner.invoke()).toBe(false);
});

test("text typed since the notice appeared keeps its own undo", () => {
  const owner = new UndoOwner();
  owner.register(() => {}, "invented");
  expect(owner.takes("invented")).toBe(true);
  expect(owner.takes("invented lantern")).toBe(false);
  expect(owner.takes(null)).toBe(true);
});

test("nothing to undo leaves the key alone", () => {
  expect(new UndoOwner().takes(null)).toBe(false);
});

test("the newest offer is known to the slot, including while paused", () => {
  const owner = new UndoOwner();
  const changed = vi.fn();
  const stop = owner.subscribe(changed);
  const a = owner.register(() => {});
  const b = owner.register(() => {});
  expect([a.isNewest(), b.isNewest()]).toEqual([false, true]);
  b.pause(true);
  expect(b.isNewest()).toBe(true);
  b();
  expect(a.isNewest()).toBe(true);
  expect(changed).toHaveBeenCalledTimes(3);
  stop();
});
