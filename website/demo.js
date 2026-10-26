import { DEFAULT_PICKER_SHORTCUT, KEY_TABLE, keysFor, keyLabel, commandKey, matchesAccelerator } from "./keys.js";
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
  // The hosted demo has no native updater; like an app build without an
  // update key, it omits the update controls offered in the app's More menu.
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
  const count = el("span"), output = el("code"), status = el("span", { className: "picker-status" });
  const interpretationText = el("bdi"), literal = el("button", { type: "button", textContent: "Search as typed" });
  const apply = el("button", { type: "button" });
  const interpretation = el("div", { className: "picker-locator" }, interpretationText, literal, apply);
  const locatorTokens = el("div", { className: "picker-locators" }); locatorTokens.setAttribute("role", "group"); locatorTokens.setAttribute("aria-label", "Selected reference locators");
  const warnings = el("p", { className: "picker-meta" });
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
  // The event key includes the layout’s Command layer (Dvorak–QWERTY ⌘);
  // only a layer without Latin letters keeps its US place (#196).
  const opens = event => matchesAccelerator(event, DEFAULT_PICKER_SHORTCUT, mac);
  if (touch) hint.textContent = "Tap a row to choose it, or its tile to combine references, then Copy.";
  else hint.append(el("kbd", { textContent: keyLabel(DEFAULT_PICKER_SHORTCUT, mac) }), " opens this from anywhere on the page, as in the app. Click a row to choose; double-click to copy; click its tile to combine references. ", ...KEY_TABLE.filter(row => labels[row.action]).flatMap((row, index) => [index ? " · " : "", ...row.keys.flatMap((key, k) => [k ? " " : "", el("kbd", { textContent: keyLabel(key, mac) })]), ` ${labels[row.action]}`]));
  const beak = el("img", { src: "assets/ibis-quote-beak.svg", alt: "", width: 22, height: 19 });
  const shelf = el("div", { className: "picker-shelf" }, beak, output, status);
  const panel = el("div", { className: "picker-panel" }, el("div", { className: "picker-search" }, input, clear, format),
    el("p", { className: "picker-meta" }, "Best matches", count), interpretation, list, locatorTokens, shelf, warnings,
    el("div", { className: "picker-foot" }, el("span", {}, "sample.bib · 6 invented references"), add, copy));
  const empty = el("li", { textContent: "Nothing matches. Try an author, title, year or journal.", className: "picker-empty" });
  empty.setAttribute("role", "none"); list.append(empty);
  const rows = new Map(references.map((ref, index) => {
    const tile = el("button", { type: "button", className: "tile", tabIndex: -1 }, icon(ref.kind));
    const row = el("div", { id: `demo-ref-${index}`, className: "pick", title: `${ref.title} · ${ref.authors} · ${ref.year} · ${ref.key}` }, tile,
      el("span", { className: "ref" }, el("strong", { textContent: ref.title }), el("span", { className: "sub", textContent: `${ref.authors} · ${ref.where}` })), el("span", { textContent: String(ref.year) }));
    row.setAttribute("role", "option");
    // Match the app's pointer ownership: a click leaves its row under the
    // pointer. Rendering is synchronous here; only the keyboard handler below
    // reveals a chosen row by scrolling (docs/design.md, "List scrolling").
    row.addEventListener("click", event => { state.selected = model.found().indexOf(ref); if (commandKey(event, mac) || event.target.closest(".tile")) model.add(ref.key); render(); input.focus(); });
    row.addEventListener("dblclick", event => { if (!commandKey(event, mac) && !event.target.closest(".tile")) performCopy(); });
    const item = el("li", {}, row); item.setAttribute("role", "none"); list.append(item);
    return [ref.key, { row, item, tile, ref }];
  }));
  function render() {
    const found = model.found(); state.selected = Math.max(0, Math.min(state.selected, found.length - 1));
    const current = found[state.selected], rendered = model.rendered(), value = rendered.text, parsed = model.interpretation();
    interpretation.hidden = !parsed;
    interpretationText.textContent = parsed ? `Using ${parsed.locator.label} ${parsed.locator.value}` : "";
    literal.setAttribute("aria-label", "Search as typed");
    const captured = current ? state.locators[current.key] : null;
    apply.hidden = !parsed || !current || !state.added.includes(current.key) || captured?.label === parsed.locator.label && captured?.value === parsed.locator.value;
    apply.textContent = current ? `Apply locator to ${current.key}` : "";
    locatorTokens.replaceChildren(...state.added.flatMap(key => {
      const locator = state.locators[key];
      if (!locator) return [];
      const text = `${locator.label} ${locator.value}`, remove = el("button", { type: "button", className: "locator-token-remove", textContent: "×" });
      remove.setAttribute("aria-label", `Remove ${text} from ${key}`);
      remove.addEventListener("click", () => { model.removeLocator(key); render(); input.focus(); });
      return [el("span", { className: "locator-token", title: `${key}: ${text}` }, el("bdi", { textContent: `${key}: ${text}` }), remove)];
    }));
    locatorTokens.hidden = !locatorTokens.childElementCount;
    warnings.textContent = rendered.warnings.join(" "); warnings.hidden = !rendered.warnings.length;
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
    output.textContent = value || "—"; status.textContent = value && value === state.copied ? (state.pasted === value ? "Pasted" : "Copied") : "";
    announce.textContent = [status.textContent, warnings.textContent].filter(Boolean).join(". ");
    output.hidden = beak.hidden = !state.preview; shelf.hidden = !state.preview && !status.textContent;
    copy.disabled = !value; copy.replaceChildren(`Copy as ${formats[state.format]} `, el("kbd", { textContent: keyLabel(keysFor("copy"), mac) }));
    add.disabled = !current; add.textContent = current && state.added.includes(current.key) ? "Remove" : "Add";
    const pasted = state.pasted ? [" ", el("code", { className: "key", textContent: state.pasted })] : [];
    const waiting = state.copied && state.copied !== state.pasted ? [" ", paste] : [];
    const blank = !state.copied ? [" ", el("span", { className: "paste-gap", ariaHidden: "true", textContent: "\u00a0" })] : [];
    result.replaceChildren(sentence, ...pasted, ...waiting, ...blank, ".");
  }
  function performCopy() { if (model.copy()) { render(); if (paste.isConnected) paste.focus(); } }
  function performPaste() { model.paste(); render(); result.focus({ preventScroll: true }); }
  input.addEventListener("input", () => { model.setQuery(input.value); render(); list.scrollTop = 0; });
  clear.addEventListener("click", () => { model.setQuery(""); render(); list.scrollTop = 0; input.focus(); });
  literal.addEventListener("click", () => { model.searchAsTyped(); render(); list.scrollTop = 0; input.focus(); });
  apply.addEventListener("click", () => { const ref = model.found()[state.selected]; if (ref) model.applyLocator(ref.key); render(); input.focus(); });
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
