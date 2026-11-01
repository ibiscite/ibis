import type { RefObject } from "react";
import { Button } from "../../ui/Button";
import { pickerText } from "../../messages";
import type { SessionEvent } from "./session";
import type { CitationReference } from "./choice";
import type { Locator } from "../../api";
import type { usePickerActions } from "./usePickerActions";

export function PickerPageToken({ usingLocator, prepared, canApplyLocator, changeLocator, send, input }: {
  usingLocator: ReturnType<typeof usePickerActions>["usingLocator"];
  prepared: ReturnType<typeof usePickerActions>["prepared"];
  canApplyLocator: ReturnType<typeof usePickerActions>["canApplyLocator"];
  changeLocator: (reference: CitationReference, locator: Readonly<Locator> | null) => void;
  send: (event: SessionEvent) => void;
  input: RefObject<HTMLInputElement | null>;
}) {
  return usingLocator && <span className="search-page-token" role="group" aria-label={pickerText.usingLocator(usingLocator.label, usingLocator.value)}>
    <Button variant="quiet" size="small" aria-label={pickerText.searchAsTyped} title={pickerText.searchAsTyped}
      onPress={() => { send({ type: "literal" }); input.current?.focus({ preventScroll: true }); }}>
      <bdi>{usingLocator.label === "page" ? "p." : usingLocator.label} {usingLocator.value}</bdi><span aria-hidden="true">×</span>
    </Button>
    {canApplyLocator && prepared && <Button variant="quiet" size="small" onPress={() => { changeLocator(prepared, usingLocator); input.current?.focus(); }} aria-label={pickerText.applyLocator(prepared.key)} title={pickerText.applyLocator(prepared.key)}>Apply</Button>}
  </span>;
}
