// User-facing library text stays together for future translation.
export const libraryMessages = {
  openError: (name: string | null, reason: string) =>
    `The library could not be opened: ${name ?? "the chosen file"}. ${reason} Choose it again or reconnect the drive.`,
};
