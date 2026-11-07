# 0040 One owner for shared settings

2026-11-07 (consolidation step 6; design attacked by Sol before code). Settings,
formats per app, the selected writing project and restoring a backup all change
through one owner per settings file, `ibis_engine::settings::Owner`.

- **The owner orders writes by when they were asked for.**
  - `submit` takes a complete edit and returns at once, never waiting for disk.
  - One worker applies edits one at a time, in that order. Each runs as the core
    transaction: the cross-process lock, a fresh read, the change, validation
    and an atomic write.
  - A later choice therefore always lands after an earlier one, and nothing
    else orders writes: no field tickets, writer mutexes or interface queues.
  - The app submits at IPC dispatch, from a synchronous command, so two windows
    and two quick clicks keep their order. It then waits for the outcome
    separately.
- **An edit that depends on an operation checks it inside its own edit**,
  because its place in the queue does not make it current. Examples: a file
  chooser answering for a project that has changed since, a library load that a
  newer one replaced, a window size from an older drag. Such an edit compares
  its identity and changes nothing when it is stale.
- **A backup restore keeps its own commit.** It compares the file with the
  settings shown in its preview, replaces them whole, and runs through the same
  owner.
- **Other processes (the command line) are ordered by the file lock.** A busy
  lock refuses at once, as before. Edits patch only the fields they mean to
  change, so a command-line change made between two app edits is kept.
- **A refused or failed save publishes nothing.** The edit's caller gets the
  refusal, and every later edit still runs.
- **A failed choice is never sent again later.** A resend could land over a
  newer choice made in the other window, so no window keeps one to retry.
  - Settings shows what is saved.
  - The picker keeps the failed choice applied until Ibis restarts, or until
    the other window saves that field after it.
  - Each `preferences:changed` names the fields its save set, so a window acts
    only on choices, never on unchanged values in a snapshot. Recent citations
    are forgotten only by a save that set that switch.
  - A choice made before the first read goes to the owner at once, which
    refuses it if the settings cannot be read.
- **Formats per app live in `settings.json`** (`app_outputs`): one output per
  system and normalised app ID, the most recently chosen last, at most 200.
  - They stay on this computer and never travel in a backup, like window places
    and paste approvals.
  - `apps.json` keeps only what is about pasting: the delivery choice (paste or
    copy only), the identity binding and the one-time plugin note (decision 0031).
  - A browser or editor's first-paste answer saves its format first, then its
    delivery. If the delivery save fails, the format stays and the answer says
    the delivery was not saved.
- **System effects stay with the app.** It registers the shortcut, shows the
  menu bar item, sizes windows and applies larger text; the owner only saves.

Supersedes the ordering parts of 0031's 2026-10-31 note ("one frontend patch
queue for ordering").
