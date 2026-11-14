# Citation features and editor protocol

This page records implemented citation editing and CAYW support. It does not
claim complete replacement of Zotero's citation dialog. Plans live in
[ROADMAP.md](../ROADMAP.md); platforms and writing-app limits are in
[compatibility](compatibility.md).

## Finding references

The picker searches title, creator, year, key and recognised identifiers in
the selected personal/group Zotero libraries or library file. Empty search can
show app recents and recently added references. It can open a selected Zotero
item through its Zotero URI.

The picker, CLI `search --shorthand` and MCP read a leading signal (`see`,
`cf.`) and trailing locators: `p. 34`, `pp. 45-48`, `ch. 3`, or
`note "the epigraph"`. All 29 CSL locator terms are recognised. Short labels
need a separator (`p 15` or `p.15`); `p15` remains search text.

## Shaping one citation

The picker supports ordered selections, repeated references, Page, citation
form, plain-text prefix/suffix and Keep My Order. Typed labels
in Page can name another locator type. Empty or incomplete locators are
refused. Engine requests also support styled affixes.

Forms are normal, narrative, author-only, year-only, full and bibliography-only.
Each output lists its supported forms through `ibis outputs`; unsupported
combinations are refused or report field losses. For example, Pandoc narrative
requires one reference. CSL narrative has style and creator-list restrictions.
Mixed forms are supported only where the output can represent them.

Cited since opening collects copied references for an editable, copyable or
exportable bibliography. It does not inspect the document. Ibis cannot edit
live citation fields, place footnotes or establish document-wide numbering and
disambiguation.

Contract evidence lives in `citation.rs`, `syntax_rules.rs`, `key_syntaxes.rs`
and the formatted fixtures. Picker/engine agreement is checked in desktop
`ibis-engine/tests/citations.rs`; CLI flags in `cite_options.rs`; MCP in its
citation tool tests.

## Better BibTeX Cite as You Write

The optional endpoint listens on `127.0.0.1:23124`. Settings shows an address
with a token. Missing/wrong tokens, browser headers and other host names are
refused. It is off by default. See [editor setup](compatibility.md#editors-cite-as-you-write).

| Request | Behaviour |
| --- | --- |
| `probe` | `ready` with a library, otherwise `No`; does not open the picker. |
| Citation request | Opens the picker in the requested output; returns the chosen citation. Hiding/cancelling or a disconnected editor ends the request. |
| `biblatex` | `command`: `autocite` (default), `parencite`, `textcite`, `cite`. |
| `natbib`, `latex`, `cite`, `citep`, `citet` | Natbib/LaTeX command handling. |
| `pandoc` | `brackets` controls brackets where fields do not require them. |
| `typst`, `mmd`, `asciidoctor-bibtex`, `jupyter`, `scannable-cite` | Corresponding engine syntax; AsciiDoc locators are pages only; scan markers require Zotero items. |
| `formatted-citation`, `formatted-bibliography` | Uses the style chosen in Ibis Settings. |
| `json` | CAYW pick shape without Zotero item IDs. |
| `clipboard` | Also copies the result. |

Picks carry locator, prefix, suffix and author suppression. Unknown or
unsupported options, including `selected`, `translate`, `eta` and label
overrides, are refused. `minimize`, `select`, `texstudio` and `application`
are accepted without effect.

`cayw.rs` checks request parsing and syntax; `editor_endpoint.rs` checks picker
ownership, cancellation and formatted output; `cayw_clients.rs` replays recorded
client requests. Interactive editor coverage remains narrower than the protocol.
