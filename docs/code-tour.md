# Code tour

Follow one action through the code, then use the tests beside that action.
Paths below describe the current desktop picker and CLI. The broader design in
[architecture.md](architecture.md) also includes planned integrations. Zotero
import, citeproc styles and direct insertion into other applications are future
work; today's picker searches a `.bib` library and copies citation keys.

## Words used here

- **Tauri**: the framework that turns the web interface (`apps/desktop/src`) and the Rust code (`apps/desktop/src-tauri`) into one desktop app.
- **CSL / CSL-JSON**: the Citation Style Language and its JSON data format, the standard way to describe references for citation tools.
- **Coalesce**: merge a burst of file-change events into one reload.
- **Quiet interval**: a short pause (250 ms, `SETTLE` in `source.rs`) with no new file events; the file is read again after it to make sure it stopped changing.
- **Candidate load**: a freshly read library that has not replaced the current one yet.
- **Generation**: a request counter that goes up each time a library is opened or reloaded, so a slow, older load can never replace a newer one.
- **Replay / logical quotas**: cached work is reused (replayed), but still counts against the same per-field limits as if it were done again.

## One search: shortcut to results

1. [Desktop main.rs](../apps/desktop/src-tauri/src/main.rs) builds the Tauri app,
   loads settings and registers the global shortcut. Its pressed-event handler
   calls `toggle_picker` in [picker.rs](../apps/desktop/src-tauri/src/picker.rs).
2. `toggle_picker` calls `show_picker` when the window needs to appear.
   `show_picker` emits `picker:shown` after showing and focusing the window.
3. [api.ts](../apps/desktop/src/api.ts) listens through `onPickerShown`.
   [App.tsx](../apps/desktop/src/App.tsx) focuses the search input and selects its
   previous text, so typing replaces the old query.
4. Typing updates React's `query`. The search effect calls `api.search`, which
   calls the generated `commands.search` function in
   [bindings.ts](../apps/desktop/src/bindings.ts).
5. The Rust command `library::search` in
   [library.rs](../apps/desktop/src-tauri/src/library.rs) reads the installed
   library and calls `OwnedIndex::search` in
   [core search.rs](../crates/ibis-core/src/search.rs).
6. Core search normalizes case and accents, matches query terms and ranks the
   results. Rust returns record identities, display fields and matched fields;
   React displays them. The UI requests 20 results; Rust caps the limit at 200.

