# Foundation boundary audit

Tested `cc1125ad792e1a7861eca1b6fb8063640d00eaed`: HEAD equals the available
`origin/main`. Fetch failed against read-only shared Git metadata; upstream
freshness and the tracking issue could not be verified offline. Invented data
only. No production changes, fixes, commits, installs or shared settings changes.

**15 failing regressions; no demonstrated P1.** File aliases below identify each
test exactly. Output excerpts are from actual runs. Related surfaces are search
results requiring the same invariant, not claims of additional proven defects.

| Alias | Test file |
| --- | --- |
| E | [crates/ibis-engine/tests/break_foundation.rs](crates/ibis-engine/tests/break_foundation.rs) |
| S | [crates/ibis-sources/tests/break_foundation.rs](crates/ibis-sources/tests/break_foundation.rs) |
| R | [crates/ibis-engine/src/server.rs](crates/ibis-engine/src/server.rs), module `tests` |
| M | [crates/ibis-engine/src/mcp/cancellation_tests.rs](crates/ibis-engine/src/mcp/cancellation_tests.rs) |
| Z | [crates/ibis-zotero-local/src/closed/database_tests.rs](crates/ibis-zotero-local/src/closed/database_tests.rs) |
| C | [crates/ibis-cli/tests/break_foundation.rs](crates/ibis-cli/tests/break_foundation.rs) |

1. **P2 — Writing outside the lock domain.** E:
   `retargeted_settings_symlink_cannot_escape_the_locked_file`.
   Output: `success=true, unlocked second target overwritten=true`.
   Another writer holds the second target's lock. The transaction nevertheless
   overwrites it after the selected symlink changes between identical files.
   Class: identity/TOCTOU. Settings locks one resolved path, fingerprints only
   bytes, then resolves the path again for writing. Related: settings backup
   restore, source identity-key I/O, file snapshots, desktop `destination.rs`,
   recents through backup saves. Fix: a pinned `SettingsFile` transaction owns
   target identity, lock and replacement; delete independent path resolutions
   and byte-only conflict identity.

2. **P2 — Ambiguous settings change consent.** E:
   `duplicate_settings_consent_is_rejected` and
   `flattened_unknown_settings_cannot_override_consent`.
   Outputs: `ambiguous consent accepted: update_checks=Some(true)`;
   `save silently changed consent to Some(true)`.
   Class: ambiguous representation. Parsing into `Value` destroys duplicate-key
   evidence; a public flattened extension map can serialize a second known key.
   Related: settings backup parsing, Zotero `live.rs`/`bbt.rs`, desktop
   `destination.rs`; RPC and CSL already have separate unique-member readers.
   Fix: one unique-object decoder and a collision-free settings encoder;
   delete duplicate readers and unrestricted flattened extension insertion.

3. **P2 — Backpressure disables cancellation.** M:
   `break_foundation_blocked_stdout_does_not_block_cancellation`; R:
   `break_foundation_full_reply_queue_does_not_block_cancellation`.
   Outputs: `blocked stdout held the transport lock and prevented cancellation`;
   `full reply queue blocked the only cancellation reader`.
   Class: control/data ownership coupling. MCP writes while holding request
   ownership; RPC's reader blocks sending replies. Both controls observe
   cancellation after output resumes. Related: library-change forwarding,
   MCP tool publication, native `editor_endpoint.rs` response/disconnect workers.
   Fix: one transport primitive with nonblocking bounded enqueue, interruptible
   writes and independent request control; delete reader-side blocking sends
   and writer access under the cancellation lock.

4. **P2 — Cancellation lies after commit.** R:
   `break_foundation_cancellation_after_commit_cannot_report_rollback`.
   Output: `committed operation reported as: {"error":{"code":-32800,...}}`.
   Class: missing irreversible-operation state. A deterministic dispatcher seam
   commits, receives cancellation and returns success; the wrapper discards it.
   Related: engine `dispatch`, `service/sources.rs` checks after publication,
   desktop `library/engine_file.rs` acceptance checks.
   Fix: a committed outcome returned by the mutation owner; delete blanket
   post-success cancellation checks. Cancellation must retire pending work,
   not erase an acknowledged effect.

5. **P2 — Settings stop watching after parent replacement.** E:
   `settings_watch_survives_renaming_its_parent_directory`.
   Output: `watch passed the initial edit, then lost all edits after parent rename: Err(Timeout)`.
   Class: duplicated watch ownership. The kernel follows the old inode;
   settings `rebind` only notices changed path spellings. Initial watching and
   the rename notification succeed. Related: `sources/file/watch.rs` already
   handles ancestor anchors and rearming; library and symlink-target watches
   need the same invariant. Fix: extract one path-watch owner; delete settings'
   separate rebind/event machinery. This is a Linux reproduction, not a
   sandbox-unavailable watcher.

