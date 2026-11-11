// Interactive explainers below the demo. Invented references only; nothing
// here reads a library or the clipboard.
(() => {
  const refs = [
    { key: "vale2024lanterns", author: "Vale", year: 2024, words: "vale lantern lanterns gardens" },
    { key: "fen2023clouds", author: "Fen", year: 2023, words: "fen clouds clockwork" },
    { key: "reed2025maps", author: "Reed and Vale", year: 2025, words: "reed maps islands sea" },
  ];

  const apps = {
    word: ["(Vale, 2024, p. 12)", "Word, Pages or Google Docs: a formatted citation in APA, as rich text. Change the style once and Ibis remembers it for that app."],
    markdown: ["[@vale2024lanterns, p. 12]", "Markdown, Obsidian or Quarto: a Pandoc citation key. Pandoc or Quarto builds the reference list when you export."],
    latex: ["\\autocite[12]{vale2024lanterns}", "LaTeX or Overleaf: a BibLaTeX command, or \\cite and natbib if you prefer. LaTeX builds the bibliography."],
    typst: ["@vale2024lanterns[p. 12]", "Typst: a Typst citation. Typst formats it in your document’s style."],
  };

  const where = document.querySelector("[data-where]");
  if (where) {
    const out = where.querySelector("[data-out]");
    const note = where.querySelector("[data-note]");
    const tabs = [...where.querySelectorAll("[data-app]")];
    const show = tab => {
      for (const t of tabs) { t.setAttribute("aria-selected", String(t === tab)); t.tabIndex = t === tab ? 0 : -1; }
      const [text, explain] = apps[tab.dataset.app];
      out.textContent = text; note.textContent = explain;
      out.classList.remove("pop"); void out.offsetWidth; out.classList.add("pop");
    };
    tabs.forEach((tab, i) => {
      tab.addEventListener("click", () => show(tab));
      tab.addEventListener("keydown", event => {
        const step = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
        if (!step) return;
        const next = tabs[(i + step + tabs.length) % tabs.length];
        next.focus(); show(next); event.preventDefault();
      });
    });
  }

  // A teaching sketch of the shorthand; the app's engine owns the real parser.
  const parse = text => {
    const words = text.trim().toLowerCase().split(/\s+/).filter(Boolean);
    const parts = {}; const terms = [];
    for (let i = 0; i < words.length; i++) {
      const w = words[i];
      if (i === 0 && ["see", "cf.", "cf", "e.g."].includes(w)) parts.prefix = w === "cf" ? "cf." : w;
      else if (["ch", "chap", "chapter"].includes(w) && /^\d+$/.test(words[i + 1] || "")) parts.locator = `chap. ${words[++i]}`;
      else if (/^\d+([-–]\d+)?$/.test(w)) parts.locator = /[-–]/.test(w) ? `pp. ${w.replace("-", "–")}` : `p. ${w}`;
      else terms.push(w);
    }
    parts.ref = terms.length ? refs.find(r => terms.every(t => r.words.includes(t))) : undefined;
    return parts;
  };
  const shorthand = document.querySelector("[data-shorthand]");
  if (shorthand) {
    const input = shorthand.querySelector("[data-shorthand-input]");
    const partsOut = shorthand.querySelector("[data-shorthand-parts]");
    const out = shorthand.querySelector("[data-shorthand-out]");
    const chip = (label, value) => `<span class="part"><span>${label}</span> ${value}</span>`;
    const render = () => {
      const p = parse(input.value);
      if (!p.ref) { partsOut.innerHTML = '<span class="small">No match in the sample references yet. Try an author: vale, fen or reed.</span>'; out.textContent = "…"; return; }
      partsOut.innerHTML = [p.prefix && chip("before", p.prefix), chip("reference", `${p.ref.author} ${p.ref.year}`), p.locator && chip("locator", p.locator)].filter(Boolean).join("");
      out.textContent = `[${p.prefix ? p.prefix + " " : ""}@${p.ref.key}${p.locator ? ", " + p.locator : ""}]`;
    };
    input.addEventListener("input", render); render();
  }

  const commands = {
    search: ["$ ibis search lantern\nvale2024lanterns  Vale · 2024\n  Lantern gardens and the rhythm\n  of imaginary cities\n\n1 result in Sample references", "Search any library from scripts. Add --json for a stable, documented output."],
    cite: ["$ ibis cite vale2024lanterns fen2023clouds --style apa --page 12\n(Vale, 2024, p. 12; Fen, 2023)\n\n$ ibis cite vale2024lanterns --format typst\n@vale2024lanterns", "The same citations the picker makes, in any style or key format."],
    mcp: ["agent  ▸ search_library \"lantern gardens\"\nibis  ◂ vale2024lanterns  (in your library, source healthy)\nagent  ▸ cite vale2024lanterns, page 12, APA\nibis  ◂ (Vale, 2024, p. 12)  receipt: record, style and source checked\n\nagent  ▸ cite smith2019imaginary\nibis  ◂ not in your library: nothing to cite", "Run ibis mcp and an AI agent can search and cite from your own library, and is told plainly when a reference isn’t there."],
  };
  const terminal = document.querySelector("[data-terminal]");
  if (terminal) {
    const out = terminal.querySelector("[data-terminal-out]");
    const note = terminal.querySelector("[data-terminal-note]");
    const tabs = [...terminal.querySelectorAll("[data-cmd]")];
    const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;
    let timer;
    const show = tab => {
      for (const t of tabs) { t.setAttribute("aria-selected", String(t === tab)); t.tabIndex = t === tab ? 0 : -1; }
      const [text, explain] = commands[tab.dataset.cmd];
      note.textContent = explain; clearInterval(timer);
      if (reduce) { out.textContent = text; return; }
      let i = 0; out.textContent = "";
      timer = setInterval(() => { i += 3; out.textContent = text.slice(0, i); if (i >= text.length) clearInterval(timer); }, 16);
    };
    tabs.forEach((tab, i) => {
      tab.addEventListener("click", () => show(tab));
      tab.addEventListener("keydown", event => {
        const step = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
        if (!step) return;
        const next = tabs[(i + step + tabs.length) % tabs.length];
        next.focus(); show(next); event.preventDefault();
      });
    });
    show(tabs[0]);
  }
})();