The search effect's `current` flag ignores replies for superseded queries.
`resultsQuery` records which query produced the visible results and also guards
copying. Initial library status and `library:changed` events have a separate
ordering problem: follow their state updates when debugging startup (#91).

The command bridge has one registry:
[Rust bindings.rs](../apps/desktop/src-tauri/src/bindings.rs). It registers commands
implemented in `library.rs`, `copy.rs` and `picker.rs`, and exports the TypeScript
contract. `api.ts` adds UI defaults, labels and event listeners. When changing a
command, start with its Rust signature and registry; regenerate the contract
with the explicit `bindings::tests::export_contract` test described in that file.

## One copy: selection to clipboard

1. `App.tsx`'s `copy` handler chooses selected records in selection order, or the
   highlighted result if it belongs to the current query.
2. `api.copyCitation` crosses the generated bridge to `copy_citation` in
   [copy.rs](../apps/desktop/src-tauri/src/copy.rs).
3. `render_citation` resolves each `RecordId` against the installed references.
   Identity is **source plus native key**; identical keys in different libraries
   must not select the same record. A missing selection fails before writing.
4. [core cite.rs](../crates/ibis-core/src/cite.rs)'s `render` validates keys and
   applies the chosen Pandoc, LaTeX, BibLaTeX or Typst syntax. Follow that format's
   escaping helper when output is wrong.
5. The desktop command writes the rendered text with the clipboard plugin, then
   remembers the format in settings. It returns text and warnings to React.
   React hides the picker after a successful copy with no warnings; otherwise
   it keeps the result or error visible. Every dismissal (copy, Escape, shortcut,
   blur or close) uses `picker::dismiss_picker`: it hides the window and, on
   macOS, the application, returning focus for the next paste. On macOS the
   picker is a non-activating NSPanel; summoning unhides Ibis without activating
   it and makes the panel key. Library dialogs explicitly activate Ibis while
   `DialogOpen` protects the picker. Windows/Linux rely on the
   window manager's focus behavior when the window hides.

## Opening a library: file to references

At startup, `main.rs` chooses the `IBIS_LIBRARY` override or saved library path
and calls `start_open`. The Choose library button takes this path:
`api.chooseLibrary` → Rust `choose_library` → `choose_library_file` → native
dialog callback → `start_open(path, true)`. These Rust functions are in
[library.rs](../apps/desktop/src-tauri/src/library.rs). `DialogOpen` prevents
duplicate dialogs and keeps the picker open while its own dialog has focus.

`start_open` numbers the request synchronously and loads in a background task.
Choosing the already installed path is a reload; choosing a different path is
an initial load. The parsing path is:

| Stage | File and function | What to inspect |
| --- | --- | --- |
| Read | [source.rs](../crates/ibis-core/src/source.rs): `load_library_file`, `read_library_file` | Regular file, UTF-8 and 64 MiB bounded read; reload rejection rules. |
| Split | [lexer.rs](../crates/ibis-core/src/bibtex/lexer.rs): `split_blocks`, `parse_entry`, `parse_string` | Entry boundaries, field parts, line numbers and macros. |
| Resolve | [resolve.rs](../crates/ibis-core/src/bibtex/resolve.rs): `Resolver::resolve` | Macro expansion, inherited fields, precedence, structural limits and caches. |
| Parse fields | [bibtex/mod.rs](../crates/ibis-core/src/bibtex/mod.rs): `load_entry` | Resolved text passed to `biblatex::Bibliography::parse`; per-entry panic containment. |
| Convert | [convert.rs](../crates/ibis-core/src/bibtex/convert.rs): `convert` | Names, dates, protected text and CSL-like normalized fields. |
| Index | [desktop library.rs](../apps/desktop/src-tauri/src/library.rs): `candidate`, `install` | `OwnedIndex`, warnings, request generation and installed state. |

[bibtex/mod.rs](../crates/ibis-core/src/bibtex/mod.rs)'s `load_with_budget`
coordinates parsing and returns a [LoadResult](../crates/ibis-core/src/model.rs):
completed `Reference` values and `LoadWarning` values with kind, line and key.
Keep three representations distinct: original entry text, resolved text sent to
the field parser, and the normalized reference. `Reference.raw` keeps the original
entry; a normalized or rendered value is not a replacement for it.

One load shares a work budget. Expanded macros and clean parent snapshots have
separate bounded caches; replay still pays for actual work and preserves the
logical quotas. The current limits are documented in
[architecture.md](architecture.md#sources). Exhaustion retains completed entries
with a warning; a limited reload returns an error so it cannot replace the index.

## Reloading: who keeps the last good library?

`watch_current` in desktop `library.rs` constructs
[SourceWatch](../crates/ibis-core/src/source.rs). It watches containing directories
so editor save-by-rename works, and also manages symlink target watches.
The bounded worker coalesces events and calls `stable_read`: two identical
bounded reads separated by a quiet interval produce a candidate load.

Startup and manual `load_library_file` calls read once; only the watcher uses
`stable_read`. Reload parsing rejects demonstrably unfinished entries and work
limit exhaustion. A quiet interval cannot prove a writer has finished at an
otherwise valid prefix.

The **desktop** owns the last good snapshot. `install` compares the request with
`LoadRequests`, discards outdated work, replaces the index on success, and records
an error while retaining the index on failure. Core returns candidates or errors;
it does not store an installed library. A valid empty file intentionally installs
an empty index. `publish` sends `library:changed`; the React listener updates
status and increments `libraryVersion` to rerun the current search.

## Settings: location and writes

`main.rs` obtains Tauri's `app_config_dir` and appends `settings.json`.
[core settings.rs](../crates/ibis-core/src/settings.rs) defines `schema_version`,
`shortcut`, `library` and `copy_format`. `load` reports problems and can recover
from backup or defaults. `save` uses a temporary file, rename and backup, and
refuses to overwrite a newer schema.

[preferences.rs](../apps/desktop/src-tauri/src/preferences.rs)'s
`SettingsFile::update` serializes the complete read-change-save operation, so
intervening edits are preserved. A successful remembered library choice saves its
path; a successful clipboard write saves the format. Shortcut configuration is
read at startup. Follow these callers before changing the settings format; its
versioning is separate from picker UI behavior.

## Debugging: choose the smallest relevant test

Run commands from the repository root. Setup and the complete check are in
[development.md](development.md); testing boundaries are in [testing.md](testing.md).

| Area | First check |
| --- | --- |
| Parsed names, text and fields | `cargo test -p ibis-core --test import_fidelity` |
| Recovery and hostile dates | `cargo test -p ibis-core --test bibtex_recovery --test bibtex_robustness` |
| Work limits and caches | `cargo test -p ibis-core --test bibtex_load_budget --test bibtex_macro_cache` |
| Directory watching | `cargo test -p ibis-core --test source_watch` |
| Search, citation syntax, settings | `cargo test -p ibis-core search::tests`, `cargo test -p ibis-core --test cite_outputs`, `cargo test -p ibis-core settings::tests` |
| Installed-library ownership | `cargo test -p ibis-desktop library::tests` |
| Rust/TypeScript contract | `cargo test -p ibis-desktop generated_contract_is_current` |
| UI state and keyboard behavior | `pnpm -C apps/desktop test` |

These repository fixtures are synthetic:

```sh
cargo run -p ibis -- search sleep --bib fixtures/bibtex/real-world.bib --json
cargo run -p ibis -- cite mueller2021 --bib fixtures/bibtex/real-world.bib --format pandoc
cargo run -p ibis -- check fixtures/bibtex/oversized-range-year.bib --json
cargo run -p ibis -- csl --all --bib fixtures/bibtex/oversized-range-year.bib
```

`check` exits 2 when it finds problems; 1 means a command error. Inspect warning
kind, line and key alongside the retained reference. The CLI uses
`source::load_library_file(path, false)` in
[CLI main.rs](../crates/ibis-cli/src/main.rs), sharing the desktop's bounded reader
and allowlisted file errors. CLI reproductions exercise import/search/citation output. Watch delivery,
desktop installation and native focus/clipboard behavior need their own checks.

## Three examples from past fixes

**Literal backslashes disappear (#90).** Start with `protect_backslashes` and
`render_field` in `resolve.rs`, then `clean` in `convert.rs`. They protect literal
text before upstream parsing and restore it after conversion. The backslash and
private-use collision cases in
[import_fidelity.rs](../crates/ibis-core/tests/import_fidelity.rs) cover titles,
plain fields, inheritance and unknown commands. Check both raw and normalized
values before changing target-specific citation escaping.

**An issue says CSL export crashed (#111).** Read
[csl_export.rs](../fuzz/fuzz_targets/csl_export.rs): it imports the input before
`library::csl_items` → `Reference::to_csl_json` → JSON serialization. Find the first
failing stage. The oversized uncertain-year fix is in `convert::date_value` and
`date_parser_overflows`, with a regression in
[bibtex_robustness.rs](../crates/ibis-core/tests/bibtex_robustness.rs). Use the fuzz
issue's exact lossless-base64 input and target; “deadly signal” names a failure
signature, not the Rust function that failed.

**A reload loses or restores the wrong records (#94).** Trace directory events
→ `stable_read` → candidate → generation-checked `install`. Compare
[source_watch.rs](../crates/ibis-core/tests/source_watch.rs) with the desktop
`library::tests`: the former checks file observations; the latter checks which
candidate becomes current. Include deletion/recreation, rename replacement and
symlink retargeting. The current tests cover recovery after retargeting a symlink
to a missing file; historical issue limitations may describe older behavior.

Unit tests and CLI probes work without a desktop. Global shortcut conflicts,
focus/blur around the chooser, actual paste, platform file notifications and
NVDA behavior still require native checks with synthetic data.
