// What Esc closes in a window, and the one key listener that decides. Screens
// and controls register what they open and never handle Escape themselves.
import { useLayoutEffect, useRef, useSyncExternalStore } from "react";
import { flushSync } from "react-dom";
import { composing } from "../composition";

/** What Esc closes, nearest first. */
export const LAYER_ORDER = ["sheet", "menu", "panel", "chooser", "search", "window"] as const;
export type LayerKind = (typeof LAYER_ORDER)[number];

/** A layer closes itself. Given the key that asked, it returns focus to what
 * opened it, or returns false to hand that key on (a menu with a submenu open).
 * Without a key a command is clearing the way: it closes at once and goes home. */
export type LayerClose = (event?: KeyboardEvent) => boolean | void;
type Layer = { kind: LayerKind; close: LayerClose };

/** The window's own keys and where its keyboard rests. Without an owner the
 * listener still closes layers, so Settings' menus follow the same rule. */
export type KeyOwner = { keyDown: (event: KeyboardEvent) => void; home: () => void };

// What floats over the window. A panel sits in it and stays live beside the list.
const OVERLAYS: readonly LayerKind[] = ["sheet", "menu"];
const layers: Layer[] = [];
const watchers = new Set<() => void>();
let owner: KeyOwner | null = null;
let composition: EventTarget | null = null;
let settling = false;
let listening = false;

const rank = (layer: Layer) => LAYER_ORDER.indexOf(layer.kind);
const nearest = () => layers.reduce<Layer | undefined>((best, layer) => !best || rank(layer) <= rank(best) ? layer : best, undefined);

/** Whether the keyboard rests where no key handler or text field can use it. */
export function focusLost(): boolean {
  const active = document.activeElement;
  return active === null || active === document.body || !active.isConnected || active.closest("[hidden], [inert]") !== null;
}

/** Some systems flag only the composition's start, not each of its keys. A
 * field that left mid-composition never reports its end, so it no longer counts. */
export function composingKey(event: KeyboardEvent): boolean {
  return composing(event, composition instanceof Node && composition.isConnected);
}

/** A key has one effect: nothing else sees a key that a layer or command used. */
export function consumeKey(event: KeyboardEvent) {
  event.preventDefault();
  event.stopPropagation();
}

/** Close the nearest layer, as Esc does. False when it handed the key on. */
export function closeNearestLayer(event?: KeyboardEvent): boolean {
  const layer = nearest();
  if (!layer) return false;
  let closed = true;
  // Commit now: the layer returns focus itself, then the screen it left is checked.
  flushSync(() => { closed = layer.close(event) !== false; });
  settle();
  return closed;
}

export const overlayOpen = () => layers.some((layer) => OVERLAYS.includes(layer.kind));

/** Clear everything floating above the window, for a command or a hide. */
export function closeOverlays() {
  for (const layer of [...layers].filter((candidate) => OVERLAYS.includes(candidate.kind)).sort((a, b) => rank(a) - rank(b))) layer.close();
}

// Runs before anything in the page, so no control can swallow Escape or a
// command, and focus resting nowhere cannot lose them.
function keyDown(event: KeyboardEvent) {
  if (owner) owner.keyDown(event);
  else if (event.key === "Escape" && !composingKey(event) && closeNearestLayer(event)) consumeKey(event);
}
const compositionStart = (event: Event) => { composition = event.target; };
const compositionEnd = () => { composition = null; };

function listen() {
  const needed = layers.length > 0 || owner !== null;
  if (needed === listening) return;
  listening = needed;
  const change = needed ? window.addEventListener : window.removeEventListener;
  change("keydown", keyDown, true);
  change("compositionstart", compositionStart, true);
  change("compositionend", compositionEnd, true);
}

function changed() {
  listen();
  for (const watcher of watchers) watcher();
}

// A layer leaving can take the focused control with it. Inspect the committed
// screen, and leave focus alone if anything else took it.
function settle() {
  if (!settling) return;
  settling = false;
  if (focusLost() && !overlayOpen()) owner?.home();
}

/** Keep a layer on the stack while `open`. `close` may change every render. */
export function useLayer(kind: LayerKind, open: boolean, close: LayerClose) {
  const latest = useRef(close);
  useLayoutEffect(() => { latest.current = close; });
  useLayoutEffect(() => {
    if (!open) return;
    const layer: Layer = { kind, close: (event) => latest.current(event) };
    layers.push(layer);
    changed();
    return () => {
      layers.splice(layers.indexOf(layer), 1);
      changed();
      settling = true;
      queueMicrotask(settle);
    };
  }, [kind, open]);
}

/** Make `keys` the window's key owner while mounted. */
export function useKeyOwner(keys: KeyOwner) {
  const latest = useRef(keys);
  useLayoutEffect(() => { latest.current = keys; });
  useLayoutEffect(() => {
    owner = { keyDown: (event) => latest.current.keyDown(event), home: () => latest.current.home() };
    listen();
    return () => {
      owner = null;
      listen();
    };
  }, []);
}

/** The nearest layer's kind, for words such as the footer's Esc hint. */
export function useNearestLayer(): LayerKind | null {
  return useSyncExternalStore((change) => {
    watchers.add(change);
    return () => watchers.delete(change);
  }, () => nearest()?.kind ?? null);
}
