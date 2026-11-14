# 0055 Find the passage

Workflow choices for 0.5, recorded 2026-11-14 at the maintainer's request.
These are design assessments using invented material, not real-user evidence
or claims about shipped behaviour. This decision carries forward
[0054](0054-owners-and-boundaries.md) without changing its architecture.

## Comprehension walkthroughs before choosing a presentation

Compare two presentations with the same query and material: a
**References/Passages scope**, starting in References, and **passages unfolded
under references**, with matching children revealed beneath their parent.
The assessments below predict comprehension; the human checks remain open.

### A Zotero highlight and its separate comment

Vale's *Marsh Routes* (2024) has the highlight "The east channel closed at
dusk", on PDF page 8, with printed label 417. Its separate comment reads
"Compare the winter survey". Search for `channel`.

- **Scopes:** References finds the bibliographic record through its passage;
  a labelled passage match offers Passages. In Passages, Highlight and Comment
  are distinct rows with the same named reference. Selecting Highlight shows
  the quoted source words; its associated Comment appears separately, labelled
  as the researcher's words. Searching `winter` finds Comment directly.
- **Unfolded:** Vale appears once with a Highlight child and its Comment child.
  The relationship is close, but a child can look like a citation variant or
  part of the highlight. Collapsing the parent hides the search evidence.
- **Design assessment:** scopes make the object being acted on explicit.
  A comment must remain a separate object even when displayed with a highlight;
  neither Copy Quote nor a search match may concatenate them.

### A standalone note

An unattached Zotero note, *Questions for the estuary visit*, says "Ask whether
the winter survey used the same gauge". It has no parent reference or page.

- **Scopes:** Passages finds a Note row. Its preview says "Standalone note ·
  No reference · Citation page unknown". Add Note reuses those authored words;
  Copy Quote with Reference is unavailable.
- **Unfolded:** there is no bibliographic parent. Inventing a reference heading
  misrepresents the note; a separate Notes group breaks the parent/child model.
- **Design assessment:** scopes accommodate an unattached note without a fake
  reference. Neither presentation supplies missing bibliographic facts.

### A bibliography-only reference

An invented `references.bib` holds Sen's *Water Markers* (2022), with no PDF,
notes or annotations. The person searches `Sen` to cite it.

- **Scopes:** References selects Sen and keeps the existing citation journey.
  This library has no passage capability, so it offers no empty Passages scope.
  In a mixed library selection, Sen still appears only in References.
- **Unfolded:** Sen has no disclosure control or children; citing also works.
  A global passage query needs an explanation when this reference has none.
- **Design assessment:** both can preserve bibliography-only use. Scopes need
  capability-based availability, not a second empty screen for these users.

### An outsider reading a Markdown brief

A colleague receives a brief containing Vale's highlight and its comment.
They have neither Ibis nor Zotero and open it in a plain Markdown reader.

- **Scopes:** selecting each passage for the brief exposes its type before
  export. Each exported section repeats Vale's full reference and separates
  the comment from the quotation; local links are optional aids.
- **Unfolded:** selecting Vale's parent could mean its citation, all its children
  or only the matching child. Explicit passage selection is still necessary.
- **Design assessment:** neither layout proves that the exported file is clear.
  Repeating attribution per passage makes each section independently readable;
  an independent reader must still demonstrate that they can distinguish them.

## Choice and actions

Choose **References/Passages scope**, with References the default on a fresh
opening. Keep reference rows flat; do not also build unfolding passage trees.
References searches bibliographic fields and may identify a passage match with
its type and an explicit **Show Matching Passages** action. Passages searches
passage text; each result names Highlight, Comment or Note and its reference,
or says that no reference is known. Image-only highlights say "No text quotation".
Passages is available when a selected source supports it, even if currently
empty; no matches says "No matching passages", with **Back to References**.

