// The project bibliography as the picker shows it. Native code owns the file and
// its generation; a row or action counts only for the generation it came from.
import { useCallback, useEffect, useRef, useState } from "react";
import type { PickerApi } from "../../App";
import type { EntryCopy, ProjectStatus, SearchResult } from "../../api";
import { pickerText } from "../../messages";
import type { useOperations } from "./useOperations";

export function useProjectBibliography(api: PickerApi, report: ReturnType<typeof useOperations>["report"], perform: ReturnType<typeof useOperations>["perform"]) {
  const { projectStatus, onProjectChanged, onProjectNotSaved, chooseProject, stopProject, copyProjectEntry, retryProject } = api;
  const [status, setStatus] = useState<ProjectStatus | null>(null);
  const [version, setVersion] = useState(0);
  const [notice, setNotice] = useState<string | null>(null);
  const reading = useRef(0);
  const shown = useRef(JSON.stringify(null));
  const read = useCallback(() => {
    const request = ++reading.current;
    void projectStatus?.().then((next) => {
      if (request !== reading.current) return;
      // Only a new answer asks searches again; nobody without a bibliography pays.
      const answer = JSON.stringify(next);
      if (answer === shown.current) return;
      shown.current = answer;
      setStatus(next);
      setVersion((old) => old + 1);
    }, () => { if (request === reading.current) report("project"); });
  }, [projectStatus, report]);
  useEffect(() => {
    read();
    // A read that finished before this listener existed sent its event to nobody.
    const changed = onProjectChanged?.(read);
    void changed?.then(read);
    const notSaved = onProjectNotSaved?.(() => report("project"));
    return () => {
      reading.current += 1;
      void changed?.then((stop) => stop());
      void notSaved?.then((stop) => stop());
    };
  }, [read, onProjectChanged, onProjectNotSaved, report]);

  const clearNotice = useCallback(() => setNotice(null), []);
  const copyEntry = useCallback((row: SearchResult) => {
    const name = status?.name;
    const generation = row.project?.generation;
    if (!copyProjectEntry || !name || generation === undefined) return;
    void perform("project", async () => {
      const outcome: EntryCopy = await copyProjectEntry(row.id, generation);
      setNotice(pickerText.projectEntry(outcome, row.key, name));
    });
  }, [copyProjectEntry, perform, status?.name]);
  return {
    status, version, notice, clearNotice, copyEntry,
    choose: chooseProject ? () => perform("project", chooseProject) : null,
    stop: stopProject ? () => perform("project", stopProject) : null,
    retry: retryProject ? () => perform("project", retryProject) : null,
  };
}

/** A row's mark, when it belongs to the bibliography on screen and the output uses keys. */
export function projectText(row: SearchResult, status: ProjectStatus | null): string | null {
  return row.project && status && row.project.generation === status.generation
    ? pickerText.projectState(row.project.state, status.name, status.last_good, row.project.lines) : null;
}
