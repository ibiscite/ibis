import { useEffect, useRef, useState } from "react";
import type { BackupProblem, BackupSystem, RestoreChange, RestoreNote, RestorePreview } from "../bindings";
import { type MessageId, useInterfaceMessages } from "../i18n";
import { Button } from "../ui/Button";
import { SettingsGroup as Group, SettingsRow as Row } from "../ui/SettingsGroup";

export type BackupApi = {
  backUpSettings: () => Promise<string | null>;
  chooseSettingsBackup: () => Promise<RestorePreview | null>;
  restoreSettingsBackup: (ticket: number) => Promise<null>;
  cancelSettingsRestore: (ticket: number) => Promise<null>;
};

const PROBLEMS: Record<BackupProblem, MessageId> = {
  "not-downloaded": "backup-problem-not-downloaded", unreadable: "backup-problem-unreadable",
  "not-a-backup": "backup-problem-not-a-backup", "settings-file": "backup-problem-settings-file",
  newer: "backup-problem-newer", invalid: "backup-problem-invalid", "settings-unusable": "backup-problem-settings-unusable",
  changed: "backup-problem-changed", "not-saved": "backup-problem-not-saved",
};
const SYSTEMS: Record<BackupSystem, string> = { macos: "macOS", windows: "Windows", linux: "Linux" };
const SETTINGS: Record<string, MessageId> = {
  library: "backup-setting-library", copy_format: "backup-setting-copy-format", citation_style: "backup-setting-citation-style",
  density: "general-rows", larger_text: "backup-setting-larger-text",
  paste: "general-paste-label", menu_bar: "general-menu-bar-label", recent_citations: "citations-recent",
  small_surprises: "general-small-surprises-label", appearance: "look-appearance", accent: "look-accent",
};
const NOTES: Record<RestoreNote, MessageId> = { "library-not-found": "backup-note-library", "shortcut-kept": "backup-note-shortcut", "repairs-settings": "backup-note-repairs" };
const VALUES: Record<string, Record<string, MessageId>> = {
  density: { compact: "general-density-compact", comfortable: "general-density-comfortable" },
  appearance: { system: "look-system", light: "look-light", dark: "look-dark" },
  accent: { ibis: "look-accent-ibis" },
};
const FORMATS = new Set(["pandoc", "autocite", "parencite", "textcite", "cite", "typst", "keys", "apa", "chicago-author-date", "harvard-cite-them-right", "ieee", "modern-language-association"]);

/** Something to write: a changed setting, or a damaged settings file to replace. */
const restorable = (preview: RestorePreview) => preview.changes.length > 0 || preview.notes.includes("repairs-settings");

function problemOf(error: unknown): MessageId {
  return typeof error === "string" && Object.hasOwn(PROBLEMS, error) ? PROBLEMS[error as BackupProblem] : "backup-problem-not-saved";
}

/** Settings › General › Another computer: a backup file out, a checked restore in (#1561).
 * A restore shows every change first; only the latest preview can be applied, and Ibis
 * restarts so every part of it starts from the restored settings. */