In References, selection chooses a reference for citation and Enter delivers
the shelf, through the citation interaction owned by
[0045](0045-citation-line-token-field.md) and the
[picker](../design/picker.md#keys). The following differences apply in Passages:

| Action | Exact result |
| --- | --- |
| Select a passage by click or arrows | Highlight the row and show its typed detail preview, full text and provenance, without moving focus; do not edit a citation or reuse any text. |
| Enter from passage search or the result list | Enter that same read-only detail by focusing its heading; no second preview. No external open, clipboard write, citation or paste. No selection means no action and "Select a passage". |
| Enter in detail | No global delivery action. On a focused button it activates that named button, using ordinary keyboard behaviour. |
| Add Note on a Note or Comment | Deliver its full authored text, labelled Note or Comment, with its known reference as attribution. Use the current plain-text or Markdown reuse choice and the delivery policy in 0009; never wrap it as a source quotation. |
| Add Note on a Highlight | Unavailable: a highlight is source text. An associated comment has its own Add Note action. This never creates or edits a Zotero note. |
| Copy Quote with Reference on a text Highlight | Copy the complete stored quotation and its known reference, with the citation-page status, as plain text or escaped Markdown. Copy only; never paste, summarise, truncate or include a comment. |
| Copy Quote with Reference on a Note, Comment or image-only annotation | Unavailable, explaining "Authored note", "Annotation comment" or "No text quotation". |

The detail's Plain Text / Markdown choice shows the exact reuse text before
Add Note or Copy Quote is activated. A failed action keeps the detail and says
why; a successful copy stays there. Add Note's paste/copy outcome and any hiding
follow [0009](0009-insertion.md), rather than a second delivery policy.

**Existing citation:** if the shelf holds Sen plus a locator when Passages is
chosen, retain that draft unchanged. Hide the citation shelf in this scope;
show **Citation kept · Back to References** instead. Enter cannot deliver the
hidden draft, and passage selection, Add Note and Copy Quote cannot replace it.
Returning restores Sen, its locator, order and output. A fresh opening returns
to References; no passage action silently becomes the next citation.

**Keyboard return:** both scopes are labelled controls reachable by Tab;
arrow keys choose within that control and Enter activates it. Back to References
is also a named action in More. Closing detail with Esc or Back returns to its
passage row; leaving Passages returns to the previous References query,
selection, scroll and focus. If that row disappeared, focus Search and say so.
Layer precedence and platform Back keys remain owned by
[0047](0047-opening-references-placement-and-back.md#going-back).

Acceptance: begin with Sen on the shelf, find Vale's Comment, press Enter,
then Copy Quote's unavailable control cannot write anything. Add Note delivers
only the labelled comment and attribution. Return by keyboard: Sen's shelf
and search position are unchanged, and Enter again has its citation meaning.
Repeat with an empty shelf: returning must not invent a Vale citation.

## Named opens and page provenance

[0047](0047-opening-references-placement-and-back.md#opening-the-reference)
owns source-opening defaults, alternates and recovery. A passage's explicit
actions name the actual target: **Open Annotation in Zotero**, **Open PDF at
Page 417**, or **Show Reference in Zotero** / **Open references.bib**. Offer
the annotation action only with a verified annotation target. A page-only
passage says so; it does not pretend to open an annotation.

If a named open fails, report that target and offer available alternatives
as separate actions. Never silently open another target. If only opening the
PDF at its start is possible, call the action **Open PDF**, not **at Page**.

[0036](0036-reader-trust-boundary.md) owns page provenance, citable labels,
conflicts and confirmation; this workflow consumes that result. Display physical
and citation pages separately, for example "PDF page 8 · cite p. 417" or
"PDF page 6 · cite p. xii". Unknown is "Citation page unknown"; ambiguous is
"Citation page ambiguous · Confirm page". Quote reuse and briefs keep that
status, without supplying a guessed locator. A person may copy without a page;
only confirmation establishes a citation page where 0036 requires it.

Acceptance: `xii` remains a printed label, never PDF index 12. Only a verified
mapping may open its physical page. Missing labels, duplicate labels and
disagreeing PDF/annotation labels take 0036's unknown or confirmation paths.
A failed annotation open offers PDF/reference actions without launching them.
Cancelling page confirmation leaves the page unresolved and the citation intact.

## One new Markdown brief

**Export Evidence Brief** collects explicitly chosen passages, not all passages
of a selected reference. Its review list is the exported order: initially the
order added, with Move Up, Move Down and Remove. Search ranking and citation
style sorting cannot reorder it. An optional question is literal text, not a
search, instruction to an agent, template or request for an answer. No generated
answer or summary is added. A changed or unavailable selected passage requires
review before saving; never silently omit it or substitute different text.

Save to **one new `.md` file only**, through exclusive creation at the write,
not an existence check followed by replacement. An existing destination,
including one created after the dialog opened, says "File already exists ·
Choose another name". Never overwrite or append, even if a system dialog offers
Replace. Cancel writes nothing; failed save reports failure, preserves any
existing file and removes an incomplete file created by this attempt.

Each section contains its passage type, complete text, full human-readable
reference, citation-page status and any separately known PDF page. Repeat the
full reference for **every passage**, including adjacent passages from the same
reference; no `ibid.`, key-only attribution or "see above". Notes and comments
are labelled authored text, outside quotation blocks. A comment may name its
associated highlight, but never merges with it. Missing attribution is explicit.

One Markdown renderer escapes every supplied field (question, text, titles,
names and labels). Preserve words and line breaks; escape Markdown punctuation
and HTML so supplied headings, lists, fences, links and `<script>` remain text.
Only the renderer emits document structure and validated links. Stored note HTML
first follows [0036](0036-reader-trust-boundary.md)'s typed note representation.
Use the sharing boundary in [0051](0051-references-in-and-out.md) for egress.
Verified local Zotero links may be offered separately as **Open in Zotero
(local; requires Zotero)**; they are never the attribution a reader depends on.

For the invented Vale pair, the readable result has this structure (the actual
writer escapes supplied text):

```text
Question
Was the channel closed in winter?

1. Highlight
> The east channel closed at dusk
Reference: Vale, Mira. Marsh Routes. Reed Press, 2024.
Citation page: 417. PDF page: 8.

2. Comment (researcher's words)
On highlight: The east channel closed at dusk
Compare the winter survey
Reference: Vale, Mira. Marsh Routes. Reed Press, 2024.
Citation page: 417 (associated highlight). PDF page: 8.
```

Acceptance: export those two sections in reversed selection order; each still
stands alone and the comment's association remains identifiable by passage text.
Try the question `# Answer [now](file:///invented) <script>`
and passage text containing backticks, brackets and newlines: they render
literally, create no links or HTML and trigger no action. Open without Ibis or
Zotero. Race an existing destination and cancel another save: neither changes
an existing file. Link correctness alone cannot prove reader comprehension.

## Changed copied keys: wording and identity before implementation

[0016](0016-settings.md) owns copied-key persistence, qualifying identities,
clipboard commit semantics and storage failures. This is the warning design,
not a second history store. History does not establish which document used a
key. Never say "Your document is broken" or claim a manuscript was repaired.

Continuity requires a stable library identity and a source-native item identity
independent of its citation key, with evidence that it still denotes the same
item. Zotero's scoped item identity qualifies; a CSL-JSON `id` qualifies only
when its stability and non-reuse are established, not merely because `id` exists.
Titles, authors, DOI matches, content hashes and similar-looking keys do not
establish continuity. **A plain BibTeX key rename is never inferred.**
An adapter that cannot prove continuity supplies none; changed-key detection
is unavailable for those items.

| Observed case | Wording and action |
| --- | --- |
| Same proven item A: last copied `valeOld`, now `vale2024` | "You previously copied valeOld for Marsh Routes; this item's current key is vale2024." Offer Show Reference, Copy Current Citation and Forget Remembered Key. No manuscript edits. |
| Same identity, title renamed, key unchanged | No changed-key warning. A bibliographic edit is not a citation-key rename. |
| Remembered `valeOld` for A now provably identifies another item B in that library | "You copied valeOld for Marsh Routes; valeOld now identifies Water Markers in this library. Check earlier citations." Name both items; offer Inspect References and Forget Remembered Key. Do not describe this as a rename. |
| A disappeared and a new item was created, even with the same title, DOI or reused identifier | Observed deletion ends continuity. Never attach A's baseline to the recreated item. A provable new binding can give the reassignment warning above; otherwise say only "Previously copied reference is unavailable" when inspecting history. |
| Read failed, item is filtered out, or library is temporarily closed | Not evidence of deletion or reassignment. Preserve history; do not manufacture a changed-key warning from absence. |
| Same key or similar item in another personal/group library, profile or file library | Separate histories; no cross-library rename or reassignment inference. Moving an item does not carry a baseline without source proof. |

Warnings appear beside affected current key output before delivery and in its
result, naming the library when needed to distinguish it. A warning does not
block a valid, newly selected B: it explains earlier use of the key. Refusing a
stale selection or conflicting current binding stays with the existing source
and citation rules in [0006](0006-sources.md) and [0028](0028-engine-first.md).
Formatted output without keys has no changed-key notice. If no usable current
key exists, offer Show Reference or another output, not Copy Current Citation.
Use current display names when resolvable, otherwise "a reference no longer
available". Do not retain title snapshots just to name a deleted reference.

**Commit:** apply 0016's clipboard-success rule to Copy Current Citation as a
normal citation action, not a diagnostic copy of the warning. Copy Quote and
Add Note qualify only if their delivered reference actually contains a citation
key, with that same current-item proof and successful receipt. Preview, cancel
and failure outcomes are exercised below; the persistence rules stay in 0016.

**Forget and retain:** Forget Remembered Key removes that item's baseline;
Settings also offers Forget All Remembered Keys. Removing a configured library
forgets its history; switching libraries does not. Forgetting affects warnings,
not the clipboard or any document, and a pending earlier copy cannot restore a
forgotten baseline. A later explicit successful key copy starts a new baseline.
Keep only the latest baseline per qualifying item across restarts, with no age
expiry, within 0016's bounded local storage. At capacity evict the item least
recently successfully copied; capacity and file format remain implementation
details under 0016. The interface says this is limited copy memory, not an audit
of every citation. Deletion does not transfer retained history to a new item.
Replacing a baseline forgets the older key; its later reassignment is outside
this warning's coverage unless another retained baseline still names that key.

Acceptance: copy A/old successfully, rename its key, preview new and cancel:
old remains the baseline. Fail a clipboard write: still old. Successfully copy
new but fail paste: new becomes the baseline. Separately, keep A/old as the
baseline and assign old to B: name A and B, never claim A was renamed to B.
Delete/recreate A, reuse an id, change only a `.bib` key, and repeat keys across
two libraries: no inferred continuity. Forget while a copy is pending,
restart, then copy deliberately:
forgotten history stays gone until that later successful copy. Check eviction
and a failed history save against 0016 without claiming durable success.

## Passage consent and files with unknown references

Apply [0033](0033-engine-for-agents-and-tools.md)'s **per-library passage consent,
off by default**, to passage search, snippets, detail, quote and brief content
through CLI, serve and MCP. Enabling one library never enables another; selecting
All Libraries grants nothing. A new or removed-and-readded library starts off.
Revocation takes effect before an unfinished response is delivered; already
delivered text cannot be recalled. References-only responses must not leak a
passage through previews or match snippets. Private links remain excluded even
with consent. The app's own passage use does not implicitly enable agent access.

A folder is a library for that consent. PDFs and Markdown notes with unknown
bibliographic details remain findable and previewable as files, with their
display name and "Reference details unknown". Never turn a filename, nearby
reference, extracted title or DOI guess into a citation. Add Note can reuse a
note with that explicit status. A text selection in a PDF can offer **Copy Quote
(without reference)**, labelled with the file name and page status, and a brief
repeats that description per passage. **Copy Quote with Reference** and citing
are unavailable until bibliographic association is established and reviewed.

Acceptance: enable library A, leave B off, query both as an agent: only A can
contribute passage content, neither contributes private links. Revoke A while
detail is pending: no passage content is returned. With an unassociated folder
PDF, copying source words must say that reference details are unknown; it must
not borrow a shelf reference or fabricate an author, year, key or printed page.

## Human evidence still required; handoff after landing

- **Real Zotero user:** with invented material, find the highlight and separate
  comment, explain whose words each contains, use Add Note versus Copy Quote,
  and return by keyboard with an existing citation intact. Record observed
  actions and misunderstandings on the implementation issue, not a predicted pass.
- **Bibliography-only user:** find and cite Sen without passage setup; explain
  what Enter delivers, including after returning from Passages in a mixed library.
- **Independent brief reader:** without Ibis or Zotero, attribute each passage,
  distinguish comment from quote, interpret unknown/ambiguous pages and local
  links, and recognise the literal question without treating it as an answer.

The purge owner should replace these exact obsolete descriptions with links to
this decision after it lands; keep feature scope and unrelated rules:

- [ROADMAP, 0.5](../../ROADMAP.md#05-find-the-passage): the entire **Rules to settle
  before building 0.5** block, from **Presentation** through **Testing**; link
  **Add a note instead of a citation** and **Changed-key warning** here for their
  settled behaviour. Keep the feature bullets and issue links. The duplicate
  changed-key feature under 0.6 should point to the settled 0.5 placement.
- [Source checks and passage reuse](../design/sources-and-sheets.md#source-checks-and-passage-reuse-planned):
  the five bullets **Find a passage**, **Open passage**, **Copy quote with
  reference**, **Export evidence brief**, **Changed copied key** become one
  link here. Keep **Copy bibliography entry**, bibliography-check rules, the
  sheet foundation and **Cited since opening** at their current sources.
