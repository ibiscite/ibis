import { Icon } from "../../icons";
import { Button } from "../../ui/Button";
import { pickerText } from "../../messages";
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
  return usingLocator && <span className="search-page-token" role="group" aria-label={pickerText.usingLocator(usingLocator.label, usingLocator.value)}>
    <Button variant="quiet" size="small" aria-label={pickerText.searchAsTyped} title={pickerText.searchAsTyped}
      onPress={() => { send({ type: "literal" }); focusHome(); }}>
      <bdi>{usingLocator.label === "page" ? "p." : usingLocator.label} {usingLocator.value}</bdi><Icon name="close" />
    </Button>
    {canApplyLocator && prepared && <Button variant="quiet" size="small" onPress={() => { changeLocator(prepared, usingLocator); focusHome(); }} aria-label={pickerText.applyLocator(prepared.key)} title={pickerText.applyLocator(prepared.key)}>Apply</Button>}
  </span>;
}
