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
| Locator typed after the search (`p. 34`, `pp. 45-48`, `ch. 3`, `S. 12`) | done in the picker and engine | #24 | `engine_file/parity_tests.rs::picker_rows_list_the_engines_search_hits_in_order`, `App.test.tsx::typed locators stay with selected references across later searches` |
| Any of the 29 locator types typed in full or short form; full words also without a space (`chap12`, `page123`) | done through search | #24 | `locator.rs::every_csl_locator_type_can_be_typed`, `engine_file/parity_tests.rs::picker_rows_list_the_engines_search_hits_in_order` |
| Short labels joined to a number (`p15`) | excluded | — | `p53` is a protein and `sec61` a gene; Ibis reads `p 15` and `p.15` |
| Textual locator in quotation marks (`note "the epigraph"`) | done through search | #24 | `locator.rs::every_csl_locator_type_can_be_typed`, `engine_file/parity_tests.rs::quoted_page_search_preserves_canonical_text_through_preview_and_copy` |

## Shaping one citation

| Behaviour | Status | Issue | Proof |
| --- | --- | --- | --- |
| Several references in one citation, in the order picked | done | — | `citation.rs::request_preserves_duplicates_order_and_mixed_intent` |
| Drag to reorder | planned (picker) | #947 | to add |
| "Keep sources sorted" for styles that sort; off keeps my order | done in the engine | — | `CitationRequest.keep_order`; picker control in #947 |
| The same reference twice | done | — | `fixtures/cases/03-duplicate-order.json` |
| Locator type and value per reference, all 29 CSL types | done through search; picker editor changes pages only | #24 | `citation.rs::all_29_csl_locator_terms_roundtrip`, `App.test.tsx::the engine receives ordered locators and an empty locator for a record without a citation key` |
| Page ranges and literal page labels | done in search and the Page editor | #24 | `engine_file/parity_tests.rs::labelled_pages_use_the_shared_parser_before_preview_and_canonical_copy`, `engine_file/parity_tests.rs::quoted_page_search_preserves_canonical_text_through_preview_and_copy` |
| Locators with no type | planned | #24 | to add |
| Empty locator refused rather than dropped | done | — | `syntax_rules.rs::empty_locators_are_refused_before_any_syntax_can_drop_them` |
| Prefix and suffix per reference, with italic, bold, sub- and superscript, kept as literal text in every CSL style and output | done in the engine; picker editor in #947; ambiguous Org suffixes are typed losses | #947 | `format.rs::affixes_keep_their_text_and_styling_in_every_representation`, `format.rs::styled_dollar_sequences_stay_literal_in_prefixes_and_suffixes`, `key_syntaxes.rs::an_org_suffix_that_reads_as_a_locator_is_not_carried` |
| Omit author (suppress author) | done | — | `syntax_rules.rs::biblatex_suppressed_author_uses_star_form_for_one_reference` |
| Narrative citation, Smith (2020); Zotero lacks it | done for key syntaxes and for one reference in CSL author-date and MLA styles; numeric styles, several references, contradictory author suppression and APA creator lists with multiple entries refused | #1315 | `format.rs::narrative_citations_name_the_author_then_cite_the_rest`, `format.rs::narrative_citations_refuse_author_suppression`, `format.rs::apa_narrative_refuses_name_lists_that_need_a_different_conjunction` |
| Author only, year only and full citation | done for key syntaxes and CSL styles, refused where a style can't say it (IEEE, year-only MLA) | #1315 | `format.rs` form tests, `syntax_contract.rs` |
| Mixed forms in one citation | done where the syntax can say it, refused otherwise; MyST uses one role per citation | #1207 | `mixed_pandoc_forms_are_explicitly_refused_in_one_request`, `key_syntaxes.rs::mixed_myst_forms_never_share_the_first_references_role` |
| Cite a PDF annotation, citing its page | planned; both Zotero readers skip annotations | #54, #1208 | `prepared.rs::annotation_uses_printed_label_and_rechecks_its_file` covers synthetic engine input only; reader-to-picker test to add |
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
| `format=biblatex` with `command` (default `autocite`) | done as a syntax | #42 | `syntax_rules.rs::biblatex_multicite_keeps_one_locator_per_reference` |
| `format=natbib` with `command`; aliases `latex`, `cite`, `citep`, `citet` | done as a syntax | #42 | `natbib_sources.rs` |
| `format=pandoc` with `brackets` | done as a syntax | #42 | `syntax_contract.rs` |
| `format=typst` | done as a syntax | #42 | `syntax_rules.rs::typst_prefix_escapes_heading_and_list_markers`, `syntax_rules.rs::typst_prefix_escapes_hyphen_list_markers` |
| `format=mmd`, `asciidoctor-bibtex`, `jupyter` | done as syntaxes (`multi-markdown`, `ascii-doc`, `jupyter`); AsciiDoc locators support pages only | #1207 | `key_syntaxes.rs` |
| `format=scannable-cite` | planned | #1207 | to add |
| `format=formatted-citation`, `formatted-bibliography` (BBT uses Zotero's Quick Copy style; Ibis uses the chosen style) | planned | #42 | to add |
| `format=json` (the picks themselves) | done as `cayw-json` (BBT's `pick` shape, without Zotero item ids) | #1207 | `key_syntaxes.rs` |
| Picks carry locator, prefix, suffix and suppress author | done in the contract | #42 | to add, one recorded request per editor |
| `clipboard` also copies the result | planned | #42 | to add |
| `selected` cites what is selected in Zotero | planned | #1314 | to add |
| `format=translate` (any Zotero export translator) | excluded | — | Export translators belong to Better BibTeX (ROADMAP non-goals) |
| `format=eta` (user templates) | excluded | — | Arbitrary templates cannot be checked; capability tables cannot describe them |
| Locator label overrides in the query (`page=`, `Section=sec.`) | excluded | — | Labels come from the style or the syntax; a plain-text override would bypass typed losses |
| `minimize`, `select`, `texstudio` | excluded | — | Window gimmicks; editors paste the returned text |
| `application` | excluded | — | Not documented by BBT |
