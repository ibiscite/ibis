import { Fragment, useRef, type ReactNode } from "react";
import type { PickerPreferences, WindowSize } from "../bindings";
import { useInterfaceMessages } from "../i18n";
import { currentPlatform } from "../platform";
import { Group, Note, Row, Section } from "../ui/Layout";
import { PictureChoice } from "../ui/PictureChoice";
import { Segmented } from "../ui/Segmented";
import { Switch } from "../ui/Switch";
import type { useSettingUndo } from "../ui/useSettingUndo";
import { defaultOutput, defaultStyle, keyOutputs, listedOutput, outputName, styleOutputs } from "../format/outputs";
import { ToolbarMenu } from "../ui/ToolbarMenu";
import { PickerPicture } from "./PickerPicture";
import { PREFERENCES, rowDetails, type SettingRow, type NativeSetting } from "./rows";

export type SettingState = {
  value: string | boolean | null; disabled?: boolean; description?: string;
  change: (value: string | boolean, restoring?: boolean) => Promise<unknown>;
  restore?: { apply: () => Promise<void>; undo: () => Promise<void> };
};
type Undo = ReturnType<typeof useSettingUndo>["reset"];
type Feedback = { highlighted?: boolean; problem?: string };

/** One row owns its label, control and reset; callers supply only saved state. */
export function Setting({ row, state, preferences, feedback, reset, size = null }: {
  row: SettingRow; state: SettingState; preferences?: PickerPreferences | null; feedback?: Feedback; reset: Undo; size?: WindowSize | null;
}) {
  const { text } = useInterfaceMessages();
  const details = rowDetails(row.id, text);
  const label = details.label, platform = currentPlatform();
  const value = state.value ?? row.default ?? null;
  const change = (next: string | boolean) => { void state.change(next).catch(() => undefined); };
  const options = "options" in row ? row.options.filter((option) => !option.currentOnly || option.value === value) : [];
  const labels = options.map((option) => ({ value: String(option.value), label: text(option.label, { platform }), isDisabled: state.disabled }));
  const choose = (next: string) => { const option = options.find((option) => String(option.value) === next); if (option) change(option.value); };
  return <>{row.control === "appearance" && <div className="ui-picture-preview" aria-hidden="true"><PickerPicture density={preferences?.density}
    appearance={value === "system" ? "light" : String(value)} split={value === "system"} frame={WINDOWS[size ?? "medium"]} /></div>}
  <Row id={row.id} {...details} description={state.description ?? details.description} {...feedback}
    reset={!state.disabled && state.value !== null && row.default !== undefined && value !== row.default ? { label, onReset: () => {
      const restore = state.restore ?? { apply: async () => { await state.change(row.default!); }, undo: async () => { await state.change(value!, true); } };
      void reset(label, restore.apply, restore.undo);
    } } : undefined}>
    {row.control === "switch" ? <Switch aria-labelledby={row.id} aria-describedby={details.description ? `${row.id}-description` : undefined}
      isSelected={Boolean(value)} isDisabled={state.disabled} onChange={change} />
    : row.control === "appearance" ? <PictureChoice size="small" label={label} value={String(value)}
      options={labels.map((option) => ({ ...option, picture: <PickerPicture density={preferences?.density}
        appearance={option.value === "system" ? "light" : option.value} split={option.value === "system"} /> }))} onChange={choose} />
    : row.control === "documents" || row.control === "keys" ? <FormatControl label={label} value={String(value)} keys={row.control === "keys"} onChange={change} />
    : <><Segmented label={label} value={String(value)} options={labels} onChange={choose} />
      {row.note && options.some((option) => option.currentOnly && option.value === value) && <Note>{text(row.note)}</Note>}</>}
  </Row></>;
}

const WINDOWS = { small: [640, 400], medium: [720, 480], large: [840, 600], custom: [720, 480] } as const;
/** Render and keyboard order are the declaration's order, including groups. */
export function SettingRows({ rows, preferences, change, owners, feedback, reset, after = {}, size }: {
  rows: readonly SettingRow[]; preferences: PickerPreferences | null;
  change: (patch: Partial<PickerPreferences>, restoring?: boolean) => Promise<void>;
  owners: Record<NativeSetting, SettingState | { content: ReactNode }>; feedback: (id: string) => Feedback; reset: Undo;
  after?: Record<string, ReactNode>; size: WindowSize | null;
}) {
  const { text } = useInterfaceMessages();
  const groups = [...new Set(rows.map((row) => row.group))];
  return <>{groups.map((group) => <Section key={group ?? "choices"} id={rows.find((row) => row.group === group)?.section} title={group && text(group)}><Group>
    {rows.filter((row) => row.group === group).map((row) => {
      const preference = row.preference;
      const state = preference ? { value: preferences?.[preference] ?? row.default ?? null, disabled: !preferences,
        change: (value: string | boolean, restoring?: boolean) => change({ [preference]: value }, restoring) } : owners[row.native!];
      return <Fragment key={row.id}>{"content" in state ? state.content : <Setting row={row} state={state} preferences={preferences} feedback={feedback(row.id)} reset={reset} size={size} />}{after[row.id]}</Fragment>;
    })}
  </Group></Section>)}</>;
}

/** Future fields remain visible without giving this build a way to overwrite them. */
export function UnknownSettings({ preferences, saved }: { preferences: PickerPreferences | null; saved?: Record<string, unknown> | null }) {
  const { text } = useInterfaceMessages();
  const unknown = Object.entries({ ...preferences, ...saved }).filter(([key]) => !Object.hasOwn(PREFERENCES, key));
  return unknown.length > 0 && <Section title={text("settings-newer-title")}><Group>{unknown.map(([key, value]) => <Row key={key} label={key} description={text("settings-newer-description")}>
    <output aria-label={key}>{typeof value === "string" ? value : JSON.stringify(value)}</output>
  </Row>)}</Group></Section>;
}

function FormatControl({ label, value, keys, onChange }: { label: string; value: string; keys: boolean; onChange: (value: string) => void }) {
  const { text } = useInterfaceMessages();
  const button = useRef<HTMLButtonElement>(null);
  const selected = listedOutput(value) ?? (keys ? defaultOutput : defaultStyle);
  return <ToolbarMenu buttonRef={button} returnFocusRef={button} label={label} value={outputName(selected)} selectedKey={selected}
    sections={[{ id: "defaults", label: text(keys ? "citations-key-formats" : "citations-style-formats"),
      items: (keys ? keyOutputs : styleOutputs).map((output) => ({ id: output.id, label: output.name, detail: output.description ?? undefined })) }]}
    filterLabel={text("citations-filter-formats")} onAction={(choice) => { const output = listedOutput(choice); if (output) onChange(output); }} />;
}
