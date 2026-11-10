import { Icon } from "../../icons";
import { Button } from "../../ui/Button";
import { say } from "../../i18n";
import type { SessionEvent } from "./session";
import type { CitationReference } from "./choice";
import type { Typed } from "./choice";
import type { usePickerActions } from "./usePickerActions";

export function PickerPageToken({ usingLocator, prepared, canApplyLocator, changeLocator, send, focusHome }: {
  usingLocator: ReturnType<typeof usePickerActions>["usingLocator"];
  prepared: ReturnType<typeof usePickerActions>["prepared"];
  canApplyLocator: ReturnType<typeof usePickerActions>["canApplyLocator"];
  changeLocator: (reference: CitationReference, typed: NonNullable<Typed>) => void;
  send: (event: SessionEvent) => void;
  focusHome: () => void;
}) {
  if (!usingLocator) return null;
  const { locator, prefix } = usingLocator;
  // What was read, as it will be cited: "see · p. 417".
  const read = [prefix, locator && `${locator.label === "page" ? "p." : locator.label} ${locator.value}`].filter(Boolean).join(" · ");
  return <span className="search-page-token" role="group" aria-label={say("picker-using-shorthand", { value: read })}>
    <Button variant="quiet" size="small" aria-label={say("picker-search-as-typed")} title={say("picker-search-as-typed")}
      onPress={() => { send({ type: "literal" }); focusHome(); }}>
      <bdi>{read}</bdi><Icon name="close" />
    </Button>
    {canApplyLocator && prepared && <Button variant="quiet" size="small" onPress={() => { changeLocator(prepared, usingLocator); focusHome(); }} aria-label={say("picker-apply-locator", { key: prepared.key })} title={say("picker-apply-locator", { key: prepared.key })}>Apply</Button>}
  </span>;
}
