import type { ReactNode, Ref, RefObject } from "react";
import { ariaKeys, keyLabel } from "../../keys";
import { say } from "../../i18n";
import { currentPlatform } from "../../platform";
import { KeyHint } from "../../ui/KeyCaps";
import { Menu } from "../../ui/Menu";

export { MenuItem, MenuSection, MenuSeparator } from "../../ui/Menu";

/**
 * The More button and its menu, opened upward from the footer. Ctrl/⌘ K sets
 * `isOpen` from the picker; Tab leaves the menu for the search box.
 */
export function MoreMenu({ isOpen, onOpenChange, buttonRef, searchRef, keys, children }: {
  isOpen: boolean; onOpenChange: (open: boolean) => void;
  buttonRef: Ref<HTMLButtonElement>; searchRef: RefObject<HTMLElement | null>;
  /** The shortcut in accelerator syntax. */
  keys: string;
  children: ReactNode;
}) {
  const platform = currentPlatform();
  const mac = platform === "macos";
  return <Menu id="more-menu" label={say("picker-more")} isOpen={isOpen} onOpenChange={onOpenChange} placement="top end"
    triggerRef={buttonRef} triggerVariant="quiet" triggerSize="small" triggerLabel={say("picker-more")} triggerTitle={say("picker-more-title", { keys: keyLabel(keys, mac) })} triggerKeys={ariaKeys(keys, mac)}
    tabFocusRef={searchRef}
    trigger={<KeyHint label={say("picker-more")} keys={keys} platform={platform} hidden />}>
    {children}
  </Menu>;
}
