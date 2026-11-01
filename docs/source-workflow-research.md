# Source workflows: first versions and research

Planning research, 2026-10-30; not implemented or a compatibility claim. This
refines four of the [five approved directions](../ROADMAP.md#saved-passages-and-project-checks),
following #1521. The interaction contract remains in
[design.md](design.md#source-checks-and-passage-reuse-planned); the behavioral
bounds below keep that work small. New presentation/keyboard proposals are marked
as such and require a canonical design update before implementation. No additional
manager adapter, source write, network use or MCP permission is approved here.
All five ship before 1.0 (maintainer, 2026-11-01); each starts with the smallest
slice that is reliable.

## Order and shared model

Finish the existing 0.3/source-storage work first. Project-bibliography checking
(#1377) remains first in the approved order. For these four features:

| User-value priority | Feature | Why this boundary |
| --- | --- | --- |
| 1 | Find/open saved PDF highlights and comments (#1208, #54, #359) | Prove source identity, text meaning and locations |
| 2 | Copy one quote with its reference (#1385), completing the same first slice | A search-only demo is insufficient; prove reusable output before batch export |
| 3 | Selected-evidence Markdown brief (#1517) | Recognizable workflow, but demand beyond existing exports is unproven; adds a second selection/file lifecycle |
| 4 | Changed copied-key warning (#1092) | Real narrower pain; durable identity/history adds complexity and ordinary .bib renames are not covered |

Prioritize project-bibliography checking and the complete find/open/copy passage
loop. Qualify their smallest useful paths; broad version/reader/annotation coverage
must not become a hidden release obligation. Notes and other kinds follow within
#54 as honest search/open results. The brief (0.5) and remembered-key warning (0.6) follow and ship before 1.0
(maintainer, 2026-11-01); their first-version contracts are below.
Other release scope and quality gates remain unchanged.

The shared passage block contains eligible quotation text, separately selected
comment, bibliographic reference, citation location and labelled source links.
Search, preview, copy and brief must preserve the same distinctions. The engine
owns source interpretation and formatting; clients do not reconstruct them.

## Find and open a saved passage

Search saved Zotero highlights, comments and notes in the existing picker, not
full PDF text. Show directly matching saved content with its reference as context.
A highlight whose comment also matches is one annotation result, with the
matching field indicated. Do not expose every child annotation by default or
treat unavailable coverage as zero matches.

**Presentation proposal, not a settled contract:** compare explicit
References / Passages scopes with the canonical design's passages unfolded under
a reference row. Preserve the query if scopes are tested. Likewise, Enter opening
a passage is a proposal, not a new default imposed by this research. Resolve
these choices in a comprehension walkthrough and update design.md before UI
implementation; the current design remains the baseline. Do not add a mode or
setting merely because it was proposed here.

Whichever presentation is chosen, the footer names Enter's actual action;
unavailable targets remain unavailable. Opening, Copy quote and deliberate
reference/page selection are distinct actions. Matching a comment must not
silently initiate quotation paste or change the citation being prepared.

| Saved object | What the person sees and can do |
| --- | --- |
| Highlight/underline with text and known reference | Passage, separate comment, reference and location; open, copy quote, or explicitly use reference/page |
| Sticky note, text box, comment-only, image or ink | Type and comment; no text quotation; open and use a known reference/page |
| Child note | Matching note and parent reference as context; open note, or explicitly use reference; no automatic quote |
| Standalone/multi-source note | Note; no guessed source; open note |
| Annotation on a standalone attachment | Passage/comment and attachment; no linked reference; open, without invented citation |

Call a field **Comment**, not “Your comment”, unless ownership is established.
Shared-library comments may belong to someone else; a note's parent is not proof
that every sentence is quoted from that reference. Source kinds and optional
parent relationships must survive adapter mapping.

Opening actions name the actual target: **Open annotation in Zotero**,
**Open PDF at page**, or **Open reference**. A failed annotation action offers the
other actions explicitly; it never silently opens a different target. A page-only
link does not promise annotation selection. Qualify the final observable target,
not merely successful OS launch, per Zotero/reader/platform combination.
Also qualify that the URI handler reaches the same profile/library that Ibis
read. Separate Zotero instances can have separate data and API ports; exact
profile routing is a qualification risk, not a demonstrated bug. If continuity
cannot be established, explain that limitation; do not add a profile manager.
[Zotero multiple-instance guidance](https://www.zotero.org/support/kb/multiple_instances).

Keep physical navigation and citation pages separate: “PDF page 8 · cite p. 417”.
Roman labels remain labels; never pass them as numeric PDF indices. Unknown
printed pages say “Citation page unknown”. Missing, not-downloaded, placeholder,
unreadable and unknown attachment states explain opening availability separately
from stored text availability. No implicit download, PDF import or OCR.

### Evidence and actual prerequisite

Zotero 10 already provides annotation-level advanced search. Its users previously
reported annotation rows overwhelming reference results, and Zotero adjusted the
default. The opportunity is access while writing with a clear result scope, not
novel annotation search. [Zotero 10](https://www.zotero.org/blog/zotero-10/),
[firsthand discussion](https://forums.zotero.org/discussion/129420/annotations-in-the-items-list).

Exact opening remains fallible: users report page-label/external-reader problems,
and page arrival does not necessarily select the annotation.
[Page-label report](https://forums.zotero.org/discussion/comment/504358/),
[annotation-selection request](https://github.com/zotero/zotero/issues/4799).

Current Ibis live and closed Zotero readers discard child objects; source
capabilities advertise references/keys. The annotation model also lacks kind and
authorship distinctions. #1208 must map and qualify these before picker work.
Zotero's annotations are separate from embedded PDF annotations: seeing a highlight
in another reader does not prove Ibis has a corresponding stored object.
[Zotero annotation source](https://raw.githubusercontent.com/zotero/zotero/master/chrome/content/zotero/xpcom/annotations.js),
[storage explanation](https://www.zotero.org/support/kb/annotations_in_database).

## Copy one quote with its reference

An explicit **Copy quote with reference** action previews one whole saved text
highlight/underline. It copies only; it does not inherit Enter's paste action.
Multi-passage output belongs to the brief. A note, text box or comment is never
promoted to a quotation just because its text is nonempty.

The preview identifies **Plain text** or **Markdown**, and the standalone
reference style. Reuse the existing Ibis-selected CSL style when applicable;
otherwise show the existing APA default explicitly. If a chosen style cannot
produce a standalone entry, explain and offer an explicit suitable choice rather
than silently changing styles. Do not derive quote output
from Zotero Quick Copy preferences, the active key syntax or the destination app.
Use a full standalone reference entry, with **Location: p. 417** separately:
the reference's publication page range is not the quotation's pinpoint. Existing
document-context limits still apply; no invented numbering, ibid. or live fields.
There is no new persistent quote preset in this slice.

Both modes initially publish text only; Markdown is Markdown source. There is no
alternate HTML payload for the recipient to interpret differently. Include only
the chosen quotation and explicitly included comment, visibly separated. Preserve
paragraphs, punctuation, Unicode and logical text order. No silent trimming,
dehyphenation, disjoint-passage joining, summary, translation or added ellipses.

Zotero text/comments can contain bounded inline italics, bold, subscript and
superscript markup. Normalize the qualified source forms into existing typed
styled runs, retaining source fingerprints; neither escape all tags as source
words nor strip them blindly. Malformed/ambiguous markup or meaning that the
chosen output cannot preserve produces a specific refusal before copying. This
does not authorize arbitrary note HTML or a general rich-text editor.
[Upstream transformation](https://raw.githubusercontent.com/zotero/zotero/master/chrome/content/zotero/xpcom/editorInstance.js).

The output is saved annotation text, which may have been edited, not verification
against today's PDF. A missing local PDF alone need not prevent copying coherent
stored text. Unknown citation pages need an explicit validated engine path:
today's annotation validator requires a matching page label. Preserve every
other identity/fingerprint check; never bypass the validator or substitute the
physical PDF page.

Clipboard failure keeps the selection/preview for retry and reports failure;
it never triggers paste or claims success. Native clipboard publication can
already have changed the clipboard before failing, so do not promise restoration.

Zotero already copies annotations with citations, but users find its shortcut and
Quick Copy settings confusing. Zotero 9 and EndNote also insert quotations with
live Word citations. The narrower Ibis benefit is an explicit portable output;
static text must not suggest that the document bibliography will update.
[Copy-setting confusion](https://forums.zotero.org/discussion/comment/512559/),
[Zotero 9](https://www.zotero.org/blog/zotero-9/),
[EndNote Quote & Cite](https://docs.endnote.com/docs/endnote/2025/v1/macos/en/content/09word/inserting_quotations_from_pdfs.htm).

## Save selected evidence as one file

Planned for 0.5, after the quote action. Zotero can already build a note from
annotations across papers and Readwise exports by tag, so the brief must stay a
small, fixed-layout export of chosen passages, never an organiser.
[Zotero's cross-paper note workflow](https://www.zotero.org/support/pdf_reader#adding_annotations_to_notes),
[Readwise tag export](https://docs.readwise.io/reader/docs/faqs/exporting).

Select passages/comments in order, optionally enter a question, inspect the whole
preview, then save a **new .md file**. The question is literal context text, not
an AI request. Use the same passage block as quote copying and one fixed layout.
An explicitly opened temporary brief selection is separate from the citation
shelf; adding evidence must not change the citation being prepared.
Repeat the reference for each passage initially; two passages from one paper keep
their own locations. Do not group, deduplicate by DOI/text, or reorder selections.
Comment-only entries remain labelled Comment. Arbitrary notes stay outside this
first brief; their text may combine sources.

No templates, YAML, citation-key dependencies, separate bibliography assembly,
embedded images, additional formats, persistent notebook or publishing service.
The file remains understandable without Ibis/Zotero. Separate public identifier
links from local Zotero links labelled “matching library required”; neither
promises PDF access. Add no source/attachment paths, unrelated fields or automatic
attachment copies. Deliberately selected text is preserved, not secretly redacted.
Escape literal question/comment/text content and follow the existing link policy.

Refuse any existing destination, including links/aliases and a concurrent new
file; defer Replace/--force. A prior existence check followed by ordinary rename
is insufficient. Stage complete bytes, use qualified no-clobber publication and
validate source closure plus operation ownership at publication. Check guarantees
on supported filesystems/OSes; a persistence helper alone does not establish them.
Failed/cancelled work leaves no partial final file and retains selections for
retry. Once publication succeeds, cancellation cannot undo that success.
“Saved locally” does not claim that a cloud provider synchronized the file.
[Rust rename](https://doc.rust-lang.org/std/fs/fn.rename.html),
[locked tempfile publication guarantees](https://docs.rs/tempfile/3.27.0/tempfile/struct.NamedTempFile.html#method.persist_noclobber).

Existing tools already export highlights. A Zotero user could not discover the
annotations → note → export route; Readwise provides Markdown exports, and
Hypothesis distinguishes readable exports from round-trip data. The testable
Ibis value is direct cross-paper selection with preserved provenance, not
Markdown itself or a new import/sync promise.
[Zotero user report](https://forums.zotero.org/discussion/129198/export-annotations),
[Readwise export](https://docs.readwise.io/reader/docs/faqs/exporting),
[Hypothesis export/import](https://web.hypothes.is/help/exporting-and-importing-annotations/).

## Warn about a changed previously copied key

Planned for 0.6. Current BBT defaults leave populated keys alone, so changes are
rare; Ibis adds bounded history, profile continuity and forgetting, and neither
repairs documents nor guesses ordinary .bib renames.

Show nonblocking “Previously copied as oldKey; current key newKey” with
**Copy change details**, Open reference and Forget remembered key. Normal citation
copy remains the way to use the current key; copying diagnostic details does not
change the remembered baseline. No modal, timeline, per-project acknowledgement,
automatic pinning, source edit or document repair. An old export/alias may still
accept the old key. This does not replace project-bibliography checking.

Remember the logical key actually bound by the engine, before output escaping,
after successful native clipboard publication; never parse it out of clipboard
text. Success counts even if paste is unavailable. Previews, failed/partial
copies, formatted references, quote/brief output, diagnostic copies, CLI stdout
and MCP formatting do not count. A copy completed before close remains a copy;
cancellation cannot retrospectively undo it.

Require proven source namespace + library + native item continuity. Current
Zotero IDs such as default:user are insufficient across profiles/databases.
Zotero's database-backed local-API server ID is a candidate continuity token for
live/closed parity where supported; qualify its versions and snapshot mapping.
Absent proof means comparison unavailable, not guessed continuity. Keep any such
token private. CSL-JSON also needs a continuous chosen source and stable id with
a separately changing citation-key. A same id in another export is insufficient.
Plain .bib identity currently contains the entry key, so a rename cannot prove
continuity, regardless of whether JabRef, BibDesk or another manager made it.
[Zotero's partitioning contract](https://raw.githubusercontent.com/zotero/zotero/db41b99fddcc52d38897c59c161b6adaca036afb/chrome/content/zotero/xpcom/server/server_localAPI.js).

One bounded private owner persists successful-copy batches in their actual order,
outside the source/clipboard barrier. Clipboard success authorizes a record;
storage failure preserves the previous durable baseline and says “Copied; could
not remember this key”. Do not reinterpret this as a failed citation copy.
Forget is ordered against pending saves and covers Ibis-owned recovery copies;
older writes/recovery cannot resurrect forgotten keys. Only report successful
forgetting after durable success. Stable/Nightly stay isolated; no history sync.

Compare only the last recorded key: A→B→A with only A copied has no difference;
if B was copied, returning to A warns B→A. No retained record, eviction, damaged
history or failed source refresh is not “no changes” or document correctness.
Missing items and ambiguous/conflicting keys retain their own errors. CLI may
inspect the same local history without making stdout into a copy event.

Users report thesis breakage after key migration; the warning should show what
changed, without guessing its cause or offering a global replacement.
[Firsthand migration report](https://forums.zotero.org/discussion/130344/zotero-changed-all-my-citation-keys).
BBT currently documents optional regeneration (off by default), library-scoped
uniqueness and bulk-change warnings. This varies from old pinning advice; Ibis
must observe verified current keys rather than promise to stabilize them.
[Current BBT preferences](https://retorque.re/zotero-better-bibtex/preferences/).

## Manager/platform boundaries and evidence still needed

| Source/platform | First-version boundary |
| --- | --- |
| Zotero 8–10 live and closed | Passage candidates, qualified per version and personal/group library; no blanket claim from bibliographic support |
| Zotero 7 | Existing bibliographic support remains; passage support awaits equally supported live/closed evidence, not closed-only behavior that disappears when Zotero opens |
| .bib / CSL-JSON / RIS | References only, regardless of originating manager; no inferred annotation support. Only qualifying stable CSL-JSON identities are key-history candidates |
| Paperpile, JabRef, BibDesk/Skim, EndNote, Mendeley, Citavi | Their annotation/PDF/knowledge-item exports are separate future adapter candidates, not support granted by importing bibliography metadata |
| macOS / Windows / Linux | Qualify opening per named reader; quote text clipboard and new-file publication need each OS, not inference from one platform |

Zotero 8 explicitly added annotation returns to its local API. Paperpile exposes
separate annotation formats, JabRef reads annotations from linked PDFs, and
Citavi distinguishes quotations from thoughts/comments. None makes .bib/RIS a
portable passage format.
[Zotero 8 changes](https://www.zotero.org/support/8.0_changelog),
[Paperpile exports](https://paperpile.com/h/print-pdfs-export-annotations/),
[JabRef file annotations](https://docs.jabref.org/v5/advanced/entryeditor),
[Citavi quotation kinds](https://www1.citavi.com/sub/manual7/en/quoting_a_text_passage.html).

Before implementation, turn each issue's success, refusal, failed I/O, delayed
A-after-B and close/reopen cases into executable scenarios. Source tests include
kind/markup, unknown pages, group comments, standalone objects, changed/deleted
annotations and attachments, and live/closed handoff. Copy/export checks inspect
the actual clipboard/file, with no mixed generations or silent selection loss.
Key tests include profile switching, batch order, storage failure and Forget
during a queued write. Test CJK/IME, RTL, combining characters, multiline text,
screen readers and large annotation libraries within existing resource budgets;
no PDF scan per keystroke or hidden polling.

These reports establish failure hypotheses, not prevalence. Test with people
using Zotero and bibliography-only managers: can they identify quotation versus
comment, the actual open target, an unknown page and a local-only link? Compare
retrieval/copy with their current workflow. Give a recipient the brief without
Ibis/Zotero; check comprehension and manual effort. For key warnings, ask what
changed and what the warning cannot repair. Confusion here should reduce scope
before adding controls. No outreach or user-data collection has been performed.
