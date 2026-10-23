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
  </main>;
}

export function renderButtonGallery(root: HTMLElement) {
  createRoot(root).render(<Preview />);
  const ready = () => {
    if (root.querySelectorAll("button").length === 12 && document.fonts.status !== "loading") document.documentElement.dataset.galleryRendered = "buttons";
    else requestAnimationFrame(ready);
  };
  requestAnimationFrame(ready);
}
