import { useId } from "react";
import { mark, pickerMark, welcomeMark } from "../brand";
import "./IbisMark.css";

/** Generated brand artwork, with private SVG ids for each mounted copy. */
export function IbisMark({ className = "", size = "opening", motion }: { className?: string; size?: "welcome" | "opening" | "search"; motion?: "blink" | "peck" }) {
  const id = `ibis-${useId().replaceAll(":", "")}`;
  const artwork = size === "search" ? pickerMark : size === "welcome" ? welcomeMark : mark;
  return <svg className={`ibis-mark ${className}`} data-motion={motion} aria-hidden="true" focusable="false" viewBox={artwork.viewBox}
    dangerouslySetInnerHTML={{ __html: artwork.body.replaceAll("ibis-mark-", `${id}-`) }} />;
}