6. **P2 — An invalid contender defeats duplicate-ID quarantine.** S:
   `csl_duplicate_id_is_quarantined_even_when_one_type_is_invalid`.
   Output: `duplicate id still resolves to 1 record(s)`.
   Class: identity registration after filtering. Type validation returns before
   reserving the second ID, leaving the first reference selectable. Related:
   BibTeX header deduplication, exported-record builders, Zotero live-row
   filtering and `Snapshot::new`, which sees only accepted objects.
   Fix: an identity-claim index before payload validation, quarantining every
   contender; delete adapter-specific accepted-record-only deduplication.

7. **P2 — Orphaned Zotero records disappear cleanly.** Z:
   `break_foundation_orphaned_library_is_not_silently_discarded`.
   Output: `orphaned bibliographic item disappeared in a successful snapshot: Ok((1, []))`.
   Class: absence conflated with deliberate exclusion. Missing selected-library
   membership uses the same skip path as an excluded feed. Related: closed
   field/creator joins, live child filtering, source attachment relationships,
   core snapshot relationships. Fix: construct `Included/Excluded/Invalid`
   membership against all library identities; delete the ambiguous
   `!contains_key => skip` path.

8. **P3 — Settings can save data they cannot reload.** E:
   `settings_save_cannot_create_a_newer_schema_than_it_can_read`;
   `settings_save_does_not_acknowledge_an_unreadable_nan_position`.
   Outputs: `Some(NewerVersion(3))`;
   `Some(Invalid("invalid type: null, expected f64"))` after successful saves.
   Class: invalid representable state. Public version and floating-point fields
   bypass reader invariants; serde serializes NaN as null. These use constructed
   Rust values, not JSON-encoded NaN. Related: Owner transactions, backup plans,
   desktop sizing/persistence. Fix: private validated settings, finite normalized
   positions and a serializer-owned version; delete caller-owned schema state
   and divergent read/write validation.

9. **P3 — NUL silently changes words across front ends.** S:
   `nul_in_csl_title_is_not_accepted_as_clean_citation_text`; E:
   `nul_cannot_merge_words_in_the_mcp_title`.
   Outputs: `NUL reached a citation field without a source warning`;
   MCP title `Inventedhidden`, with source health `ok`.
   Class: competing text semantics. CSL accepts any string; MCP removes NUL
   while formatter normalization replaces it with a space. Related: BibTeX
   `convert::clean`, RIS/CSV/Zotero mapping, core model/summary/citable fields,
   CSL input/output, CLI `terminal_display`, BibTeX export.
   Fix: one validated inline-text boundary recording semantic loss; delete
   competing field cleanup while retaining destination-specific escaping.

10. **P3 — Help reports success without flushing.** C:
    `help_flush_failure_cannot_exit_successfully`.
    Output: `left: 0`, `right: 70` for an invented `StorageFull` flush failure.
    Class: duplicated stream completion. Clap help returns after writing;
    ordinary output uses the flushing emitter and passes the control test.
    Related: version/help and terminal greeting branches, stderr diagnostics.
    Fix: one output-completion primitive; delete early-return write paths.

11. **P3 — A stale writer erases a newer preference.** E:
    `two_settings_writers_cannot_silently_overwrite_a_newer_choice`.
    Output: `second writer silently erased first writer's paste=false`.
    Class: ownership bypass. `save` locks after the caller's read; its snapshot
    cannot detect stale memory. Search found current `settings::save` callers
    only in tests: this proves a public API defect, not a current desktop flow.
    Related: recents persistence, desktop app-memory writes and revisioned backup
    plans. Fix: make Owner transactions the only mutation API; delete public
    whole-snapshot saves and convert fixture initialization to transactions.

All five regression commands exited **101**. Full outputs are in
`target/foundation-evidence/{sources,settings,protocols,zotero,cli}.log`.
Run with local `CARGO_TARGET_DIR`/`TMPDIR`, `--offline --locked`:
`cargo test -p {ibis-sources,ibis-engine,ibis} --test break_foundation` separately;
`cargo test -p {ibis-engine,ibis-zotero-local} --lib break_foundation` separately.

Passing controls cover braces/macros/large fields, BOM/CRLF/mixed encodings,
deep JSON, array limits, wrong types, non-UTF-8/long paths, framing limits,
replacement/deletion/recreation, symlinks, debounce bursts, queues, stale
replies, Zotero locks/mid-copy startup/newer or missing schemas, and CLI
locale/timezone/no-TTY output. Real disk exhaustion, signals, wall-clock jumps,
many processes, socket transports and macOS/Windows remain unverified.

The three causes behind the most breaks:

1. **Invalid states remain ordinary values:** maps, strings, floats and IDs
   lack one validating construction boundary.
2. **Ownership is split:** file identity, watching, persistence and stream
   completion each have competing paths.
3. **Lifecycle facts are booleans or timing:** cancellation, commit and
   backpressure lack explicit ownership and irreversible transitions.
