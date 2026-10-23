import { useEffect, useId, useState } from "react";
import type { Choice } from "./features/picker/choice";
import type { Sources } from "./bindings";
import { loginItemText, pickerText, setupText } from "./messages";
import { currentPlatform } from "./platform";
import { Button, ToggleButton } from "./ui/Button";
import { STYLE_NAMES, type StyleId } from "./format/styles";

type SetupApi = {
  chooseLibrary: () => Promise<unknown>;
  useZotero: () => Promise<unknown>;
  findSources?: () => Promise<Sources>;
  openFoundLibrary?: (list: number, index: number) => Promise<unknown>;
};

type Action = "choose" | "zotero" | "findSources" | "openFound";

/** Where people write, and the copy format that suits each. */
const WRITING_PLACES: { name: string; choice: Choice; formatted?: true }[] = [
  { name: setupText.places.markdown, choice: "pandoc" },
  { name: setupText.places.latex, choice: "cite" },
  { name: setupText.places.typst, choice: "typst" },
  { name: setupText.places.word, choice: "style:apa", formatted: true },
];

// Kept here for imports written before the label moved to keys.ts.
export { shortcutLabel } from "./keys";

// First-run setup while no library is chosen: Zotero if it is open, `.bib`
// files found nearby, or any file through the system chooser. Only names are
// shown; nothing is read until one is picked.
export function LibrarySetup({ api, perform, writing, shortcut, startAtLogin }: {
  api: SetupApi;
  perform: (action: Action, run: () => Promise<unknown>) => Promise<void>;
  /** The copy format now, whether formatted styles are offered, and how to change it. */
  writing?: { format: Choice; formatted: boolean; choose: (choice: Choice) => void };
  /** The shortcut that opens Ibis, when it is registered. */
  shortcut?: string | null;
  /** Starting at login, offered once here and kept in More; off until chosen. */
  startAtLogin?: { name: string; enabled: boolean; toggle: () => void };
}) {
  const { chooseLibrary, useZotero, findSources, openFoundLibrary } = api;
  // Undefined while looking; null before an explicit search or after failure.
  const [sources, setSources] = useState<Sources | null | undefined>(null);
  const [search, setSearch] = useState(0);
  const heading = useId();

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
    : sources ? setupText.found(files.length, zotero === "available") : "";

  return (
    <div className="library-setup">
      <p>{pickerText.firstRun}</p>
      <div className="setup-choices">
        <Button variant={zotero === "available" ? "secondary" : "primary"}
          aria-label={pickerText.firstRunChoose} onPress={() => void perform("choose", chooseLibrary)}>
          {pickerText.chooseLibrary}
        </Button>
        <Button variant={zotero === "available" ? "primary" : "secondary"} title={pickerText.useZoteroTitle}
          onPress={() => void perform("zotero", useZotero)}>
          {pickerText.useZotero}
        </Button>
      </div>
      {findSources && <p className="empty-hint" role="status">{outcome}</p>}
      {zotero === "access_off" && <p className="empty-hint">{setupText.zoteroAccessOff}</p>}
      {files.length > 0 && sources && (
        <section aria-labelledby={heading} className="found-files">
          <p className="empty-hint" id={heading}>{setupText.foundFiles}</p>
          <ul>
            {files.map((file, index) => (
              <li key={`${sources.list}-${index}`}>
                <Button title={setupText.openFound(file.name, file.folder)}
                  onPress={() => void perform("openFound", () => openFoundLibrary!(sources.list, index).catch((error: unknown) => {
                    // The list was replaced since it was shown: show the new one.
                    lookAgain();
                    throw error;
                  }))}>
                  <bdi className="found-name">{file.name}</bdi>
                  {file.folder && <>{" "}<span className="found-folder">{setupText.inFolder}{" "}<bdi>{file.folder}</bdi></span></>}
                </Button>
              </li>
            ))}
          </ul>
        </section>
      )}
      {sources?.cut_short && <p className="empty-hint">{setupText.cutShort}</p>}
      {findSources && (
        // Stays focusable while looking; the outcome status announces progress.
        <Button aria-disabled={looking || undefined} onPress={() => { if (!looking) lookAgain(); }}>
          {search === 0 ? setupText.findLibraries : setupText.lookAgain}
        </Button>
      )}
      {writing && (
        <section aria-labelledby={`${heading}-writing`} className="setup-writing">
          <p className="empty-hint" id={`${heading}-writing`}>{setupText.whereYouWrite}</p>
          <div className="setup-choices">
            {WRITING_PLACES.filter((place) => !place.formatted || writing.formatted).map((place) => (
              <ToggleButton key={place.name} isSelected={writing.format === place.choice}
                onChange={() => writing.choose(place.choice)}>
                {place.name}
              </ToggleButton>
            ))}
          </div>
          {writing.format.startsWith("style:") && (
            <p className="empty-hint">{setupText.styleHint(STYLE_NAMES[writing.format.slice("style:".length) as StyleId] ?? "")}</p>
          )}
        </section>
      )}
      {shortcut && <p className="empty-hint">{setupText.shortcut(shortcut)}</p>}
      {startAtLogin && (
        // The shortcut works only while Ibis runs, so this sits beside it.
        <label className="setup-login empty-hint" title={loginItemText.title(currentPlatform())}>
          <input type="checkbox" checked={startAtLogin.enabled} onChange={startAtLogin.toggle} />
          {loginItemText.sentence(startAtLogin.name, currentPlatform())}
        </label>
      )}
    </div>
  );
}
