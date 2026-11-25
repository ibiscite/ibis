import { expect, test, vi } from "vitest";
import { latest, type Latest, type Outcome, type Stamp } from "./latest";

type Entry = `started ${number}` | `finished ${number} ${"ok" | "failed"}` | `reset ${number}`;
type Log = readonly Entry[];
const push = (entry: Entry) => (log: Log): Log => [...log, entry];

/** A small seeded generator, so a failing schedule can be replayed by its seed. */
function random(seed: number) {
  let a = seed >>> 0;
  return (below: number) => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return (((t ^ (t >>> 14)) >>> 0) % below);
  };
}

/** What one owner should show, kept without stamps or identities. */
type Model = { log: Log; current: number | null; closed: boolean; publications: number };
type Run = { owner: Latest<Log>; model: Model; notified: number };

function schedule(seed: number) {
  const next = random(seed);
  const open = (): Run => {
    const run: Run = { owner: latest<Log>([]), model: { log: [], current: null, closed: false, publications: 0 }, notified: 0 };
    run.owner.subscribe(() => { run.notified += 1; });
    return run;
  };
  let live = open();
  const runs = [live];
  // [which owner, token, stamp], completed in any order.
  const outstanding: [Run, number, Stamp<Log>][] = [];
  const steps = next(48);
  for (let token = 0; token < steps; token++) {
    const { owner, model } = live;
    const kind = next(12);
    if (kind < 4) {
      outstanding.push([live, token, owner.begin()]);
      // Beginning publishes nothing: a stamp marks work, a commit shows it.
      if (!model.closed) model.current = token;
    } else if (kind < 8) {
      const [picked] = outstanding.splice(next(Math.max(outstanding.length, 1)), 1);
      if (!picked) continue;
      const [{ model: target }, work, stamp] = picked;
      const entry: Entry = `finished ${work} ${next(2) === 0 ? "ok" : "failed"}`;
      const ran = { reduce: false, after: false };
      stamp.commit((log) => { ran.reduce = true; return push(entry)(log); }, () => { ran.after = true; });
      const current = !target.closed && target.current === work;
      expect(ran).toEqual({ reduce: current, after: current });
      if (current) {
        target.log = [...target.log, entry];
        target.current = null;
        target.publications += 1;
      }
    } else if (kind < 10) {
      let ran = false;
      owner.invalidate((log) => { ran = true; return push(`reset ${token}`)(log); });
      expect(ran).toBe(!model.closed);
      if (!model.closed) {
        model.log = [...model.log, `reset ${token}`];
        model.current = null;
        model.publications += 1;
      }
    } else {
      owner.close();
      model.closed = true;
      model.current = null;
      if (kind === 11) { live = open(); runs.push(live); }
    }
    for (const run of runs) {
      expect(run.owner.getSnapshot()).toEqual(run.model.log);
      expect(run.notified).toBe(run.model.publications);
    }
  }
}

test("generated schedules match the sequential model after every step", () => {
  for (let seed = 0; seed < 500; seed++) {
    try { schedule(seed); } catch (error) { throw new Error(`Schedule ${seed} failed`, { cause: error }); }
  }
});

test("returning to the same request is a third identity", () => {
  const owner = latest("none");
  const firstA = owner.begin();
  const b = owner.begin();
  const secondA = owner.begin();
  firstA.commit(() => "old a");
  b.commit(() => "b");
  expect(owner.getSnapshot()).toBe("none");
  secondA.commit(() => "a");
  expect(owner.getSnapshot()).toBe("a");
});

test("a stamp commits once", () => {
  const owner = latest(0);
  const stamp = owner.begin();
  stamp.commit((n) => n + 1);
  stamp.commit((n) => n + 1);
  expect(owner.getSnapshot()).toBe(1);
});

test("the snapshot keeps its reference until a commit changes it, and an unchanged result notifies no one", () => {
  const owner = latest({ rows: [1] });
  const listener = vi.fn();
  owner.subscribe(listener);
  const before = owner.getSnapshot();
  owner.begin().commit((state) => state);
  owner.invalidate((state) => state);
  expect(owner.getSnapshot()).toBe(before);
  expect(listener).not.toHaveBeenCalled();
  owner.invalidate(() => ({ rows: [2] }));
  expect(listener).toHaveBeenCalledTimes(1);
});

