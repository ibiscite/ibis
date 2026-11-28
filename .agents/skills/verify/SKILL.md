---
name: verify
description: Prove an Ibis change works by driving the real CLI or app and capturing evidence. Use after a change people or clients can see, before calling it done, or when asked to run, screenshot or check Ibis.
---

# Verify a change in the running product

A passing test is not proof that Ibis behaves. Drive the surface the change
touches, the way a person or client would, and keep the evidence. Use invented
data only (`fixtures/`), never a real library.

## Pick the surface

- **CLI, MCP, engine contract:** run the built command against fixtures and
  read the output a client reads.
  ```sh
  cargo run -q -p ibis -- search sleep --file fixtures/bibtex/real-world.bib --json
  cargo run -q -p ibis -- cite chen2022 --file fixtures/bibtex/real-world.bib --style apa
  ```
  Check the exit code, the JSON shape against `ibis schema COMMAND`, and the
  human output's words.
- **Desktop app (Tauri) look:** `pnpm -C apps/desktop canvas` renders every
  scenario to `apps/desktop/dist-canvas/pictures`; `pnpm -C apps/desktop contact`
  makes one contact sheet. Look at the pictures for the screens you changed, in
  light and dark.
- **Native Mac app** (`native/macos`): on the Mac, `bash scripts/native-macos.sh shots` saves
  every review state from the invented review library.
- **Installed nightly on a Mac:** `bash scripts/hands-on-macos.sh --journey`
  types into Ibis, TextEdit and Terminal. Leave the Mac alone while it runs.
- **Windows or Linux behaviour** (paste, paths, permissions, windows): run the
  focused test on that system (see [development](../../../docs/development.md#where-work-runs)).

## Report

State what you ran, on which system and head, and what you saw: the command
and its output, or the picture paths. Say plainly what you could not check and
why. A visual change is judged from pictures by someone who did not make it.
