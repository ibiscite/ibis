import type { LibraryStatus } from "./bindings";
import { fullStop, isolate, say } from "./i18n";

/** Text is English, so numbers and times are too; one place to change with translations. */
export const appLocale = "en";
const minute = 60_000;
/** How long "updated …" stays after the references change. */
export const recentUpdate = 10 * minute;
const numbers = new Intl.NumberFormat(appLocale);
const relative = new Intl.RelativeTimeFormat(appLocale, { numeric: "auto" });

/** The library's own sentences: the footer, its tooltip and the empty states. */
export const libraryMessages = {
  opening: (name: string | null) => name ? say("library-opening", { name }) : say("library-opening-unnamed"),
  reloading: (name: string | null) => name ? say("library-reloading", { name }) : say("library-reloading-unnamed"),
  openError: (name: string | null, reason: string, zotero = false, problem: LibraryStatus["problem"] = null) => {
    if (zotero) {
      // Each fix is named once: sharing is a setting, and a profile is chosen here.
      const fix = problem === "zotero_access_off" ? ` ${say("setup-zotero-access-off")}`
        : problem === "zotero_closed" || problem === null ? ` ${say("library-open-zotero-then-reload")}` : "";
      return say("library-zotero-not-read", { name: isolate(name ?? "Zotero"), reason: fullStop(reason) }) + fix;
    }
    return say("library-file-not-opened", { name: isolate(name ?? say("library-the-chosen-file")), reason: fullStop(reason) });
  },
  emptyTitle: (name: string | null) => name ? say("library-empty-title", { name }) : say("library-empty-title-unnamed"),
  /** The footer words: the source, then its state. A ticking age is kept
   * out of this text, so the live region never repeats a clock. The source is
   * separate so the footer can isolate its direction and shorten it first. */
  summaryParts: (status: LibraryStatus | null | undefined): [string | null, string] => {
    if (status === undefined) return [null, say("library-checking")];
    if (status === null) return [null, say("library-details-unavailable")];
    if (status.state === "loading") return [requestedLabel(status), ""];
    if (status.state === "none" || (!status.has_snapshot && !status.name)) return [null, say("library-none-chosen")];
    if (!status.has_snapshot) return [requestedLabel(status), ` · ${say("library-not-opened")}`];
    // After a failed switch the previous library is still the one searched: describe it.
    const source = snapshotLabel(status);
    if (status.stale) return [source, ` ${say(status.problem === "missing" && !status.snapshot_zotero ? "library-moved" : "library-not-updated")}`];
    return [source, ` · ${status.references === 0 ? say("library-no-references") : references(status.references)}` +
      readingSummary(status) +
      (status.error ? ` · ${isolate(requestedLabel(status))} ${say(status.problem === "missing" ? "library-moved" : "library-not-opened")}` : "")];
  },
  summary: (status: LibraryStatus | null | undefined) => {
    const [source, state] = libraryMessages.summaryParts(status);
    return (source ?? "") + state;
  },
  /** The quiet age after the summary, or null when the age does not matter:
   * "updated" only after a real change, "last read" for an old copy. */
  age: (status: LibraryStatus | null | undefined, now: number) => {
    if (status?.state === "loading") return say("library-age-opening");
    if (!status?.has_snapshot) return null;
    if (status.state === "reloading") return say("library-age-reloading");
    const { updated, checked } = times(status);
    if (status.stale) return checked === null ? null : shortAge(checked, now);
    if (status.error || updated === null || now - updated > recentUpdate) return null;
    return say("library-updated", { ago: ago(updated, now) });
  },
  /** Everything known, for the tooltip and the top of More: a short first
   * line (source and count) and a quieter second (libraries, check time, any
   * problem). Names are isolated so right-to-left names keep their order. */
  detail: (status: LibraryStatus | null | undefined, now: number, libraryNames: (status: LibraryStatus) => string): [string, string | null] => {
    const [source, state] = libraryMessages.summaryParts(status);
    const summary = (source === null ? "" : isolate(source)) + state;
    if (!status?.has_snapshot || status.state === "loading") return [summary, null];
    const { checked } = times(status);
    const parts = [
      zoteroSnapshot(status) && !(status.error && !status.stale) ? libraryNames(status) : null,
      checked === null ? null : isolate(say(status.stale ? "library-last-read-at" : "library-checked-at", { ago: ago(checked, now), time: clock(checked, now) })),
      status.error ? libraryMessages.openError(status.name, status.error, status.zotero, status.problem)
        : status.stale ? say(zoteroSnapshot(status) ? "library-zotero-unreachable" : "library-file-unreadable")
        : null,
      readingDetail(status),
    ].filter((part): part is string => part !== null);
    const first = status.stale ? `${isolate(snapshotLabel(status))} · ${references(status.references)}` : summary;
    return [first, parts.length > 0 ? parts.join(" · ") : null];
  },
};

const references = (count: number) => say("library-references", { count });

/** Entries the engine left out of the library (decision 0006). */
const skips = (code: string) => code === "duplicate-key" || code === "syntax";
type Reading = Pick<LibraryStatus, "warnings" | "reading">;
const reading = (status: Reading) => status.reading ?? [];

/** The footer's words for what reading found: entries skipped, then those to
 * check. More problems than the engine details is said as one honest count. */
function readingSummary(status: Reading) {
  if (status.warnings === 0) return "";
  const notices = reading(status);
  if (notices.length < status.warnings) return ` · ${say("library-entries-to-check", { count: status.warnings })}`;
  const skipped = notices.filter(({ code }) => skips(code)).length, check = status.warnings - skipped;
  return (skipped > 0 ? ` · ${say("library-skipped", { count: skipped })}` : "") + (check > 0 ? ` · ${say("library-to-check", { count: check })}` : "");
}

