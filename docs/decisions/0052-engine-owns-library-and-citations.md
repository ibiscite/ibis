# 0052 The engine owns the library and citations; front ends submit and show

Decided 2026-11-13 by the orchestrator under the maintainer's direction ("engine
first, front ends thin"; decision 0028), after two independent sketches (Opus and
Sol) of one brief, screened for shallow modules, leaked details, pass-through
layers, split ownership and phase-ordered modules.

## Shape

- **One engine per process** (`Engine::desktop(settings)` in the app,
  `Engine::one_shot()` in the command line) with two parts:
  - `engine.library()`: `submit(Choose | Recheck | Leave)`, `status()` and
    `watch()`. It owns the requested and current source, reads, deadlines, the
    in-flight limit, rechecks, timestamps and saving the choice. Status is a whole
    snapshot (`current`, `attempt_failed`, `choice_not_saved`), never a delta.
    A failed switch to B keeps A searchable and citable and names B as the failed
    attempt; a late completion never installs.
  - `engine.citations()`: sessions by id (a bounded registry; one for the picker,
    one per JSON-RPC or MCP connection) with `preview`, `forms`, `bibliography`
    and `commit`. A newer preview retires that session's earlier preview, its
    forms and any bibliography not yet started. A record the style cannot write
    is named in `left_out` and never costs the rest.
- **Order is reserved at submission**, synchronously, before any worker runs, so
  two windows or quick clicks keep their order without locks in the front end.
- **`commit` returns the checked content** after re-checking the shown hash; the
  app writes the clipboard on the main thread with its own paste proof.
- **One validator** in `ibis-core::citation::check`, used by syntax, CSL and
  re-citing. Precedence: request-level refusals, then per item missing, changed,
  not citable, annotation, locator, form.
- **One display contract**: `DisplayNode` gains blocks (entry, margin, indent),
  built by CSL; the app renders only that, never engine HTML.
- **One vocabulary**: the app uses core `Draft`, `Preview`, `DisplayTree` and
  `CitationForm` through generated bindings.
- **Settings usability**: `Loaded::interface()` and `Loaded::cites()`.
  The app may run on a validated backup with a notice; nothing that changes cited
  output falls back silently; the command line refuses.
  An absent file with no backup uses defaults for either read. Interface
  reads may use any validated backup, including with a newer or unreadable
  primary; the retained primary problem supplies the notice. Library, writing
  and format reads use `cites()` and refuse every primary problem. Reads never
  authorize repair or overwrite a newer or unreadable file.
  Interface reads return presentation preferences that cannot be passed to format
  selection. Ordinary edits also require `cites()` usability. A missing primary
  with a backup needs an explicit restore; neither an edit nor opening the settings file in
  an editor promotes the recovery copy. The app restarts after a restore so all
  native owners start from the restored choices.
  Persistence is private to the settings owner: callers submit edits or restore
  a previewed plan, and ordered reads cannot save or repair a file. Portable
  and device choices are separate types in the same flat local JSON file;
  backups contain only the portable type. Unknown fields stay local and survive
  edits and restores. Appearance and accent are known portable choices.

## Stays in front ends

Clipboard writes, paste proof, target capture and secure input; which system
events ask for a recheck; dialogs, windows, the tray, menus and wording; the
picker's view state (order, focus, which view is open); the editor endpoint's
transport.

## Rejected

A session object the app holds with blocking methods: order would follow lock
acquisition, not dispatch, and swapping the handle on a library change recreates
the second state machine. Letting the engine call the clipboard: the write must
run on the app's main thread.

## Order

Each slice deletes the app's copy in the same pull request:
1. display blocks (delete the app's HTML parser);
2. one validator;
3. one vocabulary;
4. citation sessions (policy leaves `engine_file.rs`);
5. settings usability, after #1849;
6. the library owner, last (deletes `library/{state,load,tasks,retry,status}.rs`).
