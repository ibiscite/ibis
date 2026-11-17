// The playground page: pick a scenario and how to see it; each board is its own
// document, as the app's windows are. Everything chosen lives in the URL.
import { scenarios } from "../scenarios";
import { loadComments, markComment } from "./comments";

const variantNames = Object.keys(import.meta.glob("../variants/*.css")).map((file) => file.slice("../variants/".length, -".css".length));
const choices = {
  theme: [["light", "Light"], ["dark", "Dark"], ["both", "Both"]],
  platform: [["macos", "macOS"], ["windows", "Windows"], ["linux", "Linux"]],
  text: [["80", "80 %"], ["100", "100 %"], ["120", "120 %"]],
} as const;
type Choice = keyof typeof choices;

const query = new URLSearchParams(location.search);
const state = {
  scenario: query.get("scenario") ?? "results",
  theme: query.get("theme") ?? "light", platform: query.get("platform") ?? "macos", text: query.get("text") ?? "100",
  // "base" is the app as built; each other name is a file in canvas/variants/.
  variants: (query.get("variants") ?? "base").split(",").filter((name) => name === "base" || variantNames.includes(name)),
};
let commenting = false;

const element = <K extends keyof HTMLElementTagNameMap>(tag: K, props: Record<string, unknown> = {}, ...children: (Node | string)[]): HTMLElementTagNameMap[K] => {
  const node = Object.assign(document.createElement(tag), props);
  node.append(...children);
  return node;
};
const byId = (id: string) => document.getElementById(id)!;

function render() {
  const url = new URLSearchParams({ scenario: state.scenario, theme: state.theme, platform: state.platform, text: state.text, variants: state.variants.join(",") });
  history.replaceState(null, "", `?${url}`);
  const themes = state.theme === "both" ? ["light", "dark"] : [state.theme];
  byId("boards").replaceChildren(...state.variants.flatMap((variant) => themes.map((theme) => {
    const src = `board.html?${new URLSearchParams({ scenario: state.scenario, theme, platform: state.platform, text: state.text, variant })}`;
    return element("figure", {}, element("figcaption", { textContent: `${state.scenario} · ${theme} · ${variant === "base" ? "as built" : variant}` }),
      element("div", { className: "frame" }, element("iframe", { src, title: `${state.scenario}, ${theme}, ${variant}` })));
  })));
  for (const button of byId("scenarios").querySelectorAll("button")) button.setAttribute("aria-current", String(button.textContent === state.scenario));
}

function controls() {
  byId("controls").replaceChildren(...(Object.keys(choices) as Choice[]).flatMap((name) => {
    const select = element("select", { id: name, onchange: () => { state[name] = select.value; render(); } },
      ...choices[name].map(([value, label]) => element("option", { value, textContent: label, selected: value === state[name] })));
    return [element("label", { htmlFor: name, textContent: name === "text" ? "Text size" : name[0]!.toUpperCase() + name.slice(1) }), select];
  }));
  byId("variants").replaceChildren(...["base", ...variantNames].map((name) => {
    const box = element("input", { type: "checkbox", checked: state.variants.includes(name), onchange: () => {
      state.variants = ["base", ...variantNames].filter((other) => other === name ? box.checked : state.variants.includes(other));
      render();
    } });
    return element("label", {}, box, name === "base" ? "As built" : name);
  }));
  const list = scenarios.map((scenario) => element("li", {}, element("button", { type: "button", textContent: scenario, onclick: () => { state.scenario = scenario; render(); } })));
  byId("scenarios").replaceChildren(...list);
  const filter = byId("filter") as HTMLInputElement;
  filter.oninput = () => { for (const item of list) item.hidden = !item.textContent!.includes(filter.value.trim()); };
}

async function showComments() {
  const open = (await loadComments()).filter((comment) => !comment.done);
  byId("comments").replaceChildren(...open.length ? open.map((comment) => element("li", {},
    element("input", { type: "checkbox", title: "Mark done", onchange: () => void markComment(comment.id, true) }),
    element("button", { type: "button", textContent: `${comment.id}. ${comment.scenario}: ${comment.comment}`, onclick: () => {
      Object.assign(state, { scenario: comment.scenario, theme: comment.theme, platform: comment.platform, text: String(comment.text) });
      if (!state.variants.includes(comment.variant)) state.variants.push(comment.variant);
      controls();
      render();
    } }))) : [element("li", { className: "hint", textContent: "None yet." })]);
}

const tell = (target: Window | null) => target?.postMessage({ playground: "commenting", on: commenting }, location.origin);
byId("comment").onclick = () => {
  commenting = !commenting;
  byId("comment").setAttribute("aria-pressed", String(commenting));
  for (const frame of document.querySelectorAll("iframe")) tell(frame.contentWindow);
};
// A board that loads or reloads (an edit, a new choice) asks which mode it is in.
addEventListener("message", (event) => {
  if (event.origin === location.origin && event.data?.playground === "ready") tell(event.source as Window);
});
import.meta.hot?.on("playground:comments", () => void showComments());

controls();
render();
void showComments();
