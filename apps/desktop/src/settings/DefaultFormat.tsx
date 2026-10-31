import { useRef } from "react";
import type { CitationStyle, KeyFormat, PickerPreferences } from "../bindings";
import { useInterfaceMessages } from "../i18n";
import { ToolbarMenu } from "../ui/ToolbarMenu";

/** The same Ibis popup, checks, filtering and keyboard behaviour as the picker. */
export function DefaultFormat({ preferences, change }: {
  preferences: PickerPreferences;
  change: (patch: Partial<PickerPreferences>, key?: KeyFormat | null) => void;
}) {
  const { text } = useInterfaceMessages();
  const button = useRef<HTMLButtonElement>(null);
  const sections = [
    { id: "default-key-formats", label: text("citations-key-formats"), items: (["pandoc", "autocite", "parencite", "textcite", "cite", "typst", "keys"] as const)
      .map((id) => ({ id, label: text(`citations-format-${id}`) })) },
    { id: "default-citation-styles", label: text("citations-style-formats"), items: (["apa", "chicago-author-date", "harvard-cite-them-right", "ieee", "modern-language-association"] as const)
      .map((style) => ({ id: `style:${style}`, label: text(`citations-format-${style}`) })) },
  ];
  const value = preferences.citation_style ? `style:${preferences.citation_style}` : preferences.copy_format ?? "pandoc";
  return <ToolbarMenu buttonRef={button} returnFocusRef={button} label={text("citations-default")}
    value={sections.flatMap((section) => section.items).find((item) => item.id === value)!.label}
    selectedKey={value} sections={sections} filterLabel={text("citations-filter-formats")}
    onAction={(choice) => {
      if (choice.startsWith("style:")) change({ citation_style: choice.slice(6) as CitationStyle });
      else change({ citation_style: null }, choice as KeyFormat);
    }} />;
}
