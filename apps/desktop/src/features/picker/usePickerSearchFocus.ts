import { useEffect, useState, type RefObject } from "react";

export function usePickerSearchFocus(input: RefObject<HTMLInputElement | null>) {
  // Enter copies only from the search box; elsewhere it presses the focused control.
  const [searchFocused, setSearchFocused] = useState(false);
  // autoFocus can happen before React listens for focus events.
  // biome-ignore lint/correctness/useExhaustiveDependencies: initial autofocus is checked once; Search's ref is stable.
  useEffect(() => setSearchFocused(document.activeElement === input.current), []);
  return { searchFocused, setSearchFocused };
}
