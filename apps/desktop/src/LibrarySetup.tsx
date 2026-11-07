import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import type { ShortcutStatus, Sources, ZoteroPresence } from "./bindings";
import { ChoiceIcon } from "./icons";
import { keysFor } from "./keys";
import { useInterfaceMessages } from "./i18n";
import { say } from "./i18n";
import { currentPlatform } from "./platform";
import { shortcutLabel } from "./keys";
import { ChoiceList, type Choice } from "./ui/ChoiceList";
import { IbisMark } from "./ui/IbisMark";
import "./LibrarySetup.css";

type SetupApi = {
  chooseLibrary: () => Promise<unknown>;
  useZotero: () => Promise<unknown>;
  useSampleLibrary?: (enabled: boolean) => Promise<unknown>;
  findZotero?: () => Promise<ZoteroPresence>;
  findSources?: () => Promise<Sources>;
  openFoundLibrary?: (list: number, index: number) => Promise<unknown>;
};

export type SourceChoice = { label: string; open: () => void; icon?: ReactNode; help?: string };
export type EmptyLibraryChoices = { title: ReactNode; zotero: boolean; source?: SourceChoice };

type Action = "choose" | "zotero" | "findSources" | "openFound" | "samples";

/** First run, and the screen while no library is chosen: Zotero, a
 * library file through the system chooser, and, only when asked, libraries found
 * nearby. Only names are shown; nothing is read until one is picked (#998). */
