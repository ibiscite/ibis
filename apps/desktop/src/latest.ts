// One owner for state that newer work replaces: the interface's half of
// `ibis_core::epoch`. The owner holds the state, so a late completion cannot
// write it by any route but its own stamp, and there is no setter and no way
// to ask whether work is still current. React reads it through
// `useSyncExternalStore(owner.subscribe, owner.getSnapshot)`.

/** How work ended. A failure commits through the same stamp as a success. */
export type Outcome<A> = { ok: true; value: A } | { ok: false; error: unknown };

export type Reduce<S> = (state: Readonly<S>) => S;

/** The right to commit one piece of work, while nothing newer has begun. */
export type Stamp<S> = {
  /** Applies `reduce` unless newer work began, the work was retired or the
   * owner closed; `after` runs only then, in the same turn. A stamp commits
   * once. A reducer that throws changes nothing and spends the stamp. */
  commit(reduce: Reduce<S>, after?: () => void): void;
};

export type Latest<S> = {
  /** Begins work that replaces any outstanding work. */
  begin(): Stamp<S>;
  /** Begins `work` now and commits how it ended. `after` sees the outcome
   * only when it was committed, in the committing turn. */
  run<A>(work: () => Promise<A>, reduce: (state: Readonly<S>, outcome: Outcome<A>) => S, after?: (outcome: Outcome<A>) => void): Promise<void>;
  /** Retires outstanding work, applying `reset` in the same turn. */
  invalidate(reset?: Reduce<S>): void;
  /** Retires outstanding work for good. Reopening means a new owner. */
  close(): void;
  getSnapshot(): Readonly<S>;
  subscribe(listener: () => void): () => void;
};

export function latest<S>(initial: S): Latest<S> {
  let state: Readonly<S> = initial;
  // Each begin makes a fresh identity, so returning to the same request is new work.
  let active: object | null = null;
  let open = true;
  let reducing = false;
  const listeners = new Set<() => void>();

  const outsideReducer = () => {
    if (reducing) throw new Error("A reducer returns the next state; it cannot begin, commit or retire work.");
  };
  const apply = (reduce: Reduce<S>, after?: () => void) => {
    reducing = true;
    let next: S;
    try {
      next = reduce(state);
      // Still inside the guard: a `then` getter cannot begin or commit work either.
      if (typeof (next as { then?: unknown } | null)?.then === "function") throw new TypeError("A reducer returns the next state, not a promise.");
    } finally { reducing = false; }
    const changed = !Object.is(next, state);
    state = next;
    // A commit's effects come before any listener can begin newer work.
    try { after?.(); } finally { if (changed) for (const listener of [...listeners]) listener(); }
  };
  const begin = (): Stamp<S> => {
    outsideReducer();
    const identity = {};
    if (open) active = identity;
    return {
      commit(reduce, after) {
        outsideReducer();
        if (!open || active !== identity) return;
        active = null;
        apply(reduce, after);
      },
    };
  };

  async function run<A>(work: () => Promise<A>, reduce: (state: Readonly<S>, outcome: Outcome<A>) => S, after?: (outcome: Outcome<A>) => void) {
    outsideReducer();
    if (!open) return;
    const stamp = begin();
    let outcome: Outcome<A>;
    try { outcome = { ok: true, value: await work() }; } catch (error) { outcome = { ok: false, error }; }
    stamp.commit((current) => reduce(current, outcome), after && (() => after(outcome)));
  }

  return {
    begin,
    run,
    invalidate(reset) {
      outsideReducer();
      if (!open) return;
      active = null;
      if (reset) apply(reset);
    },
    close() {
      outsideReducer();
      open = false;
      active = null;
    },
    getSnapshot: () => state,
    subscribe(listener) {
      listeners.add(listener);
      return () => { listeners.delete(listener); };
    },
  };
}
