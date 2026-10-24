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
    reset.disabled = !root.dataset.theme; reset.textContent = reset.disabled ? "System" : "Use system";
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
  const blocks = [...document.querySelectorAll(".platform[data-os]")];
  const touchMac = /Mac/.test(navigator.platform) && navigator.maxTouchPoints > 1;
  if (/Android|iPhone|iPad|iPod/i.test(navigator.userAgent) || touchMac) return;
  const detected = { macos: "mac", windows: "windows", linux: "linux" }[document.documentElement.dataset.platform];
  if (!detected) return;
  const names = { mac: "Mac", windows: "Windows", linux: "Linux" }, choice = document.createElement("p");
  choice.className = "platform-switch"; blocks[0].before(choice);
  let selected = detected;
  function show(os, focus = false) {
    selected = os;
    if (matchMedia("(max-width: 61.99rem)").matches) { blocks.forEach(block => { block.hidden = false; }); choice.hidden = true; document.querySelector("[data-get]").textContent = "Get the early preview"; return; }
    choice.hidden = false;
    blocks.forEach(block => { block.hidden = block.dataset.os !== os; });
    const buttons = Object.keys(names).filter(value => value !== os).map(value => {
      const button = document.createElement("button"); button.type = "button"; button.textContent = names[value];
      button.addEventListener("click", () => show(value, true)); return button;
    });
    choice.replaceChildren(`For ${names[os]}${os === detected ? ", your system" : ""}. Also for `, buttons[0], " and ", buttons[1], ".");
    document.querySelector("[data-get]").textContent = `Get Ibis for ${names[os]}`;
    if (focus) buttons[0].focus();
  }
  show(detected);
  matchMedia("(max-width: 61.99rem)").addEventListener("change", () => show(selected));
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
    const current = pinned && visible(pinned) ? pinned : bottom ? targets.at(-1) : targets.filter(target => target.getBoundingClientRect().top <= window.innerHeight * .25).at(-1);
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
document.addEventListener("DOMContentLoaded", () => {
  if (!document.querySelector(".index nav")) return; // Plain pages such as 404 need only the appearance applied above.
  appearance(); platforms(); navigation();
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
  import("./demo.js").then(async ({ setupDemo }) => {
    const quiet = parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--motion-panel"));
    await new Promise(resolve => {
      const wait = () => { if (performance.now() - lastMove < quiet) requestAnimationFrame(wait); else resolve(); };
      requestAnimationFrame(wait);
    });
    setupDemo();
  }).catch(() => { /* The static illustration remains available. */ });
});
