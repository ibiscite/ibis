# Parity with Zotero's citation dialog and Better BibTeX

The promise: nobody needs Zotero's citation dialog or Better BibTeX's Cite as
You Write (CAYW) while citing. This list is how we check it. Every behaviour of
Zotero's dialog (Zotero 7 and later, including the 2025 redesign) and of BBT's
CAYW endpoint is listed once with its status in Ibis, the owning issue and the
test that proves it. "To add" means the issue must add that test before it
closes. Change a row in the same pull request that changes the behaviour.

Status: **done** (shipped and tested), **planned** (in scope, owning issue
open), **excluded** (out of scope, with the reason).

Sources: [Zotero word processor plugin
guide](https://www.zotero.org/support/word_processor_plugin_usage), the [new
citation dialog beta
notes](https://forums.zotero.org/discussion/122544/available-for-beta-testing-new-citation-dialog/p1)
and [BBT's CAYW
documentation](https://retorque.re/zotero-better-bibtex/citing/cayw/index.html).

## Finding references

| Behaviour | Status | Issue | Proof |
| --- | --- | --- | --- |
| Search by title, creator and year as you type | done | #1204 | `crates/ibis-core/tests/snapshot_search.rs` |
| Results from My Library and group libraries | done | #1286 | `zotero_sources.rs::chosen_group_libraries_publish_separately_and_a_new_choice_replaces_the_old` |
| Browse by library and collection ("Library mode") | planned (0.5) | #1313 | to add |
| Items selected in Zotero (dialog list; CAYW `selected`) | planned (0.5) | #1314 | to add |
| Suggestions before typing | planned | #45 (recent per app), #1070 (recently added) | to add |
| "Cited" items of the open document first | excluded | — | Ibis does not read documents; recent citations for the app you are in (#45) cover the same need |
| "Open documents" list | excluded | — | Belongs to Zotero's word processor plugins |
| Typing `ibid` (or `ebd.`) picks the last cited work | planned | #45 | to add |
| Open the item in Zotero | planned (0.5) | #359 | to add |
| Locator typed after the search (`p. 34`, `pp. 45-48`, `ch. 3`, `S. 12`) | done for nine labels | #24 | `fixtures/locator-queries.json`, `locator_query.rs::a_labelled_locator_is_split_off` |
| Any of the 29 locator types typed in full or short form, also without a space (`chap12`) | planned | #24 | to add |
| Textual locator in quotation marks (`note "the epigraph"`) | planned | #24 | to add |

## Shaping one citation

| Behaviour | Status | Issue | Proof |
| --- | --- | --- | --- |
| Several references in one citation, in the order picked | done | — | `citation.rs::request_preserves_duplicates_order_and_mixed_intent` |
| Drag to reorder | planned (picker) | #947 | to add |
| "Keep sources sorted" for styles that sort; off keeps my order | done in the engine | — | `CitationRequest.keep_order`; picker control in #947 |
| The same reference twice | done | — | `fixtures/cases/03-duplicate-order.json` |
| Locator type and value per reference, all 29 CSL types | done in the contract | #24 | `citation.rs::all_29_csl_locator_terms_roundtrip`; picker editor in #24 |
| Ranges and literal locators (no type) | planned | #24 | to add |
| Empty locator refused rather than dropped | done | — | `cite_outputs.rs::empty_locators_are_refused_before_any_syntax_can_drop_them` |
| Prefix and suffix per reference, with italic, bold, sub- and superscript | done in the contract | #947 | `citation.rs::hostile_and_large_rich_text_is_plain_data_and_unknown_fields_fail`, `syntax_contract.rs::capabilities_drive_forms_and_report_each_dropped_field` |
| Omit author (suppress author) | done | — | `cite_outputs.rs::biblatex_suppressed_author_uses_star_form_for_one_reference` |
| Narrative citation, Smith (2020); Zotero lacks it | done for key syntaxes; planned for CSL styles | #1315 | `syntax_contract.rs::commands_forms_and_partial_outcomes_never_change_intent_silently` |
| Author only, year only and full citation | done for key syntaxes; planned for CSL styles | #1315 | as above |
| Mixed forms in one citation | done where the syntax can say it, refused otherwise | #1207 | `mixed_pandoc_forms_are_explicitly_refused_in_one_request` |
| Cite a PDF annotation, citing its page | done in the engine | #54 | `prepared.rs::annotation_uses_printed_label_and_rechecks_its_file` |
| Add a note instead of a citation (replaces the yellow "Add Note") | planned (0.5) | #55 | to add |
| Edit the citation text by hand | excluded | — | Breaks "what you see is what's pasted"; use prefix, suffix and forms |
| Ibid, subsequent and disambiguation across a document | excluded | — | Needs the whole document; Ibis reports a notice instead (`format.rs::bibliography_reports_document_year_suffixes_withheld`) |
| Footnote or endnote placement | excluded | — | The word processor decides |
| Insert or edit a bibliography | planned | #356 (session bibliography); bibliography-only form done | `formatted.rs` goldens |
| Item edited in Zotero shows up on the next citation | done | — | `crates/ibis-sources/tests/source_watch.rs` |

## Better BibTeX Cite as You Write

The endpoint is #42. Under [decision 0008](decisions.md#0008-offline-by-default)
it listens only on `127.0.0.1`, on its own port, refuses requests that carry a
browser `Origin`, and is off until switched on in Settings. The formats below
render from the same `CitationRequest` as everything else, so CAYW output is
exactly what the picker would copy.

| Parameter or format | Status | Issue | Proof |
| --- | --- | --- | --- |
| `probe` answers `ready` without opening the picker | planned | #42 | to add |
| Opens the picker and returns the citation | planned | #42 | to add |
| `format=biblatex` with `command` (default `autocite`) | done as a syntax | #42 | `cite_outputs.rs::biblatex_multicite_keeps_one_locator_per_reference` |
| `format=natbib` with `command`; aliases `latex`, `cite`, `citep`, `citet` | done as a syntax | #42 | `natbib_sources.rs` |
| `format=pandoc` with `brackets` | done as a syntax | #42 | `syntax_contract.rs` |
| `format=typst` | done as a syntax | #42 | `citation_typst_prefix.rs` |
| `format=mmd`, `asciidoctor-bibtex`, `jupyter`, `scannable-cite` | planned | #1207 | to add |
| `format=formatted-citation`, `formatted-bibliography` (BBT uses Zotero's Quick Copy style; Ibis uses the chosen style) | planned | #42 | to add |
| `format=json` (the picks themselves) | planned | #1207 (`cayw-json`) | to add |
| Picks carry locator, prefix, suffix and suppress author | done in the contract | #42 | to add, one recorded request per editor |
| `clipboard` also copies the result | planned | #42 | to add |
| `selected` cites what is selected in Zotero | planned | #1314 | to add |
| `format=translate` (any Zotero export translator) | excluded | — | Export translators belong to Better BibTeX (ROADMAP non-goals) |
| `format=eta` (user templates) | excluded | — | Arbitrary templates cannot be checked; capability tables cannot describe them |
| Locator label overrides in the query (`page=`, `Section=sec.`) | excluded | — | Labels come from the style or the syntax; a plain-text override would bypass typed losses |
| `minimize`, `select`, `texstudio` | excluded | — | Window gimmicks; editors paste the returned text |
| `application` | excluded | — | Not documented by BBT |
