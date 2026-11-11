import { KEY_TABLE, keysFor, keyLabel, keyCapLabels, commandKey, matchesAccelerator } from "./keys.js";
import { typePaths, iconPaths } from "./icon-paths.js";
import { createDemo, references, formats } from "./demo-model.mjs";
const el = (tag, props = {}, ...children) => { const node = Object.assign(document.createElement(tag), props); node.append(...children); return node; };
function icon(kind, className = "") {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 20 20"); svg.setAttribute("aria-hidden", "true");
  svg.setAttribute("class", className);
  for (const drawing of typePaths[kind] ?? [iconPaths[kind]]) { const path = document.createElementNS(svg.namespaceURI, "path"); path.setAttribute("d", drawing); svg.append(path); }
  return svg;
}
export function setupDemo({ onCopy = () => {}, onPaste = () => {} } = {}) {
  // The hosted demo has no native updater; like an app build without an
  // update key, it omits the update controls offered in the app's More menu.
  const figure = document.querySelector("[data-picker]"), result = document.querySelector("[data-result]");
  if (!figure || !result) return;
  const words = (name, values = {}) => figure.dataset[name].replaceAll(/\{(\w+)\}/g, (_, key) => values[key]);
  const section = figure.closest("section"), anchorBottom = section.getBoundingClientRect().bottom;
  const model = createDemo(), state = model.state, platform = document.documentElement.dataset.platform ?? "linux", mac = platform === "macos";
  const sentence = result.firstChild.textContent.trim();
  result.tabIndex = -1;
  const input = el("input", { type: "search", value: state.query, placeholder: words("searchPlaceholder"), autocomplete: "off" });
  const clear = el("button", { type: "button", className: "picker-clear" }, icon("close")); clear.setAttribute("aria-label", words("clear"));
  for (const [key, value] of Object.entries({ role: "combobox", "aria-label": words("searchLabel"), "aria-autocomplete": "list", "aria-controls": "demo-results", "aria-expanded": "true" })) input.setAttribute(key, value);
  const select = el("select", {}, ...Object.entries(formats).map(([value, label]) => el("option", { value, textContent: label })));
  select.setAttribute("aria-label", words("formatLabel"));
  // A drawn chevron beside the select: native arrows ignore its height on iOS, and Safari drops background chevrons.
  const format = el("label", { className: "picker-format" }, select);
  format.insertAdjacentHTML("beforeend", '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="m4.5 6.5 3.5 3.5 3.5-3.5"/></svg>');
  const list = el("ul", { id: "demo-results", className: "picker-results" });
  list.setAttribute("role", "listbox");
  const count = el("span"), output = el("code"), statusLabel = el("span"), status = el("span", { className: "picker-status" }, icon("check"), statusLabel);
  output.dir = "ltr";
  // Keep the live region mounted so the first copy is announced.
  const announce = el("span", { className: "demo-status" }); announce.setAttribute("role", "status"); status.setAttribute("aria-hidden", "true");
  const copy = el("button", { type: "button", className: "picker-primary" }), add = el("button", { type: "button" });
  function keyCaps(accelerator) {
    const caps = el("span", { className: "key-caps" }); caps.setAttribute("aria-hidden", "true");
    for (const label of keyCapLabels(accelerator, mac)) {
      const cap = el("span", { className: "key-cap" });
      const kbd = el("kbd", { textContent: label });
      kbd.dataset.accelerator = accelerator;
      kbd.dataset.label = label; if ([...label].length === 1) kbd.dataset.single = "";
      cap.append(kbd); caps.append(cap);
    }
    return caps;
  }
  function actionLabel(button, label, command) {
    const accelerator = keysFor(command);
    button.setAttribute("aria-label", label);
    button.setAttribute("aria-keyshortcuts", accelerator);
    button.title = `${label} (${keyLabel(accelerator, mac)})`;
    button.replaceChildren(el("span", { className: "picker-action-label", textContent: label }), keyCaps(accelerator));
  }
  const paste = el("button", { type: "button", className: "paste-slot" }, words("paste"), " ", el("kbd", { textContent: keyLabel("CmdOrCtrl+V", mac) }));
  paste.setAttribute("aria-label", words("pasteLabel"));
  const reopen = el("button", { type: "button", textContent: words("reopen"), hidden: true });
  const hint = el("p", { className: "demo-hint" });
  const labels = Object.fromEntries(["copy", "add", "move", "escape", "preview"].map(action => [action, words(`legend${action[0].toUpperCase()}${action.slice(1)}`)]));
  const touch = matchMedia("(hover: none)").matches;
  hint.hidden = touch;
  hint.append(...KEY_TABLE.filter(row => labels[row.action]).flatMap((row, index) => [index ? " · " : "", ...row.keys.flatMap((key, k) => [k ? " " : "", el("kbd", { textContent: keyLabel(key, mac) })]), ` ${labels[row.action]}`]));
  if (!touch) {
    const accelerators = KEY_TABLE.filter(row => labels[row.action]).flatMap(row => row.keys);
    hint.querySelectorAll("kbd").forEach((cap, index) => { cap.dataset.accelerator = accelerators[index]; });
  }
  const decoration = className => { const node = el("span", { className }); node.setAttribute("aria-hidden", "true"); return node; };
  const mark = decoration("picker-mark"), beak = decoration("picker-beak"), closingBeak = decoration("picker-beak picker-beak-end");
  mark.append(el("img", { className: "logo-light", src: "assets/ibis-mark.svg", alt: "", width: 24, height: 24 }),
    el("img", { className: "logo-dark", src: "assets/ibis-mark-on-dark.svg", alt: "", width: 24, height: 24 }));
  const citation = el("div", { className: "picker-citation" }, beak, output, closingBeak);
  const shelf = el("div", { className: "picker-shelf" }, citation, status);
  const headingLabel = el("span"), heading = el("p", { className: "picker-meta" }, headingLabel, count);
  const more = el("span", { className: "picker-static-control picker-more" }, el("span", { className: "picker-action-label", textContent: words("more") }), keyCaps("CmdOrCtrl+K"));
  const actions = el("div", { className: "picker-actions" }, add, copy, more);
  actions.setAttribute("role", "group"); actions.setAttribute("aria-label", words("actionsLabel"));
  const panel = el("div", { className: "picker-panel" }, el("div", { className: "picker-search" },
    el("div", { className: "picker-field" }, mark, input), el("div", { className: "picker-controls" }, clear, format)),
    el("div", { className: "picker-middle" }, heading, list), shelf,
    el("div", { className: "picker-foot" }, el("span", { className: "picker-library", title: words("library", { count: references.length }) }, decoration("picker-dot"), el("span", {}, words("library", { count: references.length }))), actions));
  panel.setAttribute("data-app-tokens", ""); panel.dataset.platform = "macos";
  // C12 source choices require an actually empty or missing library. This
  // fixed invented library stays loaded; an unmatched query keeps search
  // feedback and any references already chosen, as in the app (#949).
  const clearEmpty = el("button", { type: "button", textContent: words("clear") });
  const empty = el("div", { className: "picker-empty" }, el("strong", { textContent: words("noMatches") }),
    el("p", { textContent: words("searchHelp") }), clearEmpty);
  empty.insertAdjacentHTML("afterbegin", '<svg viewBox="0 0 48 48" aria-hidden="true"><circle cx="20" cy="20" r="12"/><path d="m29 29 12 12"/></svg>');
  panel.querySelector(".picker-middle").append(empty);
  const rows = new Map(references.map((ref, index) => {
    const tile = el("button", { type: "button", className: "tile", tabIndex: -1 }, icon(ref.kind));
    const row = el("div", { id: `demo-ref-${index}`, className: "pick", title: `${ref.title} · ${ref.authors} · ${ref.year} · ${ref.key}` }, tile,
      el("span", { className: "ref" }, el("strong", { textContent: ref.title }), el("span", { className: "sub", textContent: `${ref.authors} · ${ref.where}` })), el("span", { className: "ref-year", textContent: String(ref.year) }));
    row.setAttribute("role", "option");
    // Match the app's pointer ownership: a click leaves its row under the
    // pointer. Rendering is synchronous here; only the keyboard handler below
    // reveals a chosen row by scrolling (docs/design/picker.md, "List scrolling").
    row.addEventListener("click", event => { state.selected = model.found().indexOf(ref); if (commandKey(event, mac) || event.target.closest(".tile")) model.add(ref.key); render(); input.focus(); });
    row.addEventListener("dblclick", event => { if (!commandKey(event, mac) && !event.target.closest(".tile")) performCopy(); });
    const item = el("li", {}, row); item.setAttribute("role", "none"); list.append(item);
    return [ref.key, { row, item, tile, ref }];
  }));
  function render() {
    const found = model.found(); state.selected = Math.max(0, Math.min(state.selected, found.length - 1));
    const current = found[state.selected], value = model.output();
    input.value = state.query; panel.hidden = !state.open; reopen.hidden = state.open;
    clear.hidden = !state.query;
    input.setAttribute("aria-expanded", String(state.open && found.length > 0));
    headingLabel.textContent = words(state.query.trim() ? "headingQuery" : "headingEmpty");
    list.setAttribute("aria-label", headingLabel.textContent);
    count.textContent = words(found.length === 1 ? "countOne" : "countMany", { count: found.length });
    for (const { row, item, tile, ref } of rows.values()) {
      item.hidden = !found.includes(ref); const selected = ref === current;
      row.classList.toggle("is-selected", selected); row.setAttribute("aria-selected", String(selected));
      const added = state.added.includes(ref.key); tile.classList.toggle("is-added", added); tile.replaceChildren(...(added ? [icon("check")] : [icon(ref.kind, "tile-kind"), icon("plus", "tile-add")]));
      tile.setAttribute("aria-label", words(added ? "removeNamed" : "addNamed", ref)); tile.setAttribute("aria-pressed", String(added));
      row.setAttribute("aria-label", words(added ? "rowAdded" : "row", ref));
    }
    empty.hidden = found.length > 0;
    list.hidden = !found.length;
    heading.hidden = !found.length;
    if (current) input.setAttribute("aria-activedescendant", rows.get(current.key).row.id); else input.removeAttribute("aria-activedescendant");
    output.textContent = value || "—"; output.title = value;
    status.hidden = !value || value !== state.copied;
    statusLabel.textContent = status.hidden ? "" : words("copied");
    closingBeak.hidden = !state.preview || status.hidden;
    shelf.toggleAttribute("data-copied", !status.hidden);
    const feedback = state.copied ? words(state.pasted === state.copied ? "announcePasted" : "announceCopied", { citation: state.copied }) : "";
    if (announce.textContent !== feedback) announce.textContent = feedback;
    citation.hidden = !state.preview; shelf.hidden = !value || (!state.preview && status.hidden);
    copy.disabled = !value; actionLabel(copy, words(state.added.length > 1 ? "copyMany" : "copyOne", { count: state.added.length, format: formats[state.format] }), "copy");
    add.disabled = !current; actionLabel(add, words(current && state.added.includes(current.key) ? "remove" : "add"), "add");
    const pasted = state.pasted ? [" ", el("code", { className: "key", textContent: state.pasted })] : [];
    const waiting = state.copied && state.copied !== state.pasted ? [" ", paste] : [];
    const blank = !state.copied ? [" ", el("span", { className: "paste-gap", ariaHidden: "true", textContent: "\u00a0" })] : [];
    result.replaceChildren(sentence, ...pasted, ...waiting, ...blank, ".");
  }
  function performCopy() {
    if (!model.copy()) return;
    render(); onCopy(); if (paste.isConnected) paste.focus();
  }
  function performPaste() { model.paste(); render(); onPaste(); result.focus({ preventScroll: true }); }
  input.addEventListener("input", () => { model.setQuery(input.value); render(); list.scrollTop = 0; });
  clear.addEventListener("click", () => { model.setQuery(""); render(); list.scrollTop = 0; input.focus(); });
  clearEmpty.addEventListener("click", () => { model.setQuery(""); render(); list.scrollTop = 0; input.focus(); });
  select.addEventListener("change", () => { state.format = select.value; render(); });
  copy.addEventListener("click", performCopy);
  add.addEventListener("click", () => { const ref = model.found()[state.selected]; if (ref) model.add(ref.key); render(); });
  paste.addEventListener("click", performPaste);
  figure.closest("ol").addEventListener("keydown", event => {
    if (!event.isComposing && event.keyCode !== 229 && matchesAccelerator(event, "CmdOrCtrl+V", mac) && state.copied && state.copied !== state.pasted && event.target !== input) { event.preventDefault(); performPaste(); }
  });
  reopen.addEventListener("click", () => { state.open = true; render(); input.focus(); });
  function open() {
    state.open = true; render();
    figure.scrollIntoView({ block: "center", behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "instant" : "smooth" });
    input.focus({ preventScroll: true }); input.select();
  }
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
    else if (KEY_TABLE.find(row => row.action === "move").keys.some(key => matchesAccelerator(event, key, mac))) { event.preventDefault(); state.selected += event.key === "ArrowDown" ? 1 : -1; }
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
  return { open };
}
