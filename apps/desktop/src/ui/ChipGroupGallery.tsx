import { useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { ChipGroup, type ChipItem } from "./ChipGroup";

export const chipGroupGalleryStates = [
  { id: "chips", label: "Chip keyboard foundation" },
  { id: "chips-rtl", label: "Chip keyboard, RTL" },
];
const examples: ChipItem[] = [
  { id: "one\0shared", label: "Research", textValue: "Research, one", removeLabel: "Remove Research from one" },
  { id: "two\0shared", label: "Research", textValue: "Research, two", removeLabel: "Remove Research from two" },
  { id: "three", label: "العربية", textValue: "العربية, three", removeLabel: "Remove العربية" },
];
function Preview({ rtl }: { rtl: boolean }) {
  const [items, setItems] = useState(examples);
  const search = useRef<HTMLInputElement>(null);
  return <main style={{ padding: "var(--space-4)", gap: "var(--space-4)", height: "auto" }}>
    <input ref={search} aria-label="Synthetic search" placeholder="Synthetic component preview" style={{ width: "100%", minWidth: 0 }} />
    <ChipGroup label="Chosen labels" items={items} locale={rtl ? "ar-AE" : "en-US"} fallbackFocusRef={search}
      onRemove={(keys) => setItems((previous) => previous.filter((item) => !keys.has(item.id)))} />
  </main>;
}

/** Acknowledge only after a real arrow event focuses the second visible chip. */
export function renderChipGroupGallery(root: HTMLElement, id: string) {
  const app = createRoot(root);
  const rtl = id.endsWith("rtl");
  app.render(<Preview rtl={rtl} />);
  let frame = 0; let pressed = false;
  const check = () => {
    const rows = [...root.querySelectorAll<HTMLElement>('[role="row"]')];
    if (rows.length === 3 && !pressed) {
      pressed = true; rows[0]!.focus();
      rows[0]!.dispatchEvent(new KeyboardEvent("keydown", { key: rtl ? "ArrowLeft" : "ArrowRight", bubbles: true }));
    }
    const visible = rows.length === 3 && rows.every((row) => { const box = row.getBoundingClientRect(); return box.width > 0 && box.height > 0 && box.left >= 0 && box.right <= innerWidth && box.bottom <= innerHeight; });
    if (visible && document.activeElement === rows[1] && document.fonts.status !== "loading") document.documentElement.dataset.galleryRendered = id;
    else frame = requestAnimationFrame(check);
  };
  frame = requestAnimationFrame(check);
  return () => { cancelAnimationFrame(frame); app.unmount(); };
}
