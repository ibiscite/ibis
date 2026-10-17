# 0002. License: AGPL-3.0-or-later; contributions under the DCO

Status: accepted (2026-10-17)

## Context
Ibis must remain free software, including any future hosted or web version, while staying simple to contribute to. citeproc-js is available under the AGPL; Zotero is AGPL; Citation Style Language styles are CC BY-SA 3.0.

## Decision
- The whole repository is licensed AGPL-3.0-or-later. Anyone who distributes a modified version, or runs one as a network service, must share its source.
- Contributions are made under the Developer Certificate of Origin (`git commit -s`). There is no contributor license agreement.
- CSL styles and locales are shipped as separate, unmodified data with attribution to citationstyles.org, under their own license.
- `cargo deny` enforces AGPL-compatible dependency licenses in CI.

## Consequences
No single party can relicense contributors' work as closed source. Some companies avoid AGPL code; accepted. A future plugin SDK or data format meant for reuse by other tools may be published separately under a permissive license by a new ADR. Plugins that run as separate processes are not affected by the AGPL.