test("a closed owner runs no work and keeps its last state", async () => {
  const owner = latest("shown");
  owner.close();
  const work = vi.fn(async () => "late");
  await owner.run(work, (_, outcome) => outcome.ok ? outcome.value : "failed");
  owner.invalidate(() => "reset");
  owner.begin().commit(() => "begun");
  expect(work).not.toHaveBeenCalled();
  expect(owner.getSnapshot()).toBe("shown");
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

test("run starts work at once and commits only the newest outcome, success or failure", async () => {
  const owner = latest<string>("none");
  const a = deferred<string>();
  const b = deferred<string>();
  const after = vi.fn();
  const reduce = (_: string, outcome: Outcome<string>) => outcome.ok ? outcome.value : "failed";
  const first = owner.run(() => a.promise, reduce, after);
  const second = owner.run(() => b.promise, reduce, after);
  b.resolve("b");
  await second;
  a.reject(new Error("invented failure"));
  await first;
  expect(owner.getSnapshot()).toBe("b");
  expect(after).toHaveBeenCalledTimes(1);
  expect(after).toHaveBeenCalledWith({ ok: true, value: "b" });
});

test("work that throws before it returns a promise is a failed outcome", async () => {
  const owner = latest("none");
  await owner.run(() => { throw new Error("invented failure"); }, (_, outcome) => outcome.ok ? "ok" : "failed");
  expect(owner.getSnapshot()).toBe("failed");
});

test("invalidating retires work in flight", async () => {
  const owner = latest("none");
  const read = deferred<string>();
  const pending = owner.run(() => read.promise, (_, outcome) => outcome.ok ? outcome.value : "failed");
  owner.invalidate(() => "hidden");
  read.resolve("late");
  await pending;
  expect(owner.getSnapshot()).toBe("hidden");
});

test("a reducer cannot begin or retire work, and one that throws changes nothing", () => {
  const owner = latest(0);
  expect(() => owner.begin().commit(() => { owner.invalidate(); return 1; })).toThrow(/reducer/);
  expect(() => owner.invalidate(() => { owner.begin(); return 2; })).toThrow(/reducer/);
  expect(owner.getSnapshot()).toBe(0);
  const spent = owner.begin();
  expect(() => spent.commit(() => { throw new Error("invented failure"); })).toThrow("invented failure");
  spent.commit(() => 3);
  expect(owner.getSnapshot()).toBe(0);
});

test("an asynchronous reducer is refused before it publishes", () => {
  const owner = latest<unknown>("none");
  expect(() => owner.begin().commit(() => Promise.resolve("late"))).toThrow(TypeError);
  expect(owner.getSnapshot()).toBe("none");
});

test("after and listeners may begin newer work once the state is published", () => {
  const owner = latest(0);
  let newer: Stamp<number> | undefined;
  const seen: number[] = [];
  owner.subscribe(() => { seen.push(owner.getSnapshot()); });
  owner.begin().commit(() => 1, () => { newer = owner.begin(); });
  newer?.commit((n) => n + 1);
  expect(seen).toEqual([1, 2]);
});

test("a commit's effects run before a listener can begin newer work", () => {
  const owner = latest("none");
  const effects: string[] = [];
  let once = true;
  owner.subscribe(() => {
    if (!once) return;
    once = false;
    owner.begin().commit(() => "b", () => effects.push("b"));
  });
  owner.begin().commit(() => "a", () => effects.push("a"));
  expect(effects).toEqual(["a", "b"]);
  expect(owner.getSnapshot()).toBe("b");
});

test("checking a result for a promise cannot commit other work", () => {
  const owner = latest<unknown>("none");
  const sneaky = { get then() { owner.begin().commit(() => "b"); return undefined; } };
  expect(() => owner.begin().commit(() => sneaky)).toThrow(/reducer/);
  expect(owner.getSnapshot()).toBe("none");
});
