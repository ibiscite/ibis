import assert from 'node:assert/strict';
import { test } from 'node:test';
import { monitorUpdateOfferInput } from '../apps/desktop/src/features/picker/updateOfferInput.ts';

function picker() {
  const target = new EventTarget(), timers = new Map(), calls = [];
  let serial = 0, nextTimer = 0, blocked = false, copied = 0, idle = false;
  const monitor = monitorUpdateOfferInput(target, async (ticket) => {
    calls.push(ticket);
    if (ticket === null) { idle = false; return ++serial; }
    if (ticket === serial) idle = true;
    return ticket;
  }, () => blocked, (run) => { timers.set(++nextTimer, run); return nextTimer; }, (id) => timers.delete(id));
  target.addEventListener('keydown', (e) => { if (e.key === 'Enter') copied++; });
  const key = (type, key) => { const event = new Event(type); Object.defineProperty(event, 'code', { value: key }); Object.defineProperty(event, 'key', { value: key }); target.dispatchEvent(event); };
  return { target, monitor, calls, key, input(value) { blocked = value; target.dispatchEvent(new Event('input')); },
    async settle() { for (const [id, run] of timers) { timers.delete(id); run(); } await Promise.resolve(); await Promise.resolve(); },
    get idle() { return idle; }, get copied() { return copied; }, get pending() { return timers.size; } };
}

test('an update finishing immediately before Enter leaves copying in the picker', async () => {
  const p = picker();
  p.input(true);
  await p.settle();
  assert.equal(p.idle, false, 'non-empty Search must withhold the automatic offer');
  p.key('keydown', 'Enter');
  assert.equal(p.idle, false, 'capture must block before the picker handles Enter');
  assert.equal(p.copied, 1);
  p.key('keyup', 'Enter');
  assert.equal(p.pending, 0, 'non-empty input never arms an idle offer');
  p.input(false);
  assert.equal(p.idle, false);
  await p.settle();
  assert.equal(p.idle, true, 'the next empty, idle moment permits a fresh check');
  p.monitor.stop();
});

test('held keys, IME composition and hide cancel idle work', async () => {
  const p = picker();
  p.key('keydown', 'Space');
  await p.settle(); assert.equal(p.idle, false);
  p.key('keyup', 'Space'); await p.settle(); assert.equal(p.idle, true);
  p.target.dispatchEvent(new Event('compositionstart'));
  await p.settle(); assert.equal(p.idle, false);
  p.target.dispatchEvent(new Event('compositionend')); await p.settle(); assert.equal(p.idle, true);
  p.input(true); await p.settle(); assert.equal(p.idle, false);
  p.monitor.stop(); assert.equal(p.pending, 0);
});

test('late activity A cannot settle a newer keystroke B or a stopped visit', async () => {
  const target = new EventTarget(), answers = [], calls = [], timers = new Map(); let id = 0;
  const monitor = monitorUpdateOfferInput(target, (ticket) => {
    calls.push(ticket);
    return new Promise((resolve) => answers.push(resolve));
  }, () => false, (run) => { timers.set(++id, run); return id; }, (id) => timers.delete(id));
  const stale = [...timers.values()][0];
  target.dispatchEvent(new Event('input'));
  stale(); answers[0](1); await Promise.resolve();
  assert.deepEqual(calls, [null, null]);
  monitor.stop(); answers[1](2); await Promise.resolve();
  assert.ok(calls.every((ticket) => ticket === null));
  assert.equal(timers.size, 0);
});

test('failed activity I/O never sends an idle permission, and pointer hold stays busy', async () => {
  const target = new EventTarget(), calls = [], timers = new Map(); let id = 0;
  const monitor = monitorUpdateOfferInput(target, (ticket) => { calls.push(ticket); return Promise.reject(new Error('synthetic I/O')); },
    () => false, (run) => { timers.set(++id, run); return id; }, (id) => timers.delete(id));
  for (const [id, run] of timers) { timers.delete(id); run(); }
  await Promise.resolve(); await Promise.resolve();
  assert.deepEqual(calls, [null]);
  target.dispatchEvent(new Event('pointerdown'));
  assert.equal(timers.size, 0);
  monitor.stop();
});
