import type { LibraryStatus } from "./api";

// User-facing library text stays together for future translation.
export const libraryMessages = {
  summary: (status: LibraryStatus | null | undefined) => {
    if (status?.has_snapshot) {
      return `${status.snapshot_name}: ${status.references} references` +
        (status.warnings > 0 ? `, ${status.warnings} could not be read fully` : "");
    }
    return status?.name ? status.name : status === null ? "Library status unavailable"
      : status === undefined ? "Checking library…" : "No library chosen";
  },
  openError: (name: string | null, reason: string) =>
    `The library could not be opened: ${name ?? "the chosen file"}. ${reason} Choose it again or reconnect the drive.`,
};
