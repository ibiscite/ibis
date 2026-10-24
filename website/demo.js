import { KEY_TABLE, keysFor, keyLabel, commandKey, matchesAccelerator } from "./keys.js";
import { typePaths } from "./icon-paths.js";
import { createDemo, references, formats } from "./demo-model.mjs";
const el = (tag, props = {}, ...children) => { const node = Object.assign(document.createElement(tag), props); node.append(...children); return node; };
function icon(kind) {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 20 20"); svg.setAttribute("aria-hidden", "true");
  for (const drawing of typePaths[kind]) { const path = document.createElementNS(svg.namespaceURI, "path"); path.setAttribute("d", drawing); svg.append(path); }
  return svg;
}
export function setupDemo() {
  const figure = document.querySelector("[data-picker]"), result = document.querySelector("[data-result]");
  if (!figure || !result) return;
  const section = figure.closest("section"), anchorBottom = section.getBoundingClientRect().bottom;
  const model = createDemo(), state = model.state, mac = document.documentElement.dataset.platform === "macos";
  const sentence = result.firstChild.textContent.trim();
  result.tabIndex = -1;
  const input = el("input", { type: "search", value: state.query, placeholder: "Search references", autocomplete: "off" });
  const clear = el("button", { type: "button", className: "picker-clear", textContent: "×" }); clear.setAttribute("aria-label", "Clear search");
  for (const [key, value] of Object.entries({ role: "combobox", "aria-label": "Search the made-up library", "aria-autocomplete": "list", "aria-controls": "demo-results", "aria-expanded": "true" })) input.setAttribute(key, value);
  const select = el("select", {}, ...Object.entries(formats).map(([value, label]) => el("option", { value, textContent: label })));
  select.setAttribute("aria-label", "Copy format");
  // A drawn chevron beside the select: native arrows ignore its height on iOS, and Safari drops background chevrons.
  const format = el("span", { className: "picker-format" }, select);
  format.insertAdjacentHTML("beforeend", '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="m4.5 6.5 3.5 3.5 3.5-3.5"/></svg>');
  const list = el("ul", { id: "demo-results", className: "picker-results" });
  list.setAttribute("role", "listbox"); list.setAttribute("aria-label", "Best matches");
  const count = el("span"), output = el("code", { dir: "ltr" }), status = el("span", { className: "picker-status" });
  // The visible status can appear with the shelf; this live region is always rendered so the first change is announced.
  const announce = el("span", { className: "demo-status" }); announce.setAttribute("role", "status"); status.setAttribute("aria-hidden", "true");
  const copy = el("button", { type: "button", textContent: "Copy" }), add = el("button", { type: "button", textContent: "Add" });
  const paste = el("button", { type: "button", className: "paste-slot" }, "Paste ", el("kbd", { textContent: keyLabel("CmdOrCtrl+V", mac) }));
  paste.setAttribute("aria-label", "Paste the copied citation into the sentence");
  const reopen = el("button", { type: "button", textContent: "Open the picker again", hidden: true });
  const hint = el("p", { className: "demo-hint" });
  const labels = { copy: "copy", add: "add", move: "choose", escape: "step back", preview: "show output" };
  const touch = matchMedia("(hover: none)").matches;
  // The app's default shortcut also opens this example, so visitors learn it by using it.
  const opens = mac ? event => event.ctrlKey && event.metaKey && !event.altKey && !event.shiftKey && event.code === "KeyR"
    : event => event.ctrlKey && event.shiftKey && !event.altKey && !event.metaKey && event.code === "KeyY";
  if (touch) hint.textContent = "Tap a row to choose it, or its tile to combine references, then Copy.";
  else hint.append(el("kbd", { textContent: mac ? "⌃⌘R" : "Ctrl+Shift+Y" }), " opens this from anywhere on the page, as in the app. Click a row to choose; double-click to copy; click its tile to combine references. ", ...KEY_TABLE.filter(row => labels[row.action]).flatMap((row, index) => [index ? " · " : "", ...row.keys.flatMap((key, k) => [k ? " " : "", el("kbd", { textContent: keyLabel(key, mac) })]), ` ${labels[row.action]}`]));
  const beak = el("img", { src: "assets/ibis-quote-beak.svg", alt: "", width: 22, height: 19 });
  const shelf = el("div", { className: "picker-shelf" }, beak, output, status);
  const panel = el("div", { className: "picker-panel" }, el("div", { className: "picker-search" }, input, clear, format),
    el("p", { className: "picker-meta" }, "Best matches", count), list, shelf,
    el("div", { className: "picker-foot" }, el("span", {}, "sample.bib · 6 invented references"), add, copy));
  const empty = el("li", { textContent: "Nothing matches. Try an author, title, year or journal.", className: "picker-empty" });
  empty.setAttribute("role", "none"); list.append(empty);
  const rows = new Map(references.map((ref, index) => {
    const tile = el("button", { type: "button", className: "tile", tabIndex: -1 }, icon(ref.kind));
    const row = el("div", { id: `demo-ref-${index}`, className: "pick", title: `${ref.title} · ${ref.authors} · ${ref.year} · ${ref.key}` }, tile,
      el("span", { className: "ref" }, el("strong", { textContent: ref.title }), el("span", { className: "sub", textContent: `${ref.authors} · ${ref.where}` })), el("span", { textContent: String(ref.year) }));
    row.setAttribute("role", "option");
    row.addEventListener("click", event => { state.selected = model.found().indexOf(ref); if (commandKey(event, mac) || event.target.closest(".tile")) model.add(ref.key); render(); input.focus(); });
    row.addEventListener("dblclick", event => { if (!commandKey(event, mac) && !event.target.closest(".tile")) performCopy(); });
    const item = el("li", {}, row); item.setAttribute("role", "none"); list.append(item);
    return [ref.key, { row, item, tile, ref }];
  }));
  // Added references sit inline in the shelf's citation, one focus stop for the group, as in the app.
  let activeKey = null;
  const segment = key => [...output.querySelectorAll('[role="row"]')].find(row => row.dataset.key === key);
  function renderOutput() {
    const pieces = model.pieces(), focused = output.contains(document.activeElement) ? document.activeElement.closest("[data-key]")?.dataset.key : undefined;
    if (!pieces.some(piece => piece.key)) {
      output.removeAttribute("role"); output.removeAttribute("aria-label");
      output.replaceChildren(pieces.map(piece => piece.text).join("") || "—");
    } else {
      output.setAttribute("role", "grid"); output.setAttribute("aria-label", "Selected references");
      const segments = []; let leading = "";
      for (const { text, key } of pieces) {
        if (!key) { if (segments.length) segments.at(-1).firstChild.append(text); else leading += text; continue; }
        const ref = references.find(value => value.key === key);
        const remove = el("button", { type: "button", tabIndex: -1, className: "citation-remove" });
        remove.setAttribute("aria-label", `Remove ${ref.title} from citation`);
        const cell = el("span", {}, leading, el("span", { className: "citation-pill" }, el("span", { className: "citation-text", textContent: text }), remove));
        cell.setAttribute("role", "gridcell"); leading = "";
        const row = el("span", { className: "citation-segment", tabIndex: -1 }, cell);
        row.setAttribute("role", "row"); row.setAttribute("aria-label", `${ref.authors.split(", ")[0]} ${ref.year}`); row.dataset.key = key;
        segments.push(row);
      }
      if (!state.added.includes(activeKey)) activeKey = segments[0].dataset.key;
      for (const row of segments) row.tabIndex = row.dataset.key === activeKey ? 0 : -1;
      output.replaceChildren(...segments);
    }
    if (focused === undefined) return;
    // Rebuilding keeps focus on the same reference, or its neighbour once removed, else Search.
    const row = state.preview && (segment(focused) ?? segment(activeKey));
    if (row) row.focus(); else input.focus({ preventScroll: true });
  }
  function removeSegment(key, keyboard) {
    const index = state.added.indexOf(key);
    model.remove(key); activeKey = state.added[Math.min(index, state.added.length - 1)] ?? null;
    render();
    if (!keyboard) (segment(activeKey) ?? input).focus({ preventScroll: true });
  }
  output.addEventListener("focusin", event => {
    const row = event.target.closest('[role="row"]'); if (!row) return;
    activeKey = row.dataset.key;
    for (const each of output.querySelectorAll('[role="row"]')) each.tabIndex = each === row ? 0 : -1;
  });
  output.addEventListener("click", event => { const remove = event.target.closest(".citation-remove"); if (remove) removeSegment(remove.closest("[data-key]").dataset.key, event.detail === 0); });
  // The group keeps its own keys: Escape returns to Search without stepping the picker back.
  output.addEventListener("keydown", event => {
    const row = event.target.closest('[role="row"]');
    if (!row || event.isComposing || event.keyCode === 229 || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    const segments = [...output.querySelectorAll('[role="row"]')], index = segments.indexOf(row);
    const next = { ArrowLeft: segments[index - 1], ArrowRight: segments[index + 1], Home: segments[0], End: segments.at(-1) };
    if (Object.hasOwn(next, event.key)) next[event.key]?.focus();
    else if (event.key === "Delete" || event.key === "Backspace") { if (!event.repeat) removeSegment(row.dataset.key, true); }
    else if (event.key === "Escape") input.focus();
    else return;
    event.preventDefault(); event.stopPropagation();
  });
  function render() {
    const found = model.found(); state.selected = Math.max(0, Math.min(state.selected, found.length - 1));
    const current = found[state.selected], value = model.output();
    input.value = state.query; panel.hidden = !state.open; reopen.hidden = state.open;
    clear.hidden = !state.query;
    input.setAttribute("aria-expanded", String(state.open));
    count.textContent = `${found.length} ${found.length === 1 ? "result" : "results"}`;
    for (const { row, item, tile, ref } of rows.values()) {
      item.hidden = !found.includes(ref); const selected = ref === current;
      row.classList.toggle("is-selected", selected); row.setAttribute("aria-selected", String(selected));
      const added = state.added.includes(ref.key); tile.classList.toggle("is-added", added); tile.replaceChildren(added ? "✓" : icon(ref.kind));
      tile.setAttribute("aria-label", `${added ? "Remove" : "Add"} ${ref.title}${added ? " from" : " to"} citation`); tile.setAttribute("aria-pressed", String(added));
      row.setAttribute("aria-label", `${ref.title}, ${ref.authors}, ${ref.year}, ${ref.where}${added ? ", added to citation" : ""}`);
    }
    empty.hidden = found.length > 0;
    if (current) input.setAttribute("aria-activedescendant", rows.get(current.key).row.id); else input.removeAttribute("aria-activedescendant");
    output.hidden = beak.hidden = !state.preview; renderOutput(); status.textContent = announce.textContent = value && value === state.copied ? (state.pasted === value ? "Pasted" : "Copied") : "";
    shelf.hidden = !state.preview && !status.textContent;
    copy.disabled = !value; copy.replaceChildren(`Copy as ${formats[state.format]} `, el("kbd", { textContent: keyLabel(keysFor("copy"), mac) }));
    add.disabled = !current; add.textContent = current && state.added.includes(current.key) ? "Remove" : "Add";
    const pasted = state.pasted ? [" ", el("code", { className: "key", textContent: state.pasted })] : [];
    const waiting = state.copied && state.copied !== state.pasted ? [" ", paste] : [];
    const blank = !state.copied ? [" ", el("span", { className: "paste-gap", ariaHidden: "true", textContent: "\u00a0" })] : [];
    result.replaceChildren(sentence, ...pasted, ...waiting, ...blank, ".");
  }
  function performCopy() { if (model.copy()) { render(); if (paste.isConnected) paste.focus(); } }
  function performPaste() { model.paste(); render(); result.focus({ preventScroll: true }); }
  input.addEventListener("input", () => { state.query = input.value; state.selected = 0; render(); list.scrollTop = 0; });
  clear.addEventListener("click", () => { state.query = ""; state.selected = 0; render(); list.scrollTop = 0; input.focus(); });
  select.addEventListener("change", () => { state.format = select.value; render(); input.focus(); });
  copy.addEventListener("click", performCopy);
  add.addEventListener("click", () => { const ref = model.found()[state.selected]; if (ref) model.add(ref.key); render(); });
  paste.addEventListener("click", performPaste);
  figure.closest("ol").addEventListener("keydown", event => {
    if (!event.isComposing && event.keyCode !== 229 && matchesAccelerator(event, "CmdOrCtrl+V", mac) && state.copied && state.copied !== state.pasted && event.target !== input) { event.preventDefault(); performPaste(); }
  });
  reopen.addEventListener("click", () => { state.open = true; render(); input.focus(); });
  document.addEventListener("keydown", event => {
    if (event.isComposing || event.repeat || !opens(event)) return;
    event.preventDefault(); state.open = true; render();
    figure.scrollIntoView({ block: "center", behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "instant" : "smooth" });
    input.focus({ preventScroll: true }); input.select();
  });
  panel.addEventListener("keydown", event => {
    if (event.isComposing || event.keyCode === 229) return;
    if (event.repeat && event.key === "Enter") { event.preventDefault(); return; }
    if (event.target === select) { if (event.key === keysFor("escape")) input.focus(); return; }
    const is = action => matchesAccelerator(event, keysFor(action), mac);
    if (is("escape")) { event.preventDefault(); model.back(); render(); if (!state.open) reopen.focus(); return; }
    if (is("preview")) { event.preventDefault(); if (!event.repeat) { state.preview = !state.preview; render(); } return; }
    if (event.target !== input) return;
    if (is("add")) { event.preventDefault(); const ref = model.found()[state.selected]; if (ref) model.add(ref.key); }
    else if (is("copy")) { event.preventDefault(); performCopy(); return; }
    else if (KEY_TABLE.find(row => row.action === "move").keys.includes(event.key)) { event.preventDefault(); state.selected += event.key === "ArrowDown" ? 1 : -1; }
    else return;
    render();
    const selected = list.querySelector('[aria-selected="true"]');
    if (selected) {
      const row = selected.getBoundingClientRect(), view = list.getBoundingClientRect();
      if (row.top < view.top) list.scrollTop += row.top - view.top;
      else if (row.bottom > view.bottom) list.scrollTop += row.bottom - view.bottom;
    }
  });
  figure.querySelector(".picker-panel").replaceWith(panel);
  figure.append(reopen, hint, announce); render();
  if (anchorBottom < 0) window.scrollBy({ top: section.getBoundingClientRect().bottom - anchorBottom, behavior: "instant" });
}
