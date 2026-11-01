import { expect, test, vi } from "vitest";
import { UndoOwner } from "./editUndo";

test("Edit and keyboard use the latest unpaused notice; retired menu tickets cannot consume a new action", () => {
  const publish = vi.fn();
  const owner = new UndoOwner(publish);
  const a = vi.fn(), b = vi.fn();
  const removeA = owner.register("Undo Remove Invented, 2026", a);
  const oldTicket = owner.revision;
  const removeB = owner.register("Undo Reset Look", b);
  expect(owner.title).toBe("Undo Reset Look");
  expect(owner.invoke(oldTicket)).toBe(false);
  expect(owner.invoke()).toBe(true);
  expect(b).toHaveBeenCalledOnce();
  removeA.pause(true); removeB.pause(true);
  expect(owner.title).toBeNull();
  expect(owner.invoke()).toBe(false);
  removeA.pause(false); removeB.pause(false);
  expect(owner.title).toBe("Undo Reset Look");
  removeB();
  expect(owner.title).toBe("Undo Remove Invented, 2026");
  expect(owner.invoke()).toBe(true);
  expect(a).toHaveBeenCalledOnce();
  removeA();
  expect(owner.title).toBeNull();
});

test("failed native publication leaves the visible action and keyboard undo usable", () => {
  const owner = new UndoOwner(() => { throw new Error("invented menu failure"); });
  const action = vi.fn();
  const stop = owner.register("Undo Reset Look", action);
  expect(owner.invoke()).toBe(true);
  expect(action).toHaveBeenCalledOnce();
  stop();
  expect(owner.invoke()).toBe(false);
});
