import { useEffect, useRef } from "react";
import type { PickerApi } from "../../App";
import type { SessionState } from "./session";
import type { usePickerMenus } from "./usePickerMenus";
import { monitorUpdateOfferInput } from "./updateOfferInput";

export function useUpdateOfferActivity(api: PickerApi, state: SessionState,
  { menuOpen, libraryMenuOpen, formatOpen, keysOpen }: Pick<ReturnType<typeof usePickerMenus>, "menuOpen" | "libraryMenuOpen" | "formatOpen" | "keysOpen">, choosingLibrary: boolean) {
  const { query, pendingCopy, selected } = state;
  const updateInputBlocked = useRef(false);
  updateInputBlocked.current = Boolean(query || pendingCopy || menuOpen || libraryMenuOpen || choosingLibrary || formatOpen || keysOpen || selected.size || state.activeCopy !== null);
  const updateInput = useRef<ReturnType<typeof monitorUpdateOfferInput> | null>(null);
  useEffect(() => {
    if (!api.updatePickerActivity) return;
    const monitor = monitorUpdateOfferInput(window, api.updatePickerActivity, () =>
      updateInputBlocked.current || !document.hasFocus() || document.documentElement.dataset.pickerHidden !== undefined ||
      [...document.querySelectorAll<HTMLInputElement | HTMLTextAreaElement>('input:not([type]), input[type="text"], input[type="search"], textarea')].some((input) => input.value.length > 0) ||
      [...document.querySelectorAll('[contenteditable="true"]')].some((input) => Boolean(input.textContent?.length)));
    updateInput.current = monitor;
    const visibility = new MutationObserver(monitor.activity);
    visibility.observe(document.documentElement, { attributes: true, attributeFilter: ["data-picker-hidden"] });
    return () => { visibility.disconnect(); monitor.stop(); updateInput.current = null; };
  }, [api.updatePickerActivity]);
  // biome-ignore lint/correctness/useExhaustiveDependencies: each interaction transition invalidates the old idle ticket, including close/reopen; the monitor reads current blocking state through a ref.
  useEffect(() => { updateInput.current?.activity(); }, [query, pendingCopy, menuOpen, libraryMenuOpen, choosingLibrary, formatOpen, keysOpen, selected.size, state.activeCopy, state.visibility]);
}
