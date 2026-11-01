import { IconButton } from "./IconButton";
import "./LocatorToken.css";

/** A bounded, removable detail inside a reference chip, retaining its full name. */
export function LocatorToken({ text, removeLabel, onRemove }: { text: string; removeLabel: string; onRemove: () => void }) {
  return <span className="locator-token" title={text}>
    <bdi>{text}</bdi>
    <IconButton className="locator-token-remove" aria-label={removeLabel} onPress={onRemove} />
  </span>;
}
