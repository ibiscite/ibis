---
name: diagnose
description: Find the root cause of an Ibis bug, flaky check or slowdown by building a tight failing signal first. Use when something is broken, failing, flaky or slow.
---

# Diagnose a bug

## 1. Build the signal

Get a fast pass/fail signal that goes red on this bug before reading code
for long. In order of preference:

1. A failing test at the layer that owns the behaviour
   (`cargo test -p CRATE --locked FILTER`, one Vitest file).
2. A CLI run on an invented fixture, diffed against the expected output.
3. A recording of the real built screen (pictures or per-frame state) for
   flicker and layout faults: look for state shown before its data arrives.
4. A loop or bisection (`git bisect run`) when it appeared between two states.

On CI-only failures, read the failing job's log first, then reproduce on the
same system (see [development](../../../docs/development.md#where-work-runs)).

## 2. Find the cause

Change one thing at a time against the signal. The cause is found when you can
make the signal red and green at will. A timeout, retry or sleep that turns it
green is not a cause.

## 3. Fix at the owner

Fix it in the place that owns the behaviour, keep the failing test as the
regression, and delete any workaround the fix makes unnecessary. If the same
fix would be needed elsewhere, change the shared piece instead. Still red after
three rounds: land what is right and write the rest on the issue in one line.
