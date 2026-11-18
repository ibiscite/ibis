import { CITATION_FORMS } from "../bindings";
import type { CitedWarning, Language, Problem } from "../bindings";
import { FluentBundle, FluentResource } from "@fluent/bundle";
import { parse } from "@fluent/syntax";
import { createContext, type ReactNode, useContext } from "react";
import { I18nProvider } from "react-aria-components";
import english from "../../../../locales/en-US.ftl?raw";
import german from "../../../../locales/de.ftl?raw";
import arabic from "../../../../locales/ar.ftl?raw";
import { PICKER_IDS, PICKER_VARIABLES } from "./picker";
import { SETTINGS_IDS } from "./settings";

/** The interface languages, by the tag Rust's `catalogue::Language` writes.
 * Never inferred from the citation language: a style keeps its own. Digits
 * stay Latin in Arabic, as in citations, keys and page numbers. */
export const LANGUAGES = {
  "en-US": { source: english, intl: "en-US", direction: "ltr" },
  de: { source: german, intl: "de", direction: "ltr" },
  ar: { source: arabic, intl: "ar-u-nu-latn", direction: "rtl" },
} as const satisfies Record<Language, unknown>;
export type InterfaceLanguage = keyof typeof LANGUAGES;
const IDS = [
  ...SETTINGS_IDS,
  ...PICKER_IDS,
  "choice-use-file", "choice-back", "choice-use-zotero", "choice-zotero-help", "choice-library-file", "choice-library-file-help", "choice-drop-file",
  "keys-title", "keys-close", "keys-action", "keys-shortcut", "shortcuts-filter", "shortcuts-empty",
  "general-small-surprises-label", "general-small-surprises-description", "delight-name", "delight-fact-touch", "delight-fact-scarlet", "delight-fact-mummies", "delight-fact-national", "delight-fact-flight", "undo-action", "undo-remove", "undo-removed", "undo-reset-window", "undo-shortcut",
  "delight-ibis", "delight-tip", "delight-library100", "delight-library1000", "delight-library10000", "delight-empty", "delight-loading", "delight-no-library", "delight-zoom", "delight-citation", "delight-anniversary", "delight-about-words", "canvas-hovered", "canvas-pressed", "canvas-focused",
  ...CITATION_FORMS.map((form) => `form-${form}` as const),
  "settings-pages", "settings-general", "settings-shortcuts", "settings-about",
  "general-login-label", "general-login-description", "general-login-place", "general-login-when",
  "general-login-unsupported", "general-login-needs-approval", "general-login-refused", "general-login-unreadable",
  "general-menu-bar-label", "general-menu-bar-description",
  "general-paste-title", "general-behavior-title", "general-paste-label", "general-paste-key", "general-paste-description", "general-paste-allowed", "general-paste-allowed-note",
  "general-paste-guide-title", "general-paste-guide-why", "general-paste-step-open", "general-paste-step-allow", "general-paste-step-return",
  "general-paste-pane", "general-paste-open", "general-paste-drag", "general-paste-drag-keyboard", "general-paste-drag-fallback",
  "general-paste-drag-failed", "general-paste-waiting", "general-paste-granted", "general-paste-unreadable", "general-paste-return-failed", "general-paste-stale",
  "general-rows", "general-density-compact", "general-density-comfortable",
  "shortcuts-open", "shortcuts-description", "shortcuts-record", "shortcuts-use-default", "shortcuts-change", "shortcuts-not-assigned",
  "shortcuts-configure-failed", "shortcuts-failed", "shortcuts-read-failed",
  "shortcuts-refusal-invalid", "shortcuts-refusal-needs-modifier", "shortcuts-refusal-app-shortcut", "shortcuts-refusal-alt-gr",
  "shortcuts-refusal-in-use", "shortcuts-refusal-unavailable", "shortcuts-refusal-not-saved",
  "shortcuts-command-format", "shortcuts-command-pin", "shortcuts-command-keys", "shortcuts-command-more",
  "shortcuts-command-source", "shortcuts-command-change-last", "shortcuts-command-settings", "shortcuts-command-quit", "shortcuts-picker-conflict",
  "shortcuts-region-search", "shortcuts-region-library", "shortcuts-region-picker", "shortcuts-region-menu", "shortcuts-region-system", "shortcuts-region-citation",
  "shortcuts-action-choose-source", "shortcuts-action-reveal-source", "shortcuts-action-source", "shortcuts-action-change-last", "shortcuts-action-open", "shortcuts-action-copy", "shortcuts-action-paste", "shortcuts-action-copy-only", "shortcuts-action-add", "shortcuts-action-move", "shortcuts-action-pick", "shortcuts-action-peek", "shortcuts-action-escape",
  "shortcuts-action-format", "shortcuts-action-more", "shortcuts-action-pin", "shortcuts-action-keys", "shortcuts-action-settings", "shortcuts-action-quit", "shortcuts-action-undo", "shortcuts-action-text-size", "shortcuts-action-citation-move", "shortcuts-action-citation-remove", "shortcuts-action-citation-reorder", "shortcuts-action-citation-open", "shortcuts-action-citation-focus",
  "shortcuts-action-menu-move", "shortcuts-action-menu-ends", "shortcuts-action-menu-choose", "shortcuts-action-menu-leave", "shortcuts-or",
  "about-automatic", "about-automatic-description", "about-check", "about-check-label", "about-checking", "about-check-failed", "about-check-newest", "about-check-busy", "about-mission", "about-updates", "about-links", "about-website", "about-source", "about-problem", "about-licences", "about-free", "about-copied", "about-copy-failed", "about-version", "about-unavailable",
] as const;
export type MessageId = typeof IDS[number];
/** Messages Rust owns: engine codes and the CLI, MCP and native app's words. */
const RUST = /^(error|reason|refusal|problem|warning|source-warning|loss|notice|form|cause|cli|mcp|app)-/;
type Variables = Readonly<Record<string, string | number>>;
type Audience = "app" | "bibliography" | "settings";
export type InterfaceMessages = Readonly<{
  language: InterfaceLanguage;
  /** The tag for `Intl` formatters, so lists and numbers read as the words do. */
  locale: string;
  text: (id: MessageId, variables?: Variables, audience?: Audience) => string;
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
export function createInterfaceMessages(source: string, pseudo = false, language: InterfaceLanguage = "en-US"): InterfaceMessages {
  const locale = LANGUAGES[language].intl;
  const invalid = () => new Error("Invalid interface messages");
  const syntax = parse(source, { withSpans: false });
  const ids = new Set<string>();
  for (const entry of syntax.body) {
    if (entry.type === "Comment" || entry.type === "GroupComment" || entry.type === "ResourceComment") continue;
    if (entry.type === "Term") continue;
    if (entry.type !== "Message" || !entry.value || ids.has(entry.id.name)) throw invalid();
    // The engine's and the command line's words are checked by their own tests.
    if (RUST.test(entry.id.name)) continue;
    if (entry.attributes.length) throw invalid();
    ids.add(entry.id.name);
  }
  if (IDS.some((id) => !ids.has(id) && !RUST.test(id)) || [...ids].some((id) => !(IDS as readonly string[]).includes(id))) throw invalid();
  const bundle = new FluentBundle(locale, {
    // English text and accessible names stay byte for byte; a translation
    // isolates each argument, so a Latin name or key keeps its place in Arabic.
    useIsolating: language !== "en-US",
    transform: pseudo ? pseudolocalize : undefined,
  });
  if (bundle.addResource(new FluentResource(source)).length) throw invalid();
  const text = (id: MessageId, variables?: Variables, audience?: Audience): string => {
    const message = bundle.getMessage(id);
    const errors: Error[] = [];
    if (!message?.value) throw new Error("Invalid interface message");
    const named = variables?.reference ? (variables.form ? message.attributes["app-named"] : undefined) ?? message.attributes.named : undefined;
    const variant = audience === "settings" ? message.attributes.settings : undefined;
    const value = bundle.formatPattern(named ?? variant ?? message.value, variables, errors);
    if (errors.length) throw new Error("Invalid interface message");
    const advice = audience && audience !== "settings" ? message.attributes[audience] : undefined;
    if (!advice) return value;
    const fix = bundle.formatPattern(advice, variables, errors);
    if (errors.length) throw new Error("Invalid interface message");
    return `${value} ${fix}`;
  };
  // Only the variables the interface passes; a message naming another fails here.
  const synthetic = Object.fromEntries(
    [...PICKER_VARIABLES, "list", "keys", "action", "reference", "command", "file", "setting", "from", "to", "system", "query", "path"].map((name) => [name, "Synthetic"]),
  );
  // Exercise all platform branches before publishing the immutable owner.
  // Errors remain fixed text and cannot disclose variables or source content.
  // Every count reaches each plural form a language has (Arabic has six).
  try {
    for (const platform of ["macos", "windows", "linux"]) {
      for (const count of platform === "macos" ? [0, 1, 2, 3, 11, 100] : [1]) {
        for (const id of IDS) text(id, { ...synthetic, platform, name: "Ibis", version: "0.0.0", percent: 100, count, id: 1 });
      }
    }
  } catch { throw invalid(); }
  return Object.freeze({ language, locale, text });
}

export const englishMessages = createInterfaceMessages(english);
let current = englishMessages;

/** Speak this language in this window: its words, `lang` and direction.
 * Each window calls it once before it draws; tests set and restore it. */
export function setInterfaceLanguage(language: InterfaceLanguage, pseudo = false): InterfaceMessages {
  current = language === "en-US" && !pseudo ? englishMessages : createInterfaceMessages(LANGUAGES[language].source, pseudo, language);
  document.documentElement.lang = language;
  document.documentElement.dir = LANGUAGES[language].direction;
  return current;
}

/** This window's tag for `Intl` formatters. */
export const interfaceLocale = () => current.locale;

/** The picker's words, outside React: one lookup in this window's catalogue. */
export const say: InterfaceMessages["text"] = (id, variables, audience) => current.text(id, variables, audience);

/** First-strong isolation for a name inside a sentence, so a right-to-left name keeps its place. */
export function isolate(name: string) {
  return `\u2068${name}\u2069`;
}

/** A reason from Rust as the end of a sentence: with its full stop, capitalised when it starts one. */
export function fullStop(reason: string, capital = false) {
  const text = capital ? reason.charAt(0).toUpperCase() + reason.slice(1) : reason;
  return /[.!?؟。]\u2069?$/.test(text) ? text : `${text}.`;
}
const InterfaceContext = createContext<InterfaceMessages | null>(null);
/** A window's words, and the same language for React Aria's own direction,
 * arrow keys and formatting. */
export function InterfaceMessagesProvider({ messages, children }: { messages: InterfaceMessages; children: ReactNode }) {
  return <InterfaceContext value={messages}><I18nProvider locale={messages.locale}>{children}</I18nProvider></InterfaceContext>;
}
/** A provider's catalogue (tests, the pseudolocale), else this window's language. */
export function useInterfaceMessages(): InterfaceMessages {
  return useContext(InterfaceContext) ?? current;
}

/** A citation's warning in the catalogue's words, whichever side declared it. */
export function warningText(warning: CitedWarning, text = say): string {
  return text(warning.message.id as MessageId, warning.message.args, "app");
}

/** Render a native IPC diagnostic through the shared catalogue. */
export function problemText(error: unknown, fallback: string, text = say, audience: "app" | "settings" = "app"): string {
  const problem = typeof error === "object" && error !== null ? error as Partial<Problem> : null;
  if (!problem?.message || typeof problem.message.id !== "string" || !problem.recovery || !problem.focus) return fallback;
  try { return text(problem.message.id as MessageId, problem.message.args, problem.message.context === "bibliography" ? "bibliography" : audience); }
  catch { return fallback; }
}
