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
  const list = el("ul", { id: "demo-results", className: "picker-results" });
  list.setAttribute("role", "listbox"); list.setAttribute("aria-label", "Best matches");
  const count = el("span"), output = el("code"), status = el("span"); status.setAttribute("role", "status");
  const copy = el("button", { type: "button", textContent: "Copy" }), add = el("button", { type: "button", textContent: "Add" });
  const paste = el("button", { type: "button", className: "paste-target", textContent: "Paste here" });
  paste.setAttribute("aria-label", "Paste the copied citation into this example");
  const reopen = el("button", { type: "button", textContent: "Open the picker again", hidden: true });
  const hint = el("p", { className: "demo-hint" });
  const labels = { copy: "copy", add: "add", move: "choose", escape: "step back", preview: "show output" };
  hint.textContent = "Click a row to choose; double-click to copy. Click its tile or use Add to combine references. " + KEY_TABLE.filter(row => labels[row.action]).map(row => `${row.keys.map(key => keyLabel(key, mac)).join(" / ")} ${labels[row.action]}`).join(" · ") + ".";
  const shelf = el("div", { className: "picker-shelf" }, el("img", { src: "assets/ibis-quote-beak.svg", alt: "", width: 22, height: 19 }), output);
  const panel = el("div", { className: "picker-panel" }, el("div", { className: "picker-search" }, input, clear, select),
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
  function render() {
    const found = model.found(); state.selected = Math.max(0, Math.min(state.selected, found.length - 1));
    const current = found[state.selected], value = model.output();
    input.value = state.query; panel.hidden = !state.open; reopen.hidden = state.open; shelf.hidden = !state.preview;
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
    output.textContent = value || "—"; status.textContent = state.pasted ? "Pasted in demo" : value && value === state.copied ? "Copied in demo" : "";
    copy.disabled = !value; copy.textContent = `Copy as ${formats[state.format]} ${keyLabel(keysFor("copy"), mac)}`;
    add.disabled = !current; add.textContent = current && state.added.includes(current.key) ? "Remove" : "Add";
    paste.hidden = !state.copied || Boolean(state.pasted);
    if (!state.copied) result.replaceChildren(`${sentence} `, el("span", { className: "paste-gap", ariaHidden: "true", textContent: "\u00a0" }), ".");
    else result.replaceChildren(`${sentence} `, state.pasted ? el("code", { className: "key", textContent: state.pasted }) : paste, ".");
  }
  function performCopy() { if (model.copy()) { render(); paste.focus(); } }
  input.addEventListener("input", () => { state.query = input.value; state.selected = 0; render(); list.scrollTop = 0; });
  clear.addEventListener("click", () => { state.query = ""; state.selected = 0; render(); list.scrollTop = 0; input.focus(); });
  select.addEventListener("change", () => { state.format = select.value; render(); input.focus(); });
  copy.addEventListener("click", performCopy);
  add.addEventListener("click", () => { const ref = model.found()[state.selected]; if (ref) model.add(ref.key); render(); });
  paste.addEventListener("click", () => { model.paste(); render(); result.focus({ preventScroll: true }); });
  reopen.addEventListener("click", () => { state.open = true; render(); input.focus(); });
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
  figure.append(status, reopen, hint); render();
  if (anchorBottom < 0) window.scrollBy({ top: section.getBoundingClientRect().bottom - anchorBottom, behavior: "instant" });
}
