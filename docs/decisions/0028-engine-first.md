# 0028 Engine first (replaces 0003 and 0005)

Every capability lives in the Rust crates (`ibis-core`, `ibis-sources`,
`ibis-csl`, `ibis-engine`) as typed methods. The `ibis` command line, `ibis
serve --stdio` (JSON-RPC 2.0), `ibis mcp` and the desktop app are clients; the
app links the engine in-process. The command line exposes everything; a test
fails when two front ends give different results for the same request. Methods
take and return JSON described by `ibis schema`, in a versioned envelope with
stable error codes; an error may add a stable reason, and front ends choose
their words from it rather than guessing from the code. A wrong style or
locale is a request error, not a refusal of each item (2026-11-02, #1618).
One citation request, one result, with typed losses and
refusals instead of plausible but wrong text. Records, attachments, notes,
annotations, collections, tags and saved searches are addressed by
`library + native key`; Ibis maps Zotero's raw data to CSL itself so Zotero
open and closed give identical results. Records keep only the normalised data
Ibis uses; the source stays the original, and an export that needs the original
entry reads it from the source (2026-10-27: in-memory raw copies dropped, which
nothing read and which doubled memory). Details: [architecture](../architecture.md).

2026-11-02 (maintainer): the engine is the soul of Ibis. It stands on its own and
is developed in tandem with the app, with no compromises; the GUI is only a layer.
It must work with AI agents, run on every operating system and be there for anyone
to build on.

2026-11-04 (maintainer, planning only): correctness across the app, command
line and agents is the priority. Three rules apply to that shared contract:

- **Separate task contexts.** Saved project definitions and reference updates
  can be shared; each writing task keeps its selected project, source scope
  and output. Another client's project selection never redirects ongoing work.
  An explicit “use the current Ibis project” captures that choice when the task
  starts, rather than continuously following the picker.
- **Refuse conflicting project keys.** When a selected project's bibliography
  is known to bind a key to a different work, refuse that key output until the
  conflict is resolved. This replaces warn-and-copy for that case; copying
  instead of pasting does not make the conflicting key safe.
- **Keep a last-good snapshot usable.** An ordinary refresh failure permits
  continued citation from a coherent snapshot of the same established source,
  with a clear warning and the last successful read time. No repeated
  confirmation is needed. This does not override refusals for uncertain source
  identity, changed selected records or conflicting keys. The same request and
  explicit policy have the same outcome across clients; strict mode remains
  the explicit option planned in #1538.

These are planned rules, not a claim that implementation has landed. They do
not choose a shared engine process. Context and source ownership are described
in [architecture](../architecture.md#task-contexts-planned); visible behavior is
in [design](../design/sources-and-sheets.md#source-checks-and-passage-reuse-planned). Follow-up belongs
to #1697, #1260 and #1538, with the work order on #1300 unchanged.

2026-11-05 (maintainer): an unreadable or uncertain project bibliography is a
warning, not a gate. The citation stays available and says the file couldn't
be checked; only a key known to name a different work is refused. The
2026-11-04 policy of last-good bibliography copies and one-off unverified
actions is withdrawn.

2026-11-05 (maintainer): a writing project is a `.bib` file and its format,
nothing more. The 2026-11-04 rules for sharing project edits across apps and
agents (Apply Updates, retained settings after a deletion, conflicting saves)
and for optional project search defaults are withdrawn: no one has met those
cases yet, and each added words and controls to the picker. They return only
when people need them. Separate task contexts, the refusal of conflicting
project keys and the last-good source snapshot above still apply.

2026-11-05 (maintainer): the 2026-11-04 plan for historical citations (Copy
Saved Text, Recreate the Earlier Citation, Create Current Citation) is
withdrawn until people ask for it. Receipts (#1538) and exporting what a
document cites (0.6) remain.

2026-11-10 (#1538, the maintainer's issue): **compatibility rules for the engine
contract.**

- **Three versions stay separate.** The package version (0.3.2) names a build.
  The contract version, the envelope's `ibis` field (now 1), names the wire
  contract. The formatter, style and locale releases say what printed text a
  style gives; receipts report them. None stands in for another.
- **A client chooses the contract explicitly** with `--contract 1` on any
  command, `serve` and `mcp` included; a version Ibis does not speak is refused
  with the list it does. There is one version and no negotiation. `ibis schema`
  gives each method's contract version and `doctor` reports it.
- **Requests stay strict.** A field no method reads is `bad-input`, for every
  method, never ignored.
- **Responses are forward-compatible.** A client accepts fields it does not
  know. A loss, refusal, outcome, notice, warning or finding it does not know
  reads as needing review, never as success. The Rust contract types do this
  with an `Unknown` value that the schema does not list and the engine never
  sends. A failure is a failure whatever its code; the Rust types still refuse
  an error code or reason they don't know.
- **Additive means only this:** a new optional field in a result, a new code in
  an existing list, a new method, or a new optional request field whose absence
  keeps the old answer.
- **Anything else is a new contract version:** removing or renaming a field or
  code, changing a field's type or meaning, changing the envelope, or changing
  what the same request means for the same library (which records it cites, its
  outcomes and losses, or the output it asks for).
- **Old clients keep working.** The golden cases in `fixtures/cases` are
  contract 1 requests with their answers, and they replay through every front
  end with the same citations.
