import { useMemo, useState } from "react";
import type { SystemIcon } from "../bindings";
import { ChoiceIcon } from "../icons";
import { Button } from "./Button";
import "./SourceAction.css";

/** Native raster bytes only; the source name and action survive missing or
 * undecodable system icons. No external URL or third-party logo asset. */
export function SourceIcon({ icon, file = false }: { icon: SystemIcon | null; file?: boolean }) {
  const [failed, setFailed] = useState<SystemIcon | null>(null);
  const url = useMemo(() => {
    if (!icon || !["image/png", "image/jpeg"].includes(icon.mime) || icon.bytes.length === 0 || icon.bytes.length > 65536 ||
      !icon.bytes.every((byte) => Number.isInteger(byte) && byte >= 0 && byte <= 255)) return null;
    return `data:${icon.mime};base64,${btoa(icon.bytes.map((byte) => String.fromCharCode(byte)).join(""))}`;
  }, [icon]);
  return <span className="source-icon" aria-hidden="true">
    {url && failed !== icon ? <img src={url} alt="" draggable={false} onError={() => setFailed(icon)} /> : <ChoiceIcon name={file ? "file" : "library"} />}
  </span>;
}

export type SourceActionProps = {
  label: string; keys?: string; icon: SystemIcon | null; file?: boolean; busy?: boolean; open: () => void;
};

/** Shorten in the middle so the name's start and its extension both survive. */
export function shortenedLabel(label: string, limit = 44): string {
  // Whole graphemes: slicing code points would split emoji and combining accents.
  const characters = [...new Intl.Segmenter(undefined, { granularity: "grapheme" }).segment(label)].map(({ segment }) => segment);
  if (characters.length <= limit) return label;
  const dot = characters.lastIndexOf(".");
  const ending = dot > 0 && characters.length - dot <= 8 ? characters.length - dot : 8;
  const start = limit - ending - 1;
  return `${characters.slice(0, start).join("")}…${characters.slice(-ending).join("")}`;
}

/** The footer source/fix action shares Copy's component. */
export function SourceAction({ label, keys, icon, file, busy = false, open }: SourceActionProps) {
  const shown = shortenedLabel(label);
  return <Button label={shown} keys={keys} className="source-action library-inline-action"
    aria-busy={busy || undefined} title={shown === label ? undefined : label}
    aria-label={label} onPress={open} icon={busy ? <span className="source-spinner" aria-hidden="true" /> : <SourceIcon icon={icon} file={file} />} />;
}
