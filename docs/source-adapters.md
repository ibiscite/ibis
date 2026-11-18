# Source adapters

A source adapter reads a reference manager Ibis has no reader of its own for,
and hands Ibis its library ([0056](decisions/0056-open-layer-for-research-material.md),
step 4). It is a separate program, so it can be written in any language and
carry its own licence ([0039](decisions/0039-extending-ibis-from-outside.md)).
EndNote is the first; Obsidian reference notes are planned next.

```sh
ibis search tides --source endnote=Invented.enl
ibis cite --style apa --source endnote=export.xml …
ibis sources check --source endnote=Invented.enl --expect fixtures/endnote/expected.json
```

## Finding an adapter

`--source NAME` or `--source NAME=LIBRARY` names an adapter and, in its own
terms, the library to read. Repeat it for several. Built-in adapters ship
with `ibis`, which serves them as `ibis __source NAME`. To enable an external
adapter, add its name and absolute executable path to `source-adapters.json`
in your Ibis config folder:

| System | Ibis config folder |
| --- | --- |
| macOS | `~/Library/Application Support/com.ibiscite.ibis/` |
| Linux | `$XDG_CONFIG_HOME/com.ibiscite.ibis/` or `~/.config/com.ibiscite.ibis/` |
| Windows | `%APPDATA%\com.ibiscite.ibis\` |

```json
{"example": "/opt/example/bin/read-library"}
```

Use one entry per adapter. Names start with a lowercase letter, followed by
up to 31 lowercase letters, digits or hyphens. On Windows, supply an
absolute `.exe` path, escaping backslashes in the JSON. Store the program in
a location you control. `ibis doctor` lists configured names and flags
invalid entries without starting them. Restart a running `ibis serve` or
`ibis mcp` to pick up configuration changes. The `endnote` name is reserved
for the built-in adapter.

The command line, `ibis serve` and `ibis mcp` read adapters today. The desktop
app does not yet: on macOS it first needs the Automation permission that lets
it ask EndNote, which is a change to the app's signed permissions.

## The protocol

Ibis starts the program with no arguments and only its three standard
streams, from the folder Ibis runs in. The environment is cleared, apart
from `SystemRoot` on Windows. It writes one request to the
program's standard input and closes it:

```json
{"ibis": 1, "method": "read", "library": "Invented.enl"}
```

`library` is what the person typed after `=`, or absent. The program writes
one answer to standard output and exits with status 0, either a read:

```json
{"ibis": 1, "read": {
  "scope": "…", "revision": "…", "keys": false,
  "records": [{"id": "2475", "type": "article-journal", "title": "Invented tides"}],
  "skipped": [{"cause": "unsupported-type", "entry": "An invented talk", "field": "ref-type"}]
}}
```

or a refusal, one of `manager-closed`, `library-needed`, `library-not-found`,
`permission-denied`, `unreadable` and `unsupported`:

```json
{"ibis": 1, "refused": "manager-closed"}
```

The JSON Schema is `source_adapter` in
[`fixtures/engine-contract.schema.json`](../fixtures/engine-contract.schema.json).
Standard error is discarded. The protocol version changes only additively;
an answer with an unknown field or another version is refused.

- **`records`** are CSL-JSON 1.0.2. Each `id` is the manager's own record id,
  unique and stable within `scope`.
- **`scope`** names the library those ids belong to. Change it whenever you
  cannot show that an id still names the same record, for example when the
  library was replaced by a copy: Ibis then retires every earlier handle.
- **`revision`** changes whenever any record changes, and only then.
- **`keys`** says whether `citation-key` holds the manager's own keys.

## What Ibis keeps

The adapter only says what its manager holds; Ibis keeps every promise it
makes for its own readers. The library id is a keyed digest of the adapter's
name and `scope`; a handle is that id and the record's `id`, never matched by
title or DOI. Each record is checked as `cite --records` checks records sent
to it: unknown fields and invalid values warn, `file` is dropped unread,
private and local links are removed, and a record without an `id` or a CSL
type is skipped. Repeated ids make every record under them unavailable.
Without `keys`, any `citation-key` is dropped and key outputs refuse with
`missing-key`. Ibis's `revision` is its own fingerprint of what it read.

An answer is bounded to 64 MiB and 200,000 records. A read has two minutes.
Ibis stops the Windows job or Unix process group when the adapter exits or
the read is refused. The live EndNote reader streams XML chunks within
the same byte and record limits. Libraries are read when opened and when
the person reloads. A read that fails, times out, or answers
outside the protocol changes nothing: the last good read stays searchable and
citable, and the library's health names the failure.

## The conformance check

`ibis sources check` rereads the libraries named with it, built-in readers and
adapters alike, and prints each check with `ok` or `FAIL` (exit 1 on any
failure, `--json` for scripts):

| Check | Holds when |
| --- | --- |
| `same-scope-on-reread` | An adapter's two further reads give the same library. |
| `stable-revision-on-reread` | Reading again gives the same revision. |
| `token-agrees-with-revision` | The adapter's `revision` changes exactly when Ibis's does. |
| `no-keys-without-key-capability` | A library that declares no keys has none. |
| `adapter-sends-valid-csl` | Ibis skipped or corrected no field the adapter sent. |
| `records-match-expected` | With `--expect FILE`, the records are exactly those CSL records, ids aside. |
| `revision-changed` | With `--changed-from REVISION`, the revision differs from an earlier run's. |

To test an adapter, load the shared invented records into an empty library of
its manager and run the check with `--expect` on their expected CSL. To test
change tracking, run it once, edit a record, then run it again with
`--changed-from` and the revision the first run printed. What Ibis does with a
malformed, late or refused answer is tested once for every adapter with a
fake adapter (`crates/ibis-cli/tests/source_adapters.rs`).

## EndNote

The built-in `endnote` adapter (`ibis-sources::endnote`) reads EndNote's
own XML in two ways, through one reader and one field mapping:

- **An open library, on macOS:** `--source endnote=NAME` reads the open library
  of that file name (or full path), or the only one open with
  `--source endnote`. It asks EndNote 21 through its scripting interface and
  never starts or changes EndNote. Record ids are EndNote's record numbers,
  which EndNote does not reuse. The scope is the library's EndNote id, its
  path and its `.Data` folder, so an edited library keeps its handles and a
  copied, restored or moved library gets new ones. The first read asks you
  to let the program running Ibis (Terminal, your editor) control EndNote;
  if you declined, allow it in System Settings › Privacy & Security ›
  Automation.
- **An export, on every system:** `--source endnote=export.xml` reads a file
  saved with EndNote's File › Export as XML. Its records are identified by what
  they say, as RIS records are, because an export cannot show that a number
  still names the same record.

The first slice maps journal articles, books and book sections: authors and
editors in order, title, year, journal, series or book title, publisher and
place, volume, issue, pages (a book's Number of Pages), edition and DOI.
Other types are skipped and named. EndNote has no citation keys: its Label
field holds whatever a person typed, so no key is claimed. Attachments, notes,
abstracts and URLs are not read. EndNote 21.3's scripting export returns its
text as if it were Mac Roman; the adapter undoes that only after one field
EndNote reads directly confirms it, and refuses the read otherwise.

The invented trial library is [`fixtures/endnote`](../fixtures/endnote/README.md).
