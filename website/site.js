// Apply the local preference before paint; the complete page works without it.
try {
  const choice = localStorage.getItem("ibis-theme");
  if (choice === "light" || choice === "dark") document.documentElement.dataset.theme = choice;
} catch { /* Storage is optional. */ }
const platform = navigator.userAgentData?.platform ?? navigator.platform;
if (/Mac/i.test(platform)) document.documentElement.dataset.platform = "macos";
else if (/Win/i.test(platform)) document.documentElement.dataset.platform = "windows";
else if (/Linux/i.test(platform)) document.documentElement.dataset.platform = "linux";

const THEME_KEY = "ibis-theme";
function appearance() {
  const groups = [...document.querySelectorAll(".appearance")], root = document.documentElement;
  const media = matchMedia("(prefers-color-scheme: dark)");
  const dark = () => root.dataset.theme === "dark" || (!root.dataset.theme && media.matches);
  function show() { for (const group of groups) {
    const toggle = group.querySelector('[role="switch"]'), reset = group.querySelector(".appearance-system");
    toggle.setAttribute("aria-checked", String(dark())); toggle.title = `Switch to ${dark() ? "light" : "dark"} appearance`;
    reset.disabled = !root.dataset.theme; reset.textContent = reset.disabled ? reset.dataset.system : reset.dataset.useSystem;
  } }
  function choose(choice) {
    if (choice) root.dataset.theme = choice; else delete root.dataset.theme;
    try { if (choice) localStorage.setItem(THEME_KEY, choice); else localStorage.removeItem(THEME_KEY); } catch { /* Optional storage. */ }
    show();
  }
  for (const group of groups) {
    const toggle = group.querySelector('[role="switch"]'), reset = group.querySelector(".appearance-system");
    toggle.addEventListener("click", () => choose(dark() ? "light" : "dark"));
    reset.addEventListener("click", () => { choose(null); toggle.focus(); });
    group.hidden = false;
  }
  media.addEventListener("change", show); show();
}
function platforms() {
  const touchMac = /Mac/.test(navigator.platform) && navigator.maxTouchPoints > 1;
  if (/Android|iPhone|iPad|iPod/i.test(navigator.userAgent) || touchMac) return;
  const detected = { macos: "mac", windows: "windows", linux: "linux" }[document.documentElement.dataset.platform];
  if (!detected) return;
  const get = document.querySelector("[data-get]");
  if (get?.dataset[detected]) get.textContent = get.dataset[detected];
  if (detected !== "mac") for (const cap of document.querySelectorAll("[data-shortcut-other]")) {
    if (cap.dataset.shortcutOther) cap.textContent = cap.dataset.shortcutOther;
  }
}
function outboundLinks() {
  for (const link of document.querySelectorAll("main a[href], footer a[href], .index-note a[href]")) {
    if (link.closest(".rows, .button, .key")) continue;
    const url = new URL(link.href, location.href);
    if (!/^https?:$/.test(url.protocol) || url.origin === location.origin) continue;
    const mark = document.createElement("span"); mark.className = "outbound"; mark.setAttribute("aria-hidden", "true");
    link.append(mark);
  }
}
const textEntry = target => target?.closest?.('input, textarea, select, [contenteditable]:not([contenteditable="false"])');
// One shortcut owner across all four pages; the app's matcher owns layouts and modifiers.
function pickerKeys({ DEFAULT_PICKER_SHORTCUT, matchesAccelerator }, open) {
  const mac = document.documentElement.dataset.platform === "macos", held = new Map();
  function show(event) {
    for (const cap of document.querySelectorAll("kbd[data-accelerator]")) cap.classList.toggle("pressed", [...held.values()].some(press =>
      matchesAccelerator({ ...press, ctrlKey: event.ctrlKey, metaKey: event.metaKey, altKey: event.altKey, shiftKey: event.shiftKey }, cap.dataset.accelerator, mac)));
  }
  document.addEventListener("keydown", event => {
    if (event.isComposing || event.keyCode === 229) return;
    held.set(event.code || event.key, { key: event.key, code: event.code });
    if (!event.repeat && !textEntry(event.target) && matchesAccelerator(event, DEFAULT_PICKER_SHORTCUT, mac)) {
      event.preventDefault();
      if (document.querySelector("[data-picker]")) open(); else location.assign("/#try");
    }
    show(event);
  });
  document.addEventListener("keyup", event => { held.delete(event.code || event.key); show(event); });
  const release = () => { held.clear(); for (const cap of document.querySelectorAll("kbd.pressed")) cap.classList.remove("pressed"); };
  window.addEventListener("blur", release);
  document.addEventListener("visibilitychange", () => { if (document.hidden) release(); });
}
function peck() {
  const logo = document.querySelector(".logo");
  if (!logo || matchMedia("(prefers-reduced-motion: reduce)").matches || logo.classList.contains("pecking")) return;
  const finish = () => {
    logo.classList.remove("pecking"); logo.removeEventListener("animationend", finish); logo.removeEventListener("animationcancel", finish);
  };
  logo.addEventListener("animationend", finish); logo.addEventListener("animationcancel", finish);
  logo.classList.add("pecking");
}
// The browser's own share sheet, or a copied link where there is none; no third-party buttons.
function share() {
  const button = document.querySelector(".share"), status = document.querySelector(".share-status"), url = "https://ibiscite.org/";
  if (!button || !(navigator.share || navigator.clipboard)) return;
  button.hidden = false;
  button.addEventListener("click", async () => {
    try {
      if (navigator.share) await navigator.share({ title: "Ibis", text: document.querySelector("h1").textContent, url });
      else { await navigator.clipboard.writeText(url); status.textContent = status.dataset.copied; setTimeout(() => { status.textContent = ""; }, 2000); }
    } catch { /* Cancelled or blocked; nothing changed. */ }
  });
}
// Hovering a chip cites it in each form Ibis copies: Pandoc, LaTeX, then Typst's bare key.
function chips() {
  if (!matchMedia("(hover: hover)").matches) return;
  const still = matchMedia("(prefers-reduced-motion: reduce)");
  for (const chip of document.querySelectorAll("main .key:not(code):not(button)")) {
    const key = chip.textContent.slice(1), forms = [`[@${key}]`, `\\cite{${key}}`, `@${key}`];
    let timer;
    chip.addEventListener("pointerenter", () => {
      let step = 0;
      timer = setTimeout(function cycle() { chip.textContent = forms[step++ % forms.length]; if (!still.matches) timer = setTimeout(cycle, 900); }, 150);
    });
    chip.addEventListener("pointerleave", () => { clearTimeout(timer); chip.textContent = `@${key}`; });
  }
}
function navigation() {
  const links = [...document.querySelectorAll('.index nav a[href^="#"]')];
  const targets = links.map(link => document.querySelector(link.getAttribute("href")));
  const bar = document.querySelector(".index nav ul");
  const edge = () => { bar.toggleAttribute("data-at-end", bar.scrollLeft + bar.clientWidth >= bar.scrollWidth - 1); bar.toggleAttribute("data-at-start", bar.scrollLeft <= 1); };
  bar.addEventListener("scroll", edge, { passive: true });
  window.addEventListener("resize", edge); edge();
  let pending = false, pinned = null;
  // A followed link keeps its own section current until the reader scrolls, even when the page end is reached.
  document.addEventListener("click", event => { const target = event.target.closest?.('a[href^="#"]'); pinned = target && targets.find(section => `#${section.id}` === target.getAttribute("href")) || null; });
  for (const type of ["wheel", "touchmove", "keydown"]) window.addEventListener(type, () => { pinned = null; }, { passive: true });
  function show() {
    pending = false;
    const visible = target => { const box = target.getBoundingClientRect(); return box.top < window.innerHeight && box.bottom > 0; };
    const bottom = window.scrollY + window.innerHeight >= document.documentElement.scrollHeight - 1;
    const current = pinned && visible(pinned) ? pinned : bottom ? targets.at(-1) : targets.filter(target => target.getBoundingClientRect().top <= window.innerHeight * .25).at(-1) || targets[0];
    links.forEach((link, index) => { if (targets[index] === current) {
      if (!link.hasAttribute("aria-current")) {
        link.setAttribute("aria-current", "location");
        // Scroll the chip bar sideways only; scrollIntoView would also move the page.
        const chip = link.getBoundingClientRect(), view = bar.getBoundingClientRect(), room = 16;
        if (chip.left < view.left) bar.scrollLeft += chip.left - view.left - room;
        else if (chip.right > view.right) bar.scrollLeft += chip.right - view.right + room;
      }
    } else link.removeAttribute("aria-current"); });
  }
  const schedule = () => { if (!pending) { pending = true; requestAnimationFrame(show); } };
  window.addEventListener("scroll", schedule, { passive: true }); window.addEventListener("resize", schedule); show();
}
// Where you write: each tab rewrites the citation in the sentence. The explanation lives in the panel, never on the tabs.
function writingApps() {
  const where = document.querySelector("[data-where]");
  if (!where) return;
  const tabs = [...where.querySelectorAll('[role="tab"]')], panel = where.querySelector('[role="tabpanel"]');
  const output = panel.querySelector("[data-out]"), explanation = panel.querySelector("[data-explanation]");
  function show(tab) {
    for (const item of tabs) { item.setAttribute("aria-selected", String(item === tab)); item.tabIndex = item === tab ? 0 : -1; }
    panel.setAttribute("aria-labelledby", tab.id);
    output.textContent = tab.dataset.output; explanation.textContent = tab.dataset.explain;
    output.classList.remove("landed"); void output.offsetWidth; output.classList.add("landed");
  }
  tabs.forEach((tab, i) => {
    tab.addEventListener("click", () => show(tab));
    tab.addEventListener("keydown", event => {
      const next = { ArrowRight: (i + 1) % tabs.length, ArrowLeft: (i + tabs.length - 1) % tabs.length, Home: 0, End: tabs.length - 1 }[event.key];
      if (next === undefined) return;
      event.preventDefault(); tabs[next].focus(); show(tabs[next]);
    });
  });
}
// Copy a command: the button says Copied for a moment, and screen readers hear it.
function copyButtons() {
  if (!navigator.clipboard) { document.querySelectorAll(".copy").forEach(button => { button.hidden = true; }); return; }
  for (const button of document.querySelectorAll(".copy")) button.addEventListener("click", async () => {
    const label = button.querySelector("span"), idle = label.textContent;
    try { await navigator.clipboard.writeText(button.dataset.copy); label.textContent = button.dataset.copied; button.dataset.state = "copied"; peck(); }
    catch { label.textContent = button.dataset.failed; }
    setTimeout(() => { label.textContent = idle; delete button.dataset.state; }, 1500);
  });
}
// A teaching sketch of the typed shorthand; the app's engine owns the real parser.
function shorthand() {
  const box = document.querySelector("[data-shorthand]");
  if (!box) return;
  const refs = JSON.parse(box.dataset.references);
  const input = box.querySelector("[data-shorthand-input]"), parts = box.querySelector("[data-shorthand-parts]"), out = box.querySelector("[data-shorthand-out]");
  // Each typed word over what it means, underlined by its role.
  const gloss = (role, typed, meaning) => { const item = document.createElement("li"); item.dataset.role = role; const word = document.createElement("b"), note = document.createElement("span"); word.textContent = typed; note.textContent = meaning; item.append(word, note); return item; };
  function render() {
    const words = input.value.trim().toLowerCase().split(/\s+/).filter(Boolean), terms = [];
    let prefix, locator, typedLocator;
    words.forEach((word, i) => {
      if (i === 0 && ["see", "cf.", "cf", "e.g."].includes(word)) prefix = word === "cf" ? "cf." : word;
      else if (["ch", "chap", "chapter"].includes(word) && /^\d+$/.test(words[i + 1] || "")) { locator = `chap. ${words[i + 1]}`; typedLocator = `${word} ${words[i + 1]}`; }
      else if (/^\d+([-–]\d+)?$/.test(word)) { if (!locator?.startsWith("chap")) { locator = /[-–]/.test(word) ? `pp. ${word.replace("-", "–")}` : `p. ${word}`; typedLocator = word; } }
      else terms.push(word);
    });
    const ref = terms.length ? refs.find(r => terms.every(t => r.words.includes(t))) : undefined;
    if (!ref) { parts.replaceChildren(parts.dataset.empty); out.textContent = out.dataset.empty; return; }
    parts.replaceChildren(...[prefix && gloss("before", prefix, parts.dataset.before), gloss("reference", terms.join(" "), ref.label), locator && gloss("locator", typedLocator, locator)].filter(Boolean));
    out.textContent = `[${prefix ? prefix + " " : ""}@${ref.key}${locator ? ", " + locator : ""}]`;
    out.classList.remove("landed"); void out.offsetWidth; out.classList.add("landed");
  }
  const buttons = [...box.querySelectorAll("[data-try]")], examples = [...buttons.map(button => button.textContent.trim()), input.value];
  const still = matchMedia("(prefers-reduced-motion: reduce)"), pace = parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--dur-1"));
  let stopped = document.activeElement === input, timer, index = 0, resume = begin;
  function schedule(next, delay) {
    clearTimeout(timer); resume = next;
    if (!stopped && !still.matches && !document.hidden) timer = setTimeout(next, delay);
  }
  function begin() {
    if (stopped || still.matches || document.hidden) return;
    input.value = ""; render();
    const example = examples[index];
    function type() {
      if (stopped || still.matches || document.hidden) return;
      input.value = example.slice(0, input.value.length + 1); render();
      if (input.value === example) { index = (index + 1) % examples.length; schedule(begin, 3000); }
      else schedule(type, pace * (.65 + Math.random() * .7));
    }
    schedule(type, pace);
  }
  const stop = () => { stopped = true; clearTimeout(timer); };
  input.addEventListener("focus", stop);
  input.addEventListener("input", () => { stop(); render(); });
  buttons.forEach(button => button.addEventListener("click", () => { stop(); input.value = button.textContent; render(); }));
  for (const event of ["pointerdown", "pointermove", "keydown", "scroll"]) document.addEventListener(event, () => schedule(begin, 3000), { passive: true });
  document.addEventListener("visibilitychange", () => schedule(resume, 3000));
  still.addEventListener("change", () => schedule(resume, 3000));
  render();
  schedule(begin, 3000);
}
// A link from another page lands on its section at once; smooth scrolling is only for moves within the page.
globalThis.addEventListener?.("load", () => requestAnimationFrame(() => document.documentElement.classList.add("smooth")));
document.addEventListener("DOMContentLoaded", () => {
  if (!document.querySelector(".index nav")) return; // Plain pages such as 404 need only the appearance applied above.
  appearance(); platforms(); outboundLinks(); navigation(); share(); chips(); writingApps(); copyButtons(); shorthand();
  console.info("Curious how this page works? Ibis is open source: https://github.com/ibiscite/ibis");
  let lastMove = performance.now();
  const markMove = () => { lastMove = performance.now(); };
  window.addEventListener("scroll", markMove, { passive: true }); window.addEventListener("hashchange", markMove);
  document.addEventListener("click", event => { if (event.target.closest?.('a[href^="#"]')) markMove(); });
  if (!matchMedia("(prefers-reduced-motion: reduce)").matches && "IntersectionObserver" in window) {
    const observer = new IntersectionObserver(entries => {
      for (const entry of entries) if (entry.isIntersecting) { entry.target.classList.add("reveal"); observer.unobserve(entry.target); }
    }, { threshold: .1 });
    document.querySelectorAll("main section").forEach(section => observer.observe(section));
  }
  let replay = 0;
  document.querySelector(".logo").addEventListener("click", event => {
    if (matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    for (const image of event.currentTarget.querySelectorAll("img")) image.src = image.src.split("#")[0] + `#again-${++replay}`;
  });
  const demo = document.querySelector("[data-picker]") ? import("./demo.js").then(async ({ setupDemo }) => {
    const quiet = parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--dur-2"));
    await new Promise(resolve => {
      const wait = () => { if (performance.now() - lastMove < quiet) requestAnimationFrame(wait); else resolve(); };
      requestAnimationFrame(wait);
    });
    const picker = setupDemo({ onCopy: peck, onPaste: peck });
    if (location.hash === "#try" && !textEntry(document.activeElement)) picker.open();
    return picker;
  }).catch(() => { /* The static illustration remains available. */ }) : Promise.resolve();
  import("./keys.js").then(keys => pickerKeys(keys, () => demo.then(picker => {
    if (!textEntry(document.activeElement)) picker?.open();
  }))).catch(() => { /* The page and static key caps remain available. */ });
});
