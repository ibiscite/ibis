import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import type { ShortcutStatus, Sources } from "./bindings";
import { ChoiceIcon } from "./icons";
import { keysFor } from "./keys";
import { setupText } from "./messages";
import { currentPlatform } from "./platform";
import { shortcutLabel } from "./keys";
import { ChoiceList, type Choice } from "./ui/ChoiceList";
import { IbisMark } from "./ui/IbisMark";
import "./LibrarySetup.css";

type SetupApi = {
  chooseLibrary: () => Promise<unknown>;
  useZotero: () => Promise<unknown>;
  useSampleLibrary?: (enabled: boolean) => Promise<unknown>;
  findSources?: () => Promise<Sources>;
  openFoundLibrary?: (list: number, index: number) => Promise<unknown>;
};

export type SourceChoice = { label: string; open: () => void; icon?: ReactNode };
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
  const { chooseLibrary, useZotero, useSampleLibrary: changeSamples, findSources, openFoundLibrary } = api;
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

  const looking = sources === undefined;
  const zotero = sources?.zotero;
  const files = openFoundLibrary ? sources?.files ?? [] : [];
  const onlyFile = files.length === 1 ? files[0] : undefined;
  const lookAgain = () => { intent.current += 1; setShowFiles(false); setSources(undefined); setSearch((n) => n + 1); };
  const outcome = looking ? setupText.looking
    : sources && files.length === 0 && zotero === "absent" ? setupText.nothingFound
    : sources ? setupText.found(files.length, zotero === "available") : setupText.findHelp;

  const choices: Choice[] = [
    ...(empty?.source ? [{ id: "source", title: empty.source.label, icon: empty.source.icon ?? <ChoiceIcon name={empty.zotero ? "library" : "file"} />,
      keys: keysFor("source") }] : []),
    { id: "zotero", title: setupText.zotero, help: setupText.zoteroHelp, icon: <ChoiceIcon name="library" />,
      tag: zotero === "available" || empty?.zotero ? setupText.foundOn(currentPlatform()) : undefined },
    { id: "file", title: setupText.file, help: setupText.fileHelp, icon: <ChoiceIcon name="file" /> },
    ...(!empty && findSources ? [{
      id: "find", title: onlyFile ? setupText.foundFile(onlyFile.name)
        : files.length > 1 ? setupText.chooseFound : search === 0 ? setupText.findLibraries : setupText.lookAgain,
      help: onlyFile?.folder ? setupText.foundIn(onlyFile.folder) : outcome,
      // Stays enabled while looking so keyboard focus stays on it; presses wait.
      icon: <ChoiceIcon name="find" />,
    }] : []),
    ...(!empty && changeSamples ? [{ id: "samples", title: setupText.samples, help: setupText.samplesHelp,
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
  const notes = [zotero === "access_off" && setupText.zoteroAccessOff, sources?.cut_short && setupText.cutShort].filter(Boolean);
  const hasShortcut = Boolean(shortcut?.shortcut.trim());
  const shortcutNote = !shortcut ? undefined : shortcut.registered && hasShortcut
    ? setupText.returnHint(shortcut.desktop ? shortcut.shortcut : shortcutLabel(shortcut.shortcut, currentPlatform() === "macos"))
    : hasShortcut ? shortcut.desktop?.guidance || setupText.shortcutUnavailable
    : `${setupText.noShortcut} ${shortcut.desktop?.guidance || setupText.openWithoutShortcut}`;
  const foundChoices: Choice[] = files.map((file, index) => ({
    id: `found-${index}`, title: file.name, tag: file.folder || undefined, icon: <ChoiceIcon name="file" />,
  }));
  foundChoices.push({ id: "back", title: setupText.back, icon: <ChoiceIcon name="swap" /> });
  return (
    <div className={`library-setup${empty ? "" : " library-welcome"}`}>
      {showFiles && !empty ? <ChoiceList key="found" title={setupText.foundTitle} choices={foundChoices} onChoose={choose} autoFocus
        footnote={notes.length > 0 ? notes.join(" ") : undefined} />
        : <ChoiceList key="starts" title={empty?.title ?? setupText.sourcesTitle} line={empty ? setupText.emptyLine : setupText.sourcesLine}
          choices={choices} onChoose={choose} autoFocus={autoFocus || !empty} grid={!empty && choices.length === 4} focusId={focusTile ? "find" : undefined}
          mark={!empty && <IbisMark className="welcome-mark" size="welcome" />}
          footnote={empty ? setupText.dropFile : notes.length > 0 || shortcutNote ? <>
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
  return <ChoiceList title={setupText.sampleTitle} choices={[{
    id: "own", title: setupText.ownLibrary, help: setupText.ownLibraryHelp, icon: <ChoiceIcon name="swap" />,
  }]} onChoose={() => void perform("samples", () => changeSamples(false))} />;
}