/** "line 3", "lines 3 and 9", "lines 3, 9, 12 and 40", then "and others". */
function lines(found: readonly (number | null)[]) {
  const known = found.filter((line): line is number => line !== null).map((line) => numbers.format(line));
  if (known.length === 0) return null;
  if (known.length === 1) return say("library-line", { line: known[0]! });
  const shown = known.slice(0, 4);
  return say(known.length > 4 ? "library-lines-and-others" : "library-lines", { lines: shown.slice(0, -1).join(", "), last: shown.at(-1)! });
}
const at = (found: readonly (number | null)[]) => { const where = lines(found); return where ? ` ${say("library-where", { lines: where })}` : ""; };

type Notice = { line: number | null; why?: string | null };
/** "line 21: it has no citation key; line 25: it never closes", from the
 * engine's words for each notice, or just the lines when it gives none. */
function explained(found: readonly Notice[]) {
  if (!found.some(({ why }) => why)) return lines(found.map(({ line }) => line));
  const shown = found.slice(0, 3).map(({ line, why }) =>
    line === null ? why ?? "" : why ? say("library-line-why", { line: numbers.format(line), why }) : say("library-line", { line: numbers.format(line) }));
  return [...shown, ...found.length > 3 ? [say("library-and-more", { count: found.length - 3 })] : []].join("; ");
}
const because = (found: readonly Notice[]) => { const where = explained(found); return where ? ` ${say("library-where", { lines: where })}` : ""; };

const otherNotices = {
  "work-limit": "library-work-limit",
  "cloud-synced-directory": "library-cloud-synced",
  "citation-keys-unavailable": "library-keys-unavailable",
} as const;
const isOtherNotice = (code: string): code is keyof typeof otherNotices => Object.hasOwn(otherNotices, code);

/** Where to look: which entries were skipped and why, then lines to check.
 * Keys are the person's own and shown only to them. */
function readingDetail(status: Reading) {
  const notices = reading(status);
  if (notices.length === 0) return null;
  const repeated = new Map<string, (number | null)[]>();
  for (const { code, key, line } of notices) if (code === "duplicate-key") repeated.set(key ?? "", [...repeated.get(key ?? "") ?? [], line]);
  const unreadable = notices.filter(({ code }) => code === "syntax");
  const skipped = notices.filter(({ code }) => skips(code)).length;
  const keys = [...repeated];
  const reasons = [
    ...keys.slice(0, 3).map(([key, found]) => say("library-key-users", { count: found.length, key: isolate(key) }) + at(found)),
    ...keys.length > 3 ? [say("library-others-share", { count: keys.slice(3).reduce((sum, [, found]) => sum + found.length, 0) })] : [],
    ...unreadable.length > 0 ? [say("library-could-not-read", { skipped, count: unreadable.length }) + because(unreadable)] : [],
  ];
  const check = notices.filter(({ code }) => !skips(code) && !isOtherNotice(code));
  const checkLines = explained(check);
  const sentences = [
    skipped > 0 ? say("library-skipped-because", { skipped, reasons: reasons.join(", ") }) : null,
    check.length > 0 ? checkLines ? say("library-check", { lines: checkLines }) : say("library-check-library") : null,
    ...[...new Set(notices.map(({ code }) => code))].filter(isOtherNotice).map((code) => say(otherNotices[code])),
    notices.length < status.warnings ? say("library-more-not-listed", { count: status.warnings - notices.length }) : null,
  ].filter((text): text is string => text !== null);
  return sentences.join(" ");
}

/** The library asked for, which may not have opened. */
function requestedLabel(status: LibraryStatus) {
  return status.zotero ? "Zotero" : status.name ?? "Library";
}

/** Whether the searched references are Zotero's (older backends: the request). */
function zoteroSnapshot(status: LibraryStatus) {
  return status.snapshot_zotero ?? status.zotero;
}

/** The library actually searched. */
function snapshotLabel(status: LibraryStatus) {
  return zoteroSnapshot(status) ? "Zotero" : status.snapshot_name ?? "Library";
}

/** Times from an older backend or fixture may be missing. */
function times(status: LibraryStatus) {
  return { updated: status.updated_at ?? null, checked: status.checked_at ?? null };
}

/** Short relative times, then a date with the year when different. Never in the future. */
function shortAge(at: number, now: number) {
  const seconds = Math.max(0, (now - at) / 1000);
  if (seconds < 45) return say("time-short", { unit: "now", n: 0 });
  if (seconds < 3600) return say("time-short", { unit: "minute", n: Math.max(1, Math.round(seconds / 60)) });
  if (seconds < 86400) return say("time-short", { unit: "hour", n: Math.round(seconds / 3600) });
  return say("time-short", { unit: "day", n: Math.round(seconds / 86400) });
}

export function ago(at: number, now: number) {
  const seconds = Math.max(0, (now - at) / 1000);
  if (seconds < 45) return say("time-just-now");
  if (seconds < 90 * 60) return relative.format(-Math.max(1, Math.round(seconds / 60)), "minute");
  if (seconds < 36 * 3600) return relative.format(-Math.round(seconds / 3600), "hour");
  if (seconds < 7 * 86400) return relative.format(-Math.round(seconds / 86400), "day");
  const year = new Date(at).getFullYear() === new Date(now).getFullYear() ? undefined : "numeric";
  return say("time-on", { date: new Intl.DateTimeFormat(appLocale, { month: "short", day: "numeric", year }).format(at) });
}

/** Clock time today, otherwise date and time. */
function clock(at: number, now: number) {
  const sameDay = new Date(at).toDateString() === new Date(now).toDateString();
  return new Intl.DateTimeFormat(appLocale, sameDay ? { timeStyle: "short" } : { dateStyle: "medium", timeStyle: "short" }).format(at);
}
