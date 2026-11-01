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
