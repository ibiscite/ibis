import { useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { ChipGroup, type ChipItem } from "./ChipGroup";

export const chipGroupGalleryStates = [
  { id: "chips", label: "Chip keyboard foundation" },
  { id: "chips-rtl", label: "Chip keyboard, RTL" },
  { id: "citation-segments", label: "Seven inline citation segments" },
  { id: "citation-segments-rtl", label: "Inline citation direction, RTL" },
];
const examples: ChipItem[] = [
  { id: "one\0shared", label: "Research", textValue: "Research, one", removeLabel: "Remove Research from one" },
  { id: "two\0shared", label: "Research", textValue: "Research, two", removeLabel: "Remove Research from two" },
  { id: "three", label: "العربية", textValue: "العربية, three", removeLabel: "Remove العربية" },
];
function Preview({ rtl, citation }: { rtl: boolean; citation: boolean }) {
  const [items, setItems] = useState(citation ? Array.from({ length: 7 }, (_, index) => ({
    id: `reference-${index}`, label: `García & Okafor, ${2021 + index}`, textValue: `García & Okafor, ${2021 + index}`,
    removeLabel: `Remove García & Okafor, ${2021 + index}`,
    content: rtl ? <><i>سالم و García &amp; Okafor</i>، {2021 + index}</> : `{synthetic_reference_${index + 1}}`,
    before: index === 0 ? rtl ? "(" : "\\cites" : rtl ? "; " : "", after: rtl && index === 6 ? ")" : "",
  })) : examples);
  const search = useRef<HTMLInputElement>(null);
  return <main style={{ padding: "var(--space-4)", gap: "var(--space-4)", height: "auto" }}>
    <input ref={search} aria-label="Synthetic search" placeholder="Synthetic component preview" style={{ width: "100%", minWidth: 0 }} />
    <ChipGroup label="Chosen labels" items={items} locale={rtl && !citation ? "ar-AE" : "en-US"} fallbackFocusRef={search}
      variant={citation ? "citation" : "chips"} dir={rtl ? "rtl" : "ltr"}
      onRemove={(keys) => setItems((previous) => previous.filter((item) => !keys.has(item.id)))} />
  </main>;
}

/** Acknowledge only after a real arrow event focuses the second visible chip. */
export function renderChipGroupGallery(root: HTMLElement, id: string) {
  const app = createRoot(root);
  const rtl = id.endsWith("rtl");
  const citation = id.startsWith("citation-segments");
  app.render(<Preview rtl={rtl} citation={citation} />);
  let frame = 0; let pressed = false;
  const check = () => {
    const rows = [...root.querySelectorAll<HTMLElement>('[role="row"]')];
    if (rows.length === (citation ? 7 : 3) && !pressed) {
      pressed = true; rows[0]!.focus();
      rows[0]!.dispatchEvent(new KeyboardEvent("keydown", { key: rtl ? "ArrowLeft" : "ArrowRight", bubbles: true }));
    }
    const visible = rows.length === (citation ? 7 : 3) && (citation ? rows.slice(0, 2) : rows).every((row) => { const box = row.getBoundingClientRect(); return box.width > 0 && box.height > 0 && box.left >= 0 && box.right <= innerWidth && box.bottom <= innerHeight; });
    const targets = citation ? rows.map((row) => {
      const pill = row.querySelector<HTMLElement>(".citation-pill")!;
      const button = pill.querySelector("button")!;
      const painted = button.getBoundingClientRect();
      const after = getComputedStyle(button, "::after");
      const hit = { left: painted.left + parseFloat(after.left), right: painted.right - parseFloat(after.right),
        top: painted.top + parseFloat(after.top), bottom: painted.bottom - parseFloat(after.bottom) };
      const bounds = pill.getBoundingClientRect();
      const minimum = parseFloat(getComputedStyle(pill).minWidth);
      // One browser layout unit tolerates floating-point subtraction only.
      const epsilon = 1 / 64;
      return { ...hit, contained: hit.left >= bounds.left - epsilon && hit.right <= bounds.right + epsilon
        && hit.top >= bounds.top - epsilon && hit.bottom <= bounds.bottom + epsilon
        && hit.right - hit.left >= minimum - epsilon && hit.bottom - hit.top >= minimum - epsilon };
    }).sort((a, b) => a.left - b.left) : [];
    const list = root.querySelector<HTMLElement>(".chip-group-list");
    const safeTargets = !citation || Boolean(list && list.scrollWidth > list.clientWidth
      && targets.every((target, index) => target.contained && (index === 0 || target.left >= targets[index - 1]!.right)));
    if (visible && safeTargets && document.activeElement === rows[1] && document.fonts.status !== "loading") document.documentElement.dataset.galleryRendered = id;
    else frame = requestAnimationFrame(check);
  };
  frame = requestAnimationFrame(check);
  return () => { cancelAnimationFrame(frame); app.unmount(); };
}
