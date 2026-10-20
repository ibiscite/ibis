import { useEffect, useId, useState } from "react";
import type { Sources } from "./bindings";
import { pickerText, setupText } from "./messages";

type SetupApi = {
  chooseLibrary: () => Promise<unknown>;
  useZotero: () => Promise<unknown>;
  findSources?: () => Promise<Sources>;
  openFoundLibrary?: (list: number, index: number) => Promise<unknown>;
};

type Action = "choose" | "zotero" | "findSources" | "openFound";

// First-run setup while no library is chosen: Zotero if it is open, `.bib`
// files found nearby, or any file through the system chooser. Only names are
// shown; nothing is read until one is picked.
export function LibrarySetup({ api, perform }: {
  api: SetupApi;
  perform: (action: Action, run: () => Promise<unknown>) => Promise<void>;
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
    </div>
  );
}
