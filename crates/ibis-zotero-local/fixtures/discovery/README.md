# Zotero directory discovery fixtures

These files are synthetic preferences, not exports from a real profile. The
integration tests place them under injected macOS, Windows and Linux profile
roots, including a supplied Windows roaming AppData directory.

The reader follows Zotero's [documented profile roots](https://www.zotero.org/support/kb/profile_directory),
[data defaults](https://www.zotero.org/support/zotero_data) and
[directory preference precedence](https://github.com/zotero/zotero/blob/main/chrome/content/zotero/xpcom/dataDirectory.js).
Both `useDataDir` and `dataDir` are required for a custom location. `lastDataDir`
is an obsolete descriptor migration marker and does not override `dataDir`.

The parser accepts `user_pref` scalar statements, quoted strings, comments,
generated backslash/quote/newline escapes, and hexadecimal/UTF-16 escapes.
It executes nothing. Unsupported or incomplete configuration yields fixed
warnings rather than silently switching libraries. Roots and candidates redact
their Debug output; callers must also keep explicit path access out of logs.

Discovery returns lexical candidates without checking existence or opening any
database. It reads at most a 64 KiB registry and 64 preferences files of 1 MiB
each, rejects nonregular configuration files and deduplicates identical paths.
An absolute manual override bypasses configuration reads. Legacy profile-local
data, persistent descriptors and interrupted data migrations require an explicit
manual choice; the offline snapshot reader must validate candidate availability.

No personal profile, database, network connection or write to Zotero is needed
to run `cargo test -p ibis-zotero-local --test discovery`.
