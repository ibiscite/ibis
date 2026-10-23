import { useState } from "react";
import { createRoot } from "react-dom/client";
import { Button, ToggleButton } from "./Button";

export const buttonGalleryStates = [{ id: "buttons", label: "Shared buttons: rest, disabled and selected" }];

function Preview() {
  const [selected, setSelected] = useState(true);
  return <main style={{ height: "auto", padding: 16, gap: 16 }}>
    <h2>Shared buttons</h2>
    {(["primary", "secondary", "quiet", "icon"] as const).map((variant) => <section key={variant} style={{ display: "flex", gap: 16, alignItems: "center" }}>
      <span>{variant}</span>
      <Button variant={variant} aria-label={`${variant} action`}>{variant === "icon" ? "×" : "Action"}</Button>
      <Button variant={variant} isDisabled aria-label={`${variant} unavailable`}>{variant === "icon" ? "×" : "Unavailable"}</Button>
      <ToggleButton variant={variant} isSelected={selected} onChange={setSelected} aria-label={`${variant} toggle`}>{variant === "icon" ? "×" : "Toggle"}</ToggleButton>
    </section>)}
    <section style={{ display: "flex", gap: 16, alignItems: "center" }}>
      <Button size="small" variant="quiet" aria-label="Small action">Small action</Button>
      <ToggleButton size="small" variant="icon" aria-label="Small toggle">×</ToggleButton>
    </section>
  </main>;
}

export function renderButtonGallery(root: HTMLElement) {
  createRoot(root).render(<Preview />);
  const ready = () => {
    const small = [...root.querySelectorAll<HTMLButtonElement>('button[data-size="small"]')];
    const target = matchMedia("(pointer: coarse)").matches ? 44 : 32;
    const hittable = small.every((button) => {
      const box = button.getBoundingClientRect();
      const x = box.x + box.width / 2, y = box.y + box.height / 2;
      return [-1, 1].every((direction) => document.elementFromPoint(x, y + direction * (target / 2 - 0.5)) === button
        && document.elementFromPoint(x + direction * (target / 2 - 0.5), y) === button);
    });
    if (root.querySelectorAll("button").length === 14 && hittable && document.fonts.status !== "loading") document.documentElement.dataset.galleryRendered = "buttons";
    else requestAnimationFrame(ready);
  };
  requestAnimationFrame(ready);
}
