# 0046 Evidence before change, and real users first

2026-11-11 (maintainer, after a risk audit). In four weeks, Ibis grew to about 190k lines while behaviour was rebuilt several times: writing projects were built, removed and rebuilt within a week, and the picker window was restructured six times. Nobody outside the project uses Ibis yet. So:

- **A decided behaviour changes only for new evidence,** such as a tester's report, a measured problem or a failed check, not for a new idea alone. The decision file says what that evidence is.
- **Five to ten real testers come before more 0.4 scope.** Their reports decide the order of the remaining 0.4 work.
- **Docs describe only what has shipped.** Plans live in ROADMAP.md alone. Each release reports one size number (lines of product and test code) beside its notes, and consolidation is judged by it.
- **The app ships its dependencies' licence notices,** generated at build time.