export function LibrarySetup({ api, perform, autoFocus = false, empty, shortcut, onWelcomeChange }: {
  api: SetupApi;
  perform: (action: Action, run: () => Promise<unknown>, isCurrent?: () => boolean) => Promise<void>;
  /** Focus the first row on mount, when the row that led here is gone. */
  autoFocus?: boolean;
  empty?: EmptyLibraryChoices;
  /** The picker's current status; setup never reads or guesses a shortcut. */
  shortcut?: ShortcutStatus | null;
  /** Search is hidden in welcome, including when reached through recovery. */
  onWelcomeChange?: (shown: boolean) => void;
}) {
  const { text } = useInterfaceMessages();
  const { chooseLibrary, useZotero, useSampleLibrary: changeSamples, findZotero, findSources, openFoundLibrary } = api;
  const welcome = !empty;
  useLayoutEffect(() => {
    onWelcomeChange?.(welcome);
    return () => onWelcomeChange?.(false);
  }, [welcome, onWelcomeChange]);
  // Undefined while looking; null before an explicit search or after failure.
  const [sources, setSources] = useState<Sources | null | undefined>(null);
  const [search, setSearch] = useState(0);
  const [showFiles, setShowFiles] = useState(false);
  const [focusTile, setFocusTile] = useState(false);
  const intent = useRef(0);
  useEffect(() => () => { intent.current += 1; }, []);

  // biome-ignore lint/correctness/useExhaustiveDependencies: perform only records the outcome; a new one must never start another search.
  useEffect(() => {
    intent.current += 1;
    if (!findSources || search === 0) { setSources(null); return; }
    let current = true;
    setSources(undefined);
    void perform("findSources", () => findSources().then(
      (found) => { if (current) setSources(found); },
      (error: unknown) => { if (current) setSources(null); throw error; },
    ), () => current);
    return () => { current = false; };
  }, [findSources, search]);

  // Ibis still asks: a Zotero found here is offered first, so Enter takes it,
  // and nothing is read until it is chosen.
  const [present, setPresent] = useState<ZoteroPresence | null>(null);
  useEffect(() => {
    if (!welcome || !findZotero) return;
    let current = true;
    void findZotero().then((found) => { if (current) setPresent(found); }, () => {});
    return () => { current = false; };
  }, [welcome, findZotero]);

  const looking = sources === undefined;
  const zotero = sources?.zotero ?? present;
  const zoteroHere = zotero === "available" || zotero === "closed" || zotero === "access_off";
  const files = openFoundLibrary ? sources?.files ?? [] : [];
  const onlyFile = files.length === 1 ? files[0] : undefined;
  const lookAgain = () => { intent.current += 1; setShowFiles(false); setSources(undefined); setSearch((n) => n + 1); };
  const outcome = looking ? say("setup-looking")
    : sources && files.length === 0 && zotero === "absent" ? say("setup-nothing-found")
    : sources ? [zotero === "available" ? say("setup-zotero-open") : zotero === "closed" ? say("setup-zotero-installed") : "",
      files.length ? say("setup-bib-found", { count: files.length }) : ""].filter(Boolean).join(" ") : say("setup-find-help");

  const choices: Choice[] = [
    { id: "zotero", title: text("choice-use-zotero"), help: text("choice-zotero-help"), icon: <ChoiceIcon name="library" />,
      tag: zoteroHere || empty?.zotero ? say("setup-found-on", { platform: currentPlatform() }) : undefined, suggested: welcome && zoteroHere },
    { id: "file", title: text("choice-library-file"), help: text("choice-library-file-help"), icon: <ChoiceIcon name="file" /> },
    ...(empty?.source ? [{ id: "source", title: empty.source.label, help: empty.source.help, icon: empty.source.icon ?? <ChoiceIcon name={empty.zotero ? "library" : "file"} />, keys: keysFor("source") }] : []),
    ...(!empty && findSources ? [{
      id: "find", title: onlyFile ? text("choice-use-file", { name: onlyFile.name })
        : files.length > 1 ? say("setup-choose-found") : search === 0 ? say("setup-find-libraries") : say("setup-look-again"),
      help: onlyFile?.folder ? say("setup-found-in", { folder: onlyFile.folder }) : outcome,
      // Stays enabled while looking so keyboard focus stays on it; presses wait.
      icon: <ChoiceIcon name="find" />,
    }] : []),
    ...(!empty && changeSamples ? [{ id: "samples", title: say("setup-samples"), help: say("setup-samples-help"),
      icon: <ChoiceIcon name="sample" /> }] : []),
  ];
  const openFile = (index: number) => {
    if (!sources || !openFoundLibrary) return;
    const attempt = ++intent.current;
    const isCurrent = () => attempt === intent.current;
    let refused = false;
    void perform("openFound", () => openFoundLibrary(sources.list, index).catch((error: unknown) => {
      refused = true;
      throw error;
    }), isCurrent).then(() => {
      // Report this attempt before its rescan takes a new identity. An older
      // completion may neither clear a newer failure nor replace its list.
      if (refused && isCurrent()) lookAgain();
    });
  };
  const choose = (id: string) => {
    intent.current += 1;
    if (id === "source") empty?.source?.open();
    else if (id === "zotero") void perform("zotero", useZotero);
    else if (id === "file") void perform("choose", chooseLibrary);
    else if (id === "samples" && changeSamples) void perform("samples", () => changeSamples(true));
    else if (id === "find") {
      if (looking) return;
      if (files.length === 1) openFile(0);
      else if (files.length > 1) setShowFiles(true);
      else lookAgain();
    }
    else if (id === "back") { setFocusTile(true); setShowFiles(false); }
    else if (id.startsWith("found-")) openFile(Number(id.slice("found-".length)));
  };
  const notes = [zotero === "access_off" && say("setup-zotero-access-off"), sources?.cut_short && say("setup-cut-short")].filter(Boolean);
  const hasShortcut = Boolean(shortcut?.shortcut.trim());
  const shortcutNote = !shortcut ? undefined : shortcut.registered && hasShortcut
    ? say("setup-return-hint", { shortcut: shortcut.desktop ? shortcut.shortcut : shortcutLabel(shortcut.shortcut, currentPlatform() === "macos") })
    : hasShortcut ? shortcut.desktop?.guidance || say("setup-shortcut-unavailable")
    : `${say("setup-no-shortcut")} ${shortcut.desktop?.guidance || say("setup-open-without-shortcut")}`;
  const foundChoices: Choice[] = files.map((file, index) => ({
    id: `found-${index}`, title: text("choice-use-file", { name: file.name }), tag: file.folder || undefined, icon: <ChoiceIcon name="file" />,
  }));
  foundChoices.push({ id: "back", title: text("choice-back"), icon: <ChoiceIcon name="swap" /> });
  return (
    <div className={`library-setup${empty ? "" : " library-welcome"}`}>
      {showFiles && !empty ? <ChoiceList key="found" title={say("setup-found-title")} choices={foundChoices} onChoose={choose} autoFocus
        footnote={notes.length > 0 ? notes.join(" ") : undefined} />
        : <ChoiceList key="starts" title={empty?.title ?? say("setup-sources-title")} line={empty ? say("setup-empty-line") : say("setup-sources-line", { platform: currentPlatform() })}
          choices={choices} onChoose={choose} autoFocus={autoFocus || !empty} grid={!empty && choices.length === 4} focusId={focusTile ? "find" : undefined}
          mark={!empty && <IbisMark className="welcome-mark" size="welcome" />}
          footnote={empty ? text("choice-drop-file") : notes.length > 0 || shortcutNote ? <>
            {notes.length > 0 && <span className="welcome-note">{notes.join(" ")}</span>}
            {shortcutNote && <span className="welcome-note">{shortcutNote}</span>}
          </> : undefined} />}
      {/* Search progress and outcome, announced once; the row shows the same words. */}
      {findSources && search > 0 && <p className="visually-hidden" role="status">{outcome}</p>}
    </div>
  );
}

/** The same keyboard choice rows let a sample session return to setup. */
export function SampleLibraryExit({ useSampleLibrary: changeSamples, perform }: {
  useSampleLibrary?: SetupApi["useSampleLibrary"];
  perform: (action: Action, run: () => Promise<unknown>) => Promise<void>;
}) {
  if (!changeSamples) return null;
  return <ChoiceList title={say("setup-sample-title")} choices={[{
    id: "own", title: say("setup-own-library"), help: say("setup-own-library-help"), icon: <ChoiceIcon name="swap" />,
  }]} onChoose={() => void perform("samples", () => changeSamples(false))} />;
}
