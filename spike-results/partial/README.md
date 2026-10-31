# Unfinished spike work (#1534)

The project paused on 2026-10-31 while these jobs were running. Nothing here was finished,
reviewed or measured to the stage A standard, and none of it should be applied as it stands.
The patches are parked so the next session can resume them; each applies to ea29d0be.

- `stage-b-load-speedup.patch`: first single-thread changes to search preparation, plus their
  harness. No after-measurement exists. Results for the original code are in
  `../stage-b-baseline.json` (invalid, heavy load) and `../stage-b-profile.json`.
- `stage-c-memory.patch`: allocation-tracking harness only. It produced no results.
- `stage-d-closed-zotero.patch`: table-limited `quick_check` and copy prototype. **Unreviewed, in a
  risky area, and with no damage matrix.** Do not merge without corruption tests and independent
  review.

`../stage-e-real-zotero/` holds the real Zotero runs and the scripts that produced them, and those
measurements did complete. `user.js` shows the throwaway profile's preferences.
