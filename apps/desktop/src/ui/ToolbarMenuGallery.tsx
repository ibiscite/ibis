import { useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { ToolbarMenu, type ToolbarMenuSection } from "./ToolbarMenu";

export const toolbarMenuGalleryStates = [
  { id: "toolbar-menu", label: "Toolbar menu foundation" },
  { id: "toolbar-menu-filtered", label: "Toolbar menu filtered" },
];
const sections: ToolbarMenuSection[] = [
  { id: "text", label: "Text examples", items: [
    { id: "plain", label: "Plain text", sample: "Example" }, { id: "markup", label: "Markup", sample: "[example]" },
    { id: "code", label: "Code", sample: "{example}" }, { id: "unicode", label: "Unicode", sample: "研究" },
  ] },
  { id: "other", label: "Other examples", items: [
    { id: "prose", label: "Prose", sample: "(Example, 2021)", prose: true }, { id: "compact", label: "Compact", sample: "[1]" },
    { id: "long", label: "Longer label", sample: "Example" }, { id: "unavailable", label: "Unavailable", disabled: true },
  ] },
];
function Preview() {
  const [key, setKey] = useState("plain");
  const search = useRef<HTMLInputElement>(null);
  return <main style={{ padding: 16, height: "auto", display: "flex", flexDirection: "row", alignItems: "center", justifyContent: "space-between" }}>
    <input ref={search} aria-label="Synthetic search" placeholder="Synthetic component preview" />
    <ToolbarMenu label="Output" value={sections.flatMap((section) => section.items).find((item) => item.id === key)?.label ?? ""}
      sections={sections} selectedKey={key} onAction={setKey} filterLabel="Filter examples" returnFocusRef={search} />
  </main>;
}

/** Exercise actual clicks/input; acknowledge only the visible requested state. */
export function renderToolbarMenuGallery(root: HTMLElement, id: string) {
  const app = createRoot(root);
  app.render(<Preview />);
  let frame = 0;
  let clicked = false;
  const check = () => {
    const trigger = root.querySelector<HTMLButtonElement>(".toolbar-menu-button");
    if (trigger && !clicked) { clicked = true; trigger.click(); }
    const filter = document.querySelector<HTMLInputElement>(".toolbar-menu-filter input");
    if (filter && id.endsWith("filtered") && filter.value !== "Prose") {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(filter, "Prose");
      filter.dispatchEvent(new Event("input", { bubbles: true }));
    }
    const items = document.querySelectorAll('[role="menuitemradio"]');
    const visible = [...items].every((item) => { const rect = item.getBoundingClientRect(); return rect.width > 0 && rect.height > 0 && rect.top >= 0 && rect.bottom <= innerHeight; });
    if (visible && !document.querySelector("[data-entering]") && document.fonts.status !== "loading" && items.length === (id.endsWith("filtered") ? 1 : 8)) document.documentElement.dataset.galleryRendered = id;
    else frame = requestAnimationFrame(check);
  };
  frame = requestAnimationFrame(check);
  return () => { cancelAnimationFrame(frame); app.unmount(); };
}
