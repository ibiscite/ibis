# 0026. Keep typed locator interpretation and selection in Rust

Status: accepted (2026-10-25)

## Context

The core already parses trailing locators, but desktop search used the entire
input and copying discarded locator intent. A citation assembled across searches
must retain each reference's own locator. The scope is recorded in #354/#24 and
tracking #1059; design refinement is separate.

## Decision

- Rust remains the production parser. Search accepts transient
  `{text, interpret_locators}` input, returning the same snapshot revision and
  results plus an optional `{search, locator, typed}` interpretation. Explicit
  literal search bypasses interpretation without changing the person's input.
- A prepared reference captures its locator together with its record identity
  and revision. Later queries never rewrite it. Applying a new interpretation
  or removing a locator is an explicit action on that reference.
- Core `CitationSelection {id, locator}` uses the existing `RecordId` and
  `Locator` types. The new selection envelope rejects unknown fields; identity
  matching retains the existing RecordId contract. Key preview and copy resolve
  canonical keys under the existing expected-revision snapshot lock, then use
  the same core selector and formatter in input order.
- CSL metadata retrieval keeps its existing revision check and supports records
  without citation keys. The formatting adapter receives one locator context per
  ordered reference and assigns its own canonical position IDs. Context count
  mismatches are errors. Clipboard output comes from the canonical formatting
  result, never preview annotations.
- Query interpretation mode and locator contents participate in freshness and
  pending-action ownership, including same-ID locator changes. Controls show
  the interpretation, offer literal search, and explicitly update or remove it.
- This slice supports the nine existing labels. Timestamp remains incomplete:
  CSL supports it, but Pandoc key citations do not preserve its locator type.
  A future implementation must define literal fallback and loss warnings; it
  must never silently turn a time into a page.

## Consequences

No persisted format, new command permission, network use, process or dependency
is added. The synthetic website mirrors this workflow against a shared fixture;
it is not a second production parser. Public CLI input from ADR0025 is unchanged.
