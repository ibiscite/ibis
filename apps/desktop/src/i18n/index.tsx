import { FluentBundle, FluentResource } from "@fluent/bundle";
import { parse } from "@fluent/syntax";
import { createContext, type ReactNode, useContext } from "react";
import english from "./en-US.ftl?raw";
import { SETTINGS_IDS } from "./settings";

/** Interface default only: never inferred from citation settings or navigator. */
export const INTERFACE_LOCALE = "en-US";
const IDS = [
  ...SETTINGS_IDS,
  "general-small-surprises-label", "general-small-surprises-description", "delight-name", "delight-fact-touch", "delight-fact-scarlet", "delight-fact-mummies", "delight-fact-national", "delight-fact-flight", "delight-birthday", "undo-action", "undo-remove", "undo-removed", "undo-reset-window", "undo-shortcut",
  "delight-ibis", "delight-thoth", "delight-tip", "delight-library100", "delight-library1000", "delight-library10000", "delight-empty", "delight-loading", "delight-no-library", "delight-zoom", "delight-citation", "delight-anniversary", "delight-about-words", "canvas-hovered", "canvas-pressed", "canvas-focused",
  "settings-pages", "settings-general", "settings-shortcuts", "settings-about",
  "general-login-label", "general-login-description", "general-login-place", "general-login-when",
  "general-login-unsupported", "general-login-needs-approval", "general-login-refused", "general-login-unreadable",
  "general-menu-bar-label", "general-menu-bar-description",
  "general-paste-title", "general-paste-label", "general-paste-description", "general-paste-allowed", "general-paste-allowed-note",
  "general-paste-guide-title", "general-paste-guide-why", "general-paste-step-open", "general-paste-step-allow", "general-paste-step-return",
  "general-paste-pane", "general-paste-open", "general-paste-drag", "general-paste-drag-keyboard", "general-paste-drag-fallback",
  "general-paste-drag-failed", "general-paste-waiting", "general-paste-granted", "general-paste-return-failed", "general-paste-stale",
  "general-rows", "general-density-compact", "general-density-comfortable", "general-density-spacious",
  "general-preview", "general-preview-description",
  "shortcuts-open", "shortcuts-description", "shortcuts-record", "shortcuts-use-default", "shortcuts-change", "shortcuts-not-assigned",
  "shortcuts-configure-failed", "shortcuts-failed", "shortcuts-read-failed",
  "shortcuts-refusal-invalid", "shortcuts-refusal-needs-modifier", "shortcuts-refusal-app-shortcut", "shortcuts-refusal-alt-gr",
  "shortcuts-refusal-in-use", "shortcuts-refusal-unavailable", "shortcuts-refusal-not-saved",
  "shortcuts-command-format", "shortcuts-command-pin", "shortcuts-command-keys", "shortcuts-command-more",
  "shortcuts-command-source", "shortcuts-command-preview", "shortcuts-command-settings", "shortcuts-command-quit", "shortcuts-picker-conflict",
  "shortcuts-region-search", "shortcuts-region-picker", "shortcuts-region-menu", "shortcuts-region-system",
  "shortcuts-action-choose-source", "shortcuts-action-reveal-source", "shortcuts-action-source", "shortcuts-action-preview", "shortcuts-action-copy", "shortcuts-action-copy-only", "shortcuts-action-add", "shortcuts-action-move", "shortcuts-action-escape",
  "shortcuts-action-format", "shortcuts-action-more", "shortcuts-action-pin", "shortcuts-action-keys", "shortcuts-action-settings", "shortcuts-action-quit",
  "shortcuts-action-menu-move", "shortcuts-action-menu-ends", "shortcuts-action-menu-choose", "shortcuts-action-menu-leave", "shortcuts-or",
  "about-automatic", "about-automatic-description", "about-check", "about-copy", "about-copied", "about-copy-failed", "about-version", "about-unavailable",
] as const;
export type MessageId = typeof IDS[number];
type Variables = Readonly<Record<string, string | number>>;
export type InterfaceMessages = Readonly<{
  locale: typeof INTERFACE_LOCALE;
  text: (id: MessageId, variables?: Variables) => string;
}>;

/** Visible expansion for build/test use; variables stay untouched. */
function pseudolocalize(value: string): string {
  const vowels: Record<string, string> = { a: "áá", e: "ëë", i: "ïï", o: "öö", u: "üü", A: "ÁÁ", E: "ËË", I: "ÏÏ", O: "ÖÖ", U: "ÜÜ" };
  return `⟦${value.replace(/[aeiouAEIOU]/g, (letter) => vowels[letter] ?? letter)}⟧`;
}

/** Synchronous, bundled and validated before use. No async resource owner,
 * fallback strings, network, persisted preference or engine-code interpretation.
 * The bundle remains private; consumers cannot mutate its locale or resources.
 */
export function createInterfaceMessages(source: string, pseudo = false): InterfaceMessages {
  const invalid = () => new Error("Invalid interface messages");
  const syntax = parse(source, { withSpans: false });
  const ids = new Set<string>();
  for (const entry of syntax.body) {
    if (entry.type === "Comment" || entry.type === "GroupComment" || entry.type === "ResourceComment") continue;
    if (entry.type !== "Message" || !entry.value || entry.attributes.length || ids.has(entry.id.name)) throw invalid();
    ids.add(entry.id.name);
  }
  if (ids.size !== IDS.length || IDS.some((id) => !ids.has(id))) throw invalid();
  const bundle = new FluentBundle(INTERFACE_LOCALE, {
    // Preserve existing English text and accessible names byte for byte.
    // Direction/isolation policy is separate from this interface default.
    useIsolating: false,
    transform: pseudo ? pseudolocalize : undefined,
  });
  if (bundle.addResource(new FluentResource(source)).length) throw invalid();
  const text = (id: MessageId, variables?: Variables): string => {
    const message = bundle.getMessage(id);
    const errors: Error[] = [];
    if (!message?.value) throw new Error("Invalid interface message");
    const value = bundle.formatPattern(message.value, variables, errors);
    if (errors.length) throw new Error("Invalid interface message");
    return value;
  };
  // Exercise all platform branches before publishing the immutable owner.
  // Errors remain fixed text and cannot disclose variables or source content.
  try {
    for (const platform of ["macos", "windows", "linux"]) {
      for (const id of IDS) text(id, { platform, name: "Ibis", list: "Synthetic List", version: "0.0.0", percent: 100, count: 1, library: "Invented Library", action: "Synthetic Action", reference: "Synthetic Reference", command: "Synthetic Command" });
    }
  } catch { throw invalid(); }
  return Object.freeze({ locale: INTERFACE_LOCALE, text });
}

export const englishMessages = createInterfaceMessages(english);
const InterfaceContext = createContext<InterfaceMessages>(englishMessages);
export function InterfaceMessagesProvider({ messages, children }: { messages: InterfaceMessages; children: ReactNode }) {
  return <InterfaceContext value={messages}>{children}</InterfaceContext>;
}
export function useInterfaceMessages(): InterfaceMessages {
  return useContext(InterfaceContext);
}
