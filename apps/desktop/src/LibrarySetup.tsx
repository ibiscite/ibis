import { useEffect, useState } from "react";
import type { Choice as Format } from "./features/picker/choice";
import type { Sources } from "./bindings";
import { ChoiceIcon } from "./icons";
import { setupText } from "./messages";
import { currentPlatform } from "./platform";
import { ChoiceList, type Choice } from "./ui/ChoiceList";

type SetupApi = {
  chooseLibrary: () => Promise<unknown>;
  useZotero: () => Promise<unknown>;
  findSources?: () => Promise<Sources>;
  openFoundLibrary?: (list: number, index: number) => Promise<unknown>;
};

type Action = "choose" | "zotero" | "findSources" | "openFound";

// Kept here for imports written before the label moved to keys.ts.
export { shortcutLabel } from "./keys";

/** Where people write, and the copy format that suits each (first run, step 2). */
const WRITING_PLACES = [
  { id: "markdown", choice: "pandoc" },
  { id: "latex", choice: "cite" },
  { id: "typst", choice: "typst" },
] as const satisfies readonly { id: keyof typeof setupText.places; choice: Format }[];

/** First run, step 1, and the screen while no library is chosen: Zotero, a
 * .bib file through the system chooser, and, only when asked, libraries found
 * nearby. Only names are shown; nothing is read until one is picked (#998). */
export function LibrarySetup({ api, perform }: {
  api: SetupApi;
  perform: (action: Action, run: () => Promise<unknown>) => Promise<void>;
}) {
  const { chooseLibrary, useZotero, findSources, openFoundLibrary } = api;
  // Undefined while looking; null before an explicit search or after failure.
  const [sources, setSources] = useState<Sources | null | undefined>(null);
  const [search, setSearch] = useState(0);

  useEffect(() => {
    if (!findSources || search === 0) { setSources(null); return; }
    let current = true;
    setSources(undefined);
    void perform("findSources", () => findSources().then(
      (found) => { if (current) setSources(found); },
      // A search replaced or left behind is nobody's failure.
      (error: unknown) => { if (current) { setSources(null); throw error; } },
    ));
    return () => { current = false; };
  }, [findSources, search]);

  const looking = sources === undefined;
  const zotero = sources?.zotero;
  const files = openFoundLibrary ? sources?.files ?? [] : [];
  const lookAgain = () => { setSources(undefined); setSearch((n) => n + 1); };
  const outcome = looking ? setupText.looking
    : sources && files.length === 0 && zotero === "absent" ? setupText.nothingFound
    : sources ? setupText.found(files.length, zotero === "available") : setupText.findHelp;

  const choices: Choice[] = [
    { id: "zotero", title: setupText.zotero, help: setupText.zoteroHelp, icon: <ChoiceIcon name="library" />,
      tag: zotero === "available" ? setupText.foundOn(currentPlatform()) : undefined },
    { id: "file", title: setupText.file, help: setupText.fileHelp, icon: <ChoiceIcon name="file" /> },
    ...files.map((file, index): Choice => ({
      id: `found-${index}`, title: file.name, tag: file.folder || undefined, icon: <ChoiceIcon name="file" />,
    })),
    ...(findSources ? [{
      id: "find", title: search === 0 ? setupText.findLibraries : setupText.lookAgain, help: outcome,
      // Stays enabled while looking so keyboard focus stays on it; presses wait.
      icon: <ChoiceIcon name="find" />,
    }] : []),
  ];
  const choose = (id: string) => {
    if (id === "zotero") void perform("zotero", useZotero);
    else if (id === "file") void perform("choose", chooseLibrary);
    else if (id === "find") { if (!looking) lookAgain(); }
    else if (id.startsWith("found-") && sources && openFoundLibrary) {
      const index = Number(id.slice("found-".length));
      void perform("openFound", () => openFoundLibrary(sources.list, index).catch((error: unknown) => {
        // The list was replaced since it was shown: show the new one.
        lookAgain();
        throw error;
      }));
    }
  };
  const notes = [zotero === "access_off" && setupText.zoteroAccessOff, sources?.cut_short && setupText.cutShort].filter(Boolean);
  return (
    <div className="library-setup">
      <ChoiceList title={setupText.sourcesTitle} line={setupText.sourcesLine} choices={choices} onChoose={choose}
        footnote={notes.length > 0 ? notes.join(" ") : undefined} />
      {/* Search progress and outcome, announced once; the row shows the same words. */}
      {findSources && search > 0 && <p className="visually-hidden" role="status">{outcome}</p>}
    </div>
  );
}

/** First run, step 2: where the person writes picks the copy format. */
export function WritingSetup({ format, choose, shortcut }: {
  format: Format;
  choose: (choice: Format) => void;
  /** The shortcut that opens Ibis, when it is registered. */
  shortcut?: string | null;
}) {
  const choices: Choice[] = WRITING_PLACES.map((place) => ({
    id: place.id, title: setupText.places[place.id], help: setupText.placeExamples[place.id],
    icon: <ChoiceIcon name="write" />, tag: format === place.choice ? setupText.current : undefined,
  }));
  return (
    <ChoiceList title={setupText.writingTitle} line={setupText.writingLine} choices={choices}
      onChoose={(id) => choose(WRITING_PLACES.find((place) => place.id === id)!.choice)}
      footnote={shortcut ? setupText.shortcut(shortcut) : undefined} />
  );
}
