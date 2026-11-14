import { art } from "../icon-art";

/** The browser's serialization of the full upstream glyph, including all shapes. */
export function iconBody(name: keyof typeof art.body) {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.innerHTML = art.body[name];
  return svg.innerHTML;
}
