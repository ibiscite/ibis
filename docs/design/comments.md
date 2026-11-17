# Design comments

Comments left on the design playground (`pnpm -C apps/desktop playground`), one per line:
`` - [ ] number · scenario · theme · platform · text size · variant · `selector` · comment ``.

To apply one, open its scenario in the playground, find the element with the selector, and change
the code that owns it: a token in `tokens.css`, a part in `apps/desktop/src/ui/` or the screen's page
under `docs/design/`, never one screen alone. The page updates as you save. Then mark the line `[x]`.
Delete done lines when their change lands.
