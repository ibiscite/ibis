// The view stack: what is open in a window, nearest first, and the one key
// listener that walks back through it. Screens and controls register what they
// open and never handle a going-back key themselves (decision 0047).
import { createContext, useContext, useLayoutEffect, useRef, useSyncExternalStore } from "react";
import { flushSync } from "react-dom";
import { composing } from "../composition";
import { isBackShortcut } from "../keys";

/** The ways back: Esc; the platform's Back (⌘[, Alt+Left, the mouse's Back
 * button); Backspace in an empty field. */
export type Gesture = "escape" | "back" | "erase";
type Kind = { back: readonly Gesture[]; opens?: "floats" | "view" | "beside" };
/** Every kind of layer, nearest first, in one table: which ways back it answers
 * (decision 0047's table; the nearest layer decides alone, so an open pop-up
 * keeps Back and Backspace from the view under it), and what opening it is. A
 * menu and a popover float over the window, one at a time; a palette and a sheet
 * are views, taking the window's place; a panel sits beside the list. Search and
 * the window register too, but typing is not opening anything. */
export const KINDS = {
  menu: { back: ["escape"], opens: "floats" }, popover: { back: ["escape", "back"], opens: "floats" },
  palette: { back: ["escape", "back", "erase"], opens: "view" }, sheet: { back: ["escape", "back"], opens: "view" },
  panel: { back: ["escape", "back"], opens: "beside" }, chooser: { back: ["escape"] }, search: { back: ["escape"] }, window: { back: ["escape"] },
} as const satisfies Record<string, Kind>;
export type LayerKind = keyof typeof KINDS;
const of = (layer: Layer): Kind => KINDS[layer.kind];
/** Over the window: what a command or a hide clears, and what keeps ⌘1–9 from the list. */
const over = (layer: Layer) => of(layer).opens === "floats" || of(layer).opens === "view";

/** A layer closes itself. Given the key that asked, it returns focus to what
 * opened it, or returns false to hand that key on (a menu with a submenu open).
 * Without a key a command is clearing the way: it closes at once and goes home. */
export type LayerClose = (event?: KeyboardEvent) => boolean | void;
type Layer = { kind: LayerKind; close: LayerClose; parent: Scope | null };
// Read when asked: a surface that mounts open registers before the layer around it.
const within = (layer: Layer, outer: Layer) => { for (let at = layer.parent?.current; at; at = at.parent?.current) if (at === outer) return true; return false; };

/** The window's own keys, where its keyboard rests, and what anything opening
 * over it ends (a pending copy). Without an owner the listener still closes
 * layers, so Settings' menus follow the same rule. */
export type KeyOwner = { keyDown: (event: KeyboardEvent) => void; home: () => void; opened?: () => void };
const layers: Layer[] = [];
const watchers = new Set<() => void>();
let owner: KeyOwner | null = null;
let composition: EventTarget | null = null;
let settling = false;
let listening = false;

const ORDER = Object.keys(KINDS);
const rank = (layer: Layer) => ORDER.indexOf(layer.kind);
const nearest = () => layers.filter((layer) => !layers.some((other) => within(other, layer)))
  .reduce<Layer | undefined>((best, layer) => !best || rank(layer) <= rank(best) ? layer : best, undefined);

/** Whether the keyboard rests where no key handler or text field can use it. */
export function focusLost(): boolean {
  const active = document.activeElement;
  return active === null || active === document.body || !active.isConnected || active.closest("[hidden], [inert]") !== null;
}

/** Some systems flag only the composition's start, not each of its keys. A
 * field that left mid-composition never reports its end, so it no longer counts. */
export function composingKey(event: KeyboardEvent): boolean {
  return composing(event, compositionInProgress());
}

export const compositionInProgress = () => composition instanceof Node && composition.isConnected;

/** A key has one effect: nothing else sees a key that a layer or command used. */
export function consumeKey(event: Event) {
  event.preventDefault();
  event.stopPropagation();
}

/** Go back one step: the nearest layer closes if it answers `gesture`. False
 * when nothing answered or the layer handed the key on. */
export function goBack(gesture: Gesture = "escape", event?: KeyboardEvent): boolean {
  const layer = nearest();
  if (!layer || !of(layer).back.includes(gesture)) return false;
  let closed = true;
  // Commit now: the layer returns focus itself, then the screen it left is checked.
  flushSync(() => { closed = layer.close(event) !== false; });
  settle();
  return closed;
}

export const overlayOpen = () => layers.some(over);

/** Clear everything floating above the window, for a command or a hide. */
export function closeOverlays() {
  for (const layer of layers.filter(over).sort((a, b) => rank(a) - rank(b))) layer.close();
}

const answered = (gesture: Gesture) => { const layer = nearest(); return layer !== undefined && of(layer).back.includes(gesture); };
const emptyField = (target: EventTarget | null) => (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) && target.value === "";

