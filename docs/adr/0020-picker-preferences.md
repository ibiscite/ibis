# 0020. Remember picker preferences in settings format 2

Status: accepted (2026-10-20); adds fields to settings format 2 (ADR 0018), as ADR 0019 did

## Context

The picker design (#223, step 4) lets people choose a citation style (#21), a row density and larger text. These choices should still apply after a restart. ADR 0016 requires an ADR for any change to `settings.json`. ADR 0019 set the precedent that optional fields can be added within format 2, because older builds ignore fields they do not know.

## Decision

Settings format 2 gains three optional fields. When a field is absent, the default applies, and a default is never written.

| Field | Value | Absent means |
| --- | --- | --- |
| `citation_style` | `"apa"`, `"chicago-author-date"`, `"harvard-cite-them-right"`, `"ieee"` or `"modern-language-association"` | Copy citation keys in `copy_format` |
| `density` | `"compact"`, `"comfortable"` or `"spacious"` | `"comfortable"` |
| `larger_text` | `true` or `false` | `false` |

- A style ID names one of the styles bundled with Ibis. An unknown value makes the file invalid, just like an unknown `copy_format`. The file is reported, the backup is used, and nothing is guessed.
- When `citation_style` is set, the picker formats with that style. When the person picks a key format, the field is removed. `copy_format` keeps the last key format either way, so switching back restores it.
- The picker reads all three through one command and writes all three through another. Every other field is kept, using the same locked read, change and save as copies.

Display choice and remembered window position are not part of this decision. They need per-display data and get their own ADR.

## Consequences

- An older build ignores these fields, and a save from it drops them, as ADR 0016 allows.
- Adding a bundled style adds a value, which older builds then report as invalid. That is acceptable before 1.0. Later, an added style may need format 3.
- Style IDs are CSL file names, so later styles can follow the same rule.
