import { useEffect, useId, useState } from "react";
import type { Choice } from "./App";
import type { Sources } from "./bindings";
import { pickerText, setupText } from "./messages";
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

/** A shortcut as people read it: `⌃⌘R` on macOS, `Ctrl+Shift+Y` elsewhere. */
export function shortcutLabel(shortcut: string, mac: boolean): string {
  // Settings may spell keys in any case and with the aliases the shortcut
  // parser accepts, such as `CmdOrCtrl` or `KeyR`.
  const names: Record<string, [mac: string, other: string]> = {
    control: ["⌃", "Ctrl"], ctrl: ["⌃", "Ctrl"],
    alt: ["⌥", "Alt"], option: ["⌥", "Alt"],
    shift: ["⇧", "Shift"],
    command: ["⌘", "Super"], cmd: ["⌘", "Super"], super: ["⌘", "Super"], meta: ["⌘", "Super"],
    commandorcontrol: ["⌘", "Ctrl"], cmdorctrl: ["⌘", "Ctrl"], commandorctrl: ["⌘", "Ctrl"], cmdorcontrol: ["⌘", "Ctrl"],
    space: ["Space", "Space"],
  };
  const keys = shortcut.split("+").map((key) => key.trim()).filter(Boolean).map((key) => {
    const known = names[key.toLowerCase()];
    if (known) return mac ? known[0] : known[1];
    const plain = /^(?:Key|Digit)(.)$/i.exec(key)?.[1] ?? key;
    return plain.length === 1 ? plain.toUpperCase() : plain;
  });
  return keys.join(mac ? "" : "+");
}

// First-run setup while no library is chosen: Zotero if it is open, `.bib`
// files found nearby, or any file through the system chooser. Only names are
// shown; nothing is read until one is picked.
export function LibrarySetup({ api, perform, writing, shortcut }: {
  api: SetupApi;
  perform: (action: Action, run: () => Promise<unknown>) => Promise<void>;
  /** The copy format now, whether formatted styles are offered, and how to change it. */
  writing?: { format: Choice; formatted: boolean; choose: (choice: Choice) => void };
  /** The shortcut that opens Ibis, when it is registered. */
  shortcut?: string | null;
}) {
  const { chooseLibrary, useZotero, findSources, openFoundLibrary } = api;
  // Undefined while looking; null when looking failed or is not offered.
  const [sources, setSources] = useState<Sources | null>();
  const [search, setSearch] = useState(0);
  const heading = useId();

  useEffect(() => {
    if (!findSources) { setSources(null); return; }
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
  const lookAgain = () => setSearch((n) => n + 1);
  const outcome = looking ? setupText.looking
    : sources && files.length === 0 && zotero === "absent" ? setupText.nothingFound
    : sources ? setupText.found(files.length, zotero === "available") : "";

  return (
    <div className="library-setup">
      <p>{pickerText.firstRun}</p>
      <div className="setup-choices">
        <button type="button" aria-label={pickerText.firstRunChoose} onClick={() => void perform("choose", chooseLibrary)}>
          {pickerText.chooseLibrary}
        </button>
        <button type="button" title={pickerText.useZoteroTitle}
          onClick={() => void perform("zotero", useZotero)}>
          {pickerText.useZotero}
        </button>
      </div>
      {findSources && <p className="empty-hint" role="status">{outcome}</p>}
      {zotero === "access_off" && <p className="empty-hint">{setupText.zoteroAccessOff}</p>}
      {files.length > 0 && sources && (
        <section aria-labelledby={heading} className="found-files">
          <p className="empty-hint" id={heading}>{setupText.foundFiles}</p>
          <ul>
            {files.map((file, index) => (
              <li key={`${sources.list}-${index}`}>
                <button type="button" title={setupText.openFound(file.name, file.folder)}
                  onClick={() => void perform("openFound", () => openFoundLibrary!(sources.list, index).catch((error: unknown) => {
                    // The list was replaced since it was shown: show the new one.
                    lookAgain();
                    throw error;
                  }))}>
                  <bdi className="found-name">{file.name}</bdi>
                  {file.folder && <>{" "}<span className="found-folder">{setupText.inFolder}{" "}<bdi>{file.folder}</bdi></span></>}
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
      {sources?.cut_short && <p className="empty-hint">{setupText.cutShort}</p>}
      {findSources && (
        // Stays in place while looking, so focus is not lost.
        <button type="button" aria-disabled={looking || undefined} onClick={() => { if (!looking) lookAgain(); }}>
          {setupText.lookAgain}
        </button>
      )}
      {writing && (
        <section aria-labelledby={`${heading}-writing`} className="setup-writing">
          <p className="empty-hint" id={`${heading}-writing`}>{setupText.whereYouWrite}</p>
          <div className="setup-choices">
            {WRITING_PLACES.filter((place) => !place.formatted || writing.formatted).map((place) => (
              <button type="button" key={place.name} aria-pressed={writing.format === place.choice}
                onClick={() => writing.choose(place.choice)}>
                {place.name}
              </button>
            ))}
          </div>
          {writing.format.startsWith("style:") && (
            <p className="empty-hint">{setupText.styleHint(STYLE_NAMES[writing.format.slice("style:".length) as StyleId] ?? "")}</p>
          )}
        </section>
      )}
      {shortcut && <p className="empty-hint">{setupText.shortcut(shortcut)}</p>}
    </div>
  );
}
