# First EndNote XML slice

Invented journal article (type 17), book (6) and book section (5), read through
`load_file`. Only bibliographic records are supported: no annotations,
attachment import or claim to other EndNote types. Styled text becomes plain
text; EndNote corporate-name commas are decoded before shared name handling.
Dates support years and numeric `YYYY/MM/DD` or `YYYY-MM-DD` parts; unparsed
publication dates warn and retain a valid year. DTDs are rejected outright;
the decoder accepts text only and has no file or network resolver.

The fidelity test runs the installed Pandoc `endnotexml` reader on
`library.xml`. Observed with Pandoc 3.13: common fields agree after these
explicit differences (Ibis fields are checked in the parser fixture):

| Field | Ibis | Pandoc 3.13 |
| --- | --- | --- |
| Identity | Shared `Records` content hash; duplicate identities quarantined; no citation key | `rec-number` as `id` |
| Edition | CSL `edition` | Omitted |
| Editors | Book editors and series editors have distinct CSL roles | Omitted |
| Electronic resource number | Validated CSL `DOI` | Omitted |
| Publication date | Given year, month and day | Year only |
| Pages | CSL `page`, book `number-of-pages` | `pages` |
| Article number | CSL `issue` | `number` |
| Book secondary title | CSL `collection-title` | `container-title` |

Run the parser tests and the Pandoc fidelity test:

```sh
RUSTC_WRAPPER= CARGO_TARGET_DIR="$PWD/target" cargo test -p ibis-sources --test endnote_xml --locked --offline
IBIS_REQUIRE_PANDOC=1 RUSTC_WRAPPER= CARGO_TARGET_DIR="$PWD/target" cargo test -p ibis --test fidelity --locked --offline
```

The fidelity test skips when Pandoc is unavailable unless `IBIS_REQUIRE_PANDOC` is set; it never downloads it.

Maintainer scope, 2026-11-14: retain the existing bounded file lifecycle and
attachment/deadline work in `file.rs`; merge forward `stage3-attachments`
when it lands. This slice owns dependency changes and uses cached, locked
quick-xml 0.42.0 without installs. Leave the changes uncommitted.
