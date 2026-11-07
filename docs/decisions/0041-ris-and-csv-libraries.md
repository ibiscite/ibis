# 0041 RIS and CSV libraries

2026-11-07 (#1531). RIS exports and a CSV file in Ibis's template are read as
read-only libraries beside `.bib` and CSL-JSON, with the same lifecycle: open,
refresh on change, skipped records named by line, and the last good library kept
when a read fails.

- **No citation keys.** RIS has none, and neither has the CSV template. Ibis never
  invents one: these records give formatted citations, and key output refuses them
  with `missing-key`, as for a CSL-JSON record without `citation-key`. Zotero's RIS
  import ignores `ID`, and JabRef keeps it only as an unknown `refid` field; neither
  makes a key from it, and Ibis does not either.
- **A record's identity is a versioned hash of its citation fields** (`ris:` or
  `csv:`, then a SHA-256 prefix of the CSL data Ibis reads). The same reference
  keeps its identity across refreshes and fresh exports, whatever the record order,
  notes or database fields; a corrected field, a new edition or a changed type is a
  new reference. Nothing matches by position. `ID`, `AN`, `DB` and `DP` are not read:
  no exporter's `ID` is proven to persist, so they wait until one is. Two records
  with the same citation fields are one reference, kept once.
- **RIS is recognised by its content.** A `.ris` or `.txt` file, or any other whose
  first line is `TY  - `, is read as RIS; one with no record is refused, which turns
  away renamed `.nbib`, RefWorks Tagged and native Web of Science files. UTF-8 only.
- **Damage follows decision 0006.** An unknown type, two different titles, dates or
  page ranges, or a record without `ER` skip that record with its line; an
  unfinished record at the end of the file is skipped on open and reload alike,
  because a refresh already waits for two identical reads (the `.bib` rule).
- **The CSV template's header is checked whole.** Its first row must name only
  template columns, each once, including `type` and `title`; otherwise the file is
  refused with `csv-template` and says so. Rows that do not fit are skipped with their
  line numbers. The columns are listed in the
  [compatibility guide](../compatibility.md#csv-template).
