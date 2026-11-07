import { Icon } from "../../icons";
import { Button } from "../../ui/Button";
import { say } from "../../i18n";
import type { SessionEvent } from "./session";
import type { CitationReference } from "./choice";
import type { Locator } from "../../api";
import type { usePickerActions } from "./usePickerActions";

export function PickerPageToken({ usingLocator, prepared, canApplyLocator, changeLocator, send, focusHome }: {
  usingLocator: ReturnType<typeof usePickerActions>["usingLocator"];
  prepared: ReturnType<typeof usePickerActions>["prepared"];
  canApplyLocator: ReturnType<typeof usePickerActions>["canApplyLocator"];
  changeLocator: (reference: CitationReference, locator: Readonly<Locator> | null) => void;
  send: (event: SessionEvent) => void;
  focusHome: () => void;
}) {
  return usingLocator && <span className="search-page-token" role="group" aria-label={say("picker-using-locator", { label: usingLocator.label, value: usingLocator.value })}>
    <Button variant="quiet" size="small" aria-label={say("picker-search-as-typed")} title={say("picker-search-as-typed")}
      onPress={() => { send({ type: "literal" }); focusHome(); }}>
      <bdi>{usingLocator.label === "page" ? "p." : usingLocator.label} {usingLocator.value}</bdi><Icon name="close" />
    </Button>
    {canApplyLocator && prepared && <Button variant="quiet" size="small" onPress={() => { changeLocator(prepared, usingLocator); focusHome(); }} aria-label={say("picker-apply-locator", { key: prepared.key })} title={say("picker-apply-locator", { key: prepared.key })}>Apply</Button>}
  </span>;
}
