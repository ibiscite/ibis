import { useMemo, useState } from "react";
import type { SystemIcon } from "../bindings";
import { ChoiceIcon } from "../icons";
import { ariaKeys } from "../keys";
import { currentPlatform } from "../platform";
import { Button } from "./Button";
import { KeyHint } from "./KeyCaps";
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
  label: string; keys?: string; icon: SystemIcon | null; file?: boolean; open: () => void;
};

/** The footer's one quiet, filled source/fix action. */
export function SourceAction({ label, keys, icon, file, open }: SourceActionProps) {
  return <Button variant="quiet" size="small" className="source-action library-inline-action" aria-label={label}
    aria-keyshortcuts={keys ? ariaKeys(keys, currentPlatform() === "macos") : undefined} onPress={open}>
    <SourceIcon icon={icon} file={file} />
    {keys ? <KeyHint label={label} keys={keys} hidden /> : <bdi>{label}</bdi>}
  </Button>;
}