// Runs before anything in the page, so no control can swallow Escape or a
// command, and focus resting nowhere cannot lose them. Back and an empty
// Backspace go back here; Esc goes to the window's owner first, which keeps one
// step per press.
function keyDown(event: KeyboardEvent) {
  const plain = !event.ctrlKey && !event.metaKey && !event.altKey && !event.shiftKey;
  const gesture = isBackShortcut(event) ? "back" : event.key === "Backspace" && plain && emptyField(event.target) ? "erase" : null;
  if (gesture && !composingKey(event) && answered(gesture)) {
    consumeKey(event);
    // A held key never walks back: it must be released and pressed again.
    if (!event.repeat) goBack(gesture, event);
  } else if (owner) owner.keyDown(event);
  // One press, one step: a held Esc never walks down the stack.
  else if (event.key === "Escape" && !composingKey(event) && (event.repeat ? answered("escape") : goBack("escape", event))) consumeKey(event);
}
function mouseDown(event: MouseEvent) {
  if (event.button !== 3 || event.defaultPrevented || compositionInProgress() || !answered("back")) return;
  event.preventDefault(); event.stopPropagation();
  goBack("back");
}
const compositionStart = (event: Event) => { composition = event.target; };
const compositionEnd = () => { composition = null; };
// Any unused Back must still never leave the webview, including Search's no-op
// and the release after a mouse press closed a view.
const preventBrowserBackKey = (event: KeyboardEvent) => { if (isBackShortcut(event) && !composingKey(event)) event.preventDefault(); };
const preventBrowserBackMouse = (event: MouseEvent) => { if (event.button === 3) event.preventDefault(); };

function listen() {
  const needed = layers.length > 0 || owner !== null;
  if (needed === listening) return;
  listening = needed;
  const change = needed ? window.addEventListener : window.removeEventListener;
  change("keydown", keyDown, true);
  change("mousedown", mouseDown, true);
  change("compositionstart", compositionStart, true);
  change("compositionend", compositionEnd, true);
  change("keydown", preventBrowserBackKey);
  change("mousedown", preventBrowserBackMouse);
  change("mouseup", preventBrowserBackMouse);
  change("auxclick", preventBrowserBackMouse);
}

function changed() {
  listen();
  // While a view is open the window shows only it.
  if (layers.some((layer) => of(layer).opens === "view")) document.documentElement.dataset.view = "";
  else delete document.documentElement.dataset.view;
  for (const watcher of watchers) watcher();
}

// A layer leaving can take the focused control with it. Inspect the committed
// screen, and leave focus alone if anything else took it.
function settle() {
  if (!settling) return;
  settling = false;
  if (focusLost() && !overlayOpen()) owner?.home();
}

/** A layer's handle: whether it is nearest, so its own keys act only then, and
 * `leave`, which takes it off the stack at once, before it unmounts, so what is
 * under it shows before focus returns there. */
export type LayerHandle = { nearest: () => boolean; leave: () => void; scope: Scope };
type Scope = { readonly current: Layer | null };
/** The layer whatever renders inside it belongs to (ui/Layer). It crosses portals. */
export const LayerParent = createContext<Scope | null>(null);

// One rule, so no opener has to clear the way itself: a surface opening closes
// every open transient it is not inside or around, and a view every other view.
// It runs after the commit that opened it, where a closing layer may flush, and
// focus stays where the new surface put it.
function displace(layer: Layer) {
  const rivals = layers.filter((other) => other !== layer && !within(layer, other) && !within(other, layer)
    && (of(other).opens === "floats" || of(layer).opens === "view" && of(other).opens === "view")).reverse();
  if (!rivals.length) return;
  queueMicrotask(() => {
    if (!layers.includes(layer)) return;
    const focused = document.activeElement;
    for (const rival of rivals) if (layers.includes(rival)) flushSync(() => { rival.close(); });
    if (focused instanceof HTMLElement && focused.isConnected && document.activeElement !== focused) focused.focus({ preventScroll: true });
  });
}

/** Keep a layer on the stack while `open`. `close` may change every render. */
export function useLayer(kind: LayerKind, open: boolean, close: LayerClose): LayerHandle {
  const latest = useRef(close);
  const registered = useRef<Layer | null>(null);
  const parent = useContext(LayerParent);
  useLayoutEffect(() => { latest.current = close; });
  useLayoutEffect(() => {
    if (!open) return;
    const layer: Layer = { kind, close: (event) => latest.current(event), parent };
    registered.current = layer;
    layers.push(layer);
    changed();
    if (of(layer).opens) displace(layer);
    if (over(layer)) owner?.opened?.();
    return () => {
      registered.current = null;
      const at = layers.indexOf(layer);
      if (at >= 0) { layers.splice(at, 1); changed(); }
      settling = true;
      queueMicrotask(settle);
    };
  }, [kind, open, parent]);
  const handle = useRef<LayerHandle>(null);
  handle.current ??= {
    scope: registered,
    nearest: () => registered.current !== null && nearest() === registered.current,
    leave: () => {
      const at = registered.current ? layers.indexOf(registered.current) : -1;
      if (at >= 0) { layers.splice(at, 1); changed(); }
    },
  };
  return handle.current;
}

/** Make `keys` the window's key owner while mounted. */
export function useKeyOwner(keys: KeyOwner) {
  const latest = useRef(keys);
  useLayoutEffect(() => { latest.current = keys; });
  useLayoutEffect(() => {
    owner = { keyDown: (event) => latest.current.keyDown(event), home: () => latest.current.home(), opened: () => latest.current.opened?.() };
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
