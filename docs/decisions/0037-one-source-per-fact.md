# 0037 One source per fact

Each fact has one program-readable owner. Repeated representations are
generated from it or checked against it. Routine work is a command, so a change
requires fewer facts to remember and fewer places to edit.

Rust types generate bindings, schemas and CLI options through
`node scripts/gen.mjs`. `locales/en-US.ftl` supplies the shared wording.
Interface values come from `tokens.css` and shared `ui/` parts; the canvas
renders the real components. Budgets and geometry rules are data where checks
need them. `scripts/land.sh` owns the merge gate
([0049](0049-local-gate.md)).