export function BackupRows({ api }: { api: BackupApi }) {
  const { text } = useInterfaceMessages();
  const [busy, setBusy] = useState<"backup" | "restore" | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [problem, setProblem] = useState<{ row: "backup" | "restore"; id: MessageId } | null>(null);
  const [preview, setPreview] = useState<RestorePreview | null>(null);
  const [restarting, setRestarting] = useState(false);
  const operation = useRef(0);
  const shown = useRef<number | null>(null);
  // Leaving Settings forgets a restore that was shown and not chosen.
  useEffect(() => () => {
    operation.current += 1;
    if (shown.current !== null) void api.cancelSettingsRestore(shown.current).catch(() => undefined);
  }, [api]);

  const label = (setting: string) => {
    const system = setting.startsWith("shortcut.") ? setting.slice("shortcut.".length) : null;
    if (system !== null) return text("backup-setting-shortcut", { system: SYSTEMS[system as BackupSystem] ?? system });
    const id = SETTINGS[setting];
    return id ? text(id) : setting;
  };
  const value = (setting: string, written: string | null) => {
    if (written === null) return text("backup-value-default");
    if (written === "on" || written === "off") return text(written === "on" ? "backup-value-on" : "backup-value-off");
    if ((setting === "copy_format" || setting === "citation_style") && FORMATS.has(written)) return text(`citations-format-${written}` as MessageId);
    const known = VALUES[setting]?.[written];
    if (known) return text(known);
    if (setting === "library") {
      if (written === "zotero") return text("backup-value-zotero");
      const [kind, count] = written.split(":");
      if (kind === "zotero" && count) return text("backup-value-zotero-many", { count: Number(count) });
      if (kind === "zotero-group" && count) return text("backup-value-zotero-group", { id: count });
    }
    return written;
  };
  const change = (item: RestoreChange) => text("backup-change", { setting: label(item.setting), from: value(item.setting, item.from), to: value(item.setting, item.to) });

  const forget = () => {
    if (shown.current !== null) void api.cancelSettingsRestore(shown.current).catch(() => undefined);
    shown.current = null;
    setPreview(null);
  };
  const backUp = () => {
    if (busy) return;
    const current = ++operation.current;
    setBusy("backup"); setProblem(null); setSaved(null);
    void api.backUpSettings()
      .then((file) => { if (current === operation.current && file) setSaved(file); })
      .catch((error: unknown) => { if (current === operation.current) setProblem({ row: "backup", id: problemOf(error) }); })
      .finally(() => { if (current === operation.current) setBusy(null); });
  };
  const choose = () => {
    if (busy) return;
    forget();
    const current = ++operation.current;
    setBusy("restore"); setProblem(null); setSaved(null);
    void api.chooseSettingsBackup()
      .then((next) => {
        if (current !== operation.current) { if (next) void api.cancelSettingsRestore(next.ticket).catch(() => undefined); return; }
        shown.current = next?.ticket ?? null;
        setPreview(next);
      })
      .catch((error: unknown) => { if (current === operation.current) setProblem({ row: "restore", id: problemOf(error) }); })
      .finally(() => { if (current === operation.current) setBusy(null); });
  };
  const restore = () => {
    if (!preview || busy) return;
    const current = ++operation.current;
    const { ticket } = preview;
    shown.current = null;
    setBusy("restore");
    void api.restoreSettingsBackup(ticket)
      .then(() => { if (current === operation.current) setRestarting(true); })
      .catch((error: unknown) => {
        if (current !== operation.current) return;
        setPreview(null);
        setProblem({ row: "restore", id: problemOf(error) });
        setBusy(null);
      });
  };

  return (
    <Group id="another-computer" title={text("backup-title")}>
      <Row id="backup-settings" label={text("backup-label")} description={text("backup-description")}
        problem={problem?.row === "backup" ? text(problem.id) : undefined}>
        <Button aria-describedby="backup-settings-description" isDisabled={busy !== null || restarting} onPress={backUp}>{text("backup-button")}</Button>
        {saved && <span role="status" className="ui-settings-description">{text("backup-saved", { file: saved })}</span>}
      </Row>
      <Row id="restore-settings" label={text("restore-label")} description={text("restore-description")}
        problem={problem?.row === "restore" ? text(problem.id) : undefined}>
        <Button aria-describedby="restore-settings-description" isDisabled={busy !== null || restarting} onPress={choose}>{text("restore-button")}</Button>
      </Row>
      {preview && (
        <Row id="restore-preview" label={text("restore-from", { system: SYSTEMS[preview.made_on], version: preview.made_by })}>
          <div className="settings-restore" role="group" aria-labelledby="restore-preview">
            {preview.changes.length
              ? <ul className="ui-settings-description">{preview.changes.map((item) => <li key={item.setting}>{change(item)}</li>)}</ul>
              : !restorable(preview) && <p className="ui-settings-description">{text("restore-same")}</p>}
            {preview.notes.map((note) => <p key={note} className="ui-settings-description">{text(NOTES[note])}</p>)}
            {restarting
              ? <p role="status" className="ui-settings-description">{text("restore-restarting")}</p>
              : <div className="settings-restore-actions">
                  <Button onPress={forget} isDisabled={busy !== null}>{text("restore-cancel")}</Button>
                  {restorable(preview) && <Button variant="primary" onPress={restore} isDisabled={busy !== null}>{text("restore-confirm")}</Button>}
                </div>}
          </div>
        </Row>
      )}
    </Group>
  );
}
