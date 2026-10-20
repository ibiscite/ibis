# Development process

This project favours small, verified, reversible steps.

## Flow of work

1. **Issue.** Every change starts from an issue that states the user-visible outcome and acceptance criteria. Its labels, title and milestone follow [Issues, labels and milestones](#issues-labels-and-milestones).
2. **Branch.** One short-lived branch per issue, named `<type>/<issue>-<short-description>`, e.g. `feat/12-zotero-snapshot-reader`.
3. **Test first.** Add a failing test or fixture that captures the acceptance criteria, then implement.
4. **Check.** `node scripts/check.mjs` must pass locally. It checks GitHub Actions expressions with actionlint before the Rust and desktop checks, and runs every desktop crate test (`cargo test -p ibis-desktop`), which plain `cargo test` skips because the desktop crate is not a default workspace member. A failing check exits 1; a missing tool reports incomplete (exit 2) on every platform. It does not run the web builds that CI's Web interface job checks (`pnpm build`, and `pnpm exec vite build --mode evidence` for the screenshot gallery); run those in `apps/desktop` when you change the build setup.
5. **Pull request and review.** Follow [AGENTS.md](../AGENTS.md#working-autonomously) for review, merge permissions and required checks. Use a Conventional Commit title and link the issue (`Closes #12`). Describe the resulting behaviour, verification and limits for someone who did not write the change.
6. **Merge.** Follow the [same merge conditions](../AGENTS.md#working-autonomously); `main` history is never rewritten.

## Working quickly and safely

CI takes a few minutes per pull request. Most delay comes from waiting, so work is arranged to overlap:

- **Verify the exact commit.** Run the full check in a clean checkout of the commit. Use a fresh target directory as well: sharing a target across checkouts can reuse stale project test binaries (#387).

  ```sh
  sha=$(git rev-parse HEAD)
  verification_dir=$(mktemp -d)
  git worktree add --detach "$verification_dir/source" "$sha" &&
    (cd "$verification_dir/source" && CARGO_TARGET_DIR="$verification_dir/target" node scripts/check.mjs)
  ```

  Keep failed output for diagnosis. Remove the verification worktree after inspecting the result. This is POSIX shell; on Windows, run it in Git Bash.

- **Stack instead of waiting.** Start the next issue on a branch based on the previous one (one level at most: never build on a chain of unmerged branches). After the previous pull request is squash-merged, move the branch onto `main` and confirm the content did not change:

  ```sh
  tree=$(git rev-parse next-branch^{tree})
  git fetch origin
  git rebase --onto origin/main previous-branch next-branch
  test "$(git rev-parse next-branch^{tree})" = "$tree" && echo unchanged
  ```

- **Review in parallel.** Run the one independent review of the final diff while CI runs, and save findings in the pull request (fixed, P3 filed as an issue, or rejected with a reason).
- **Independent work in parallel.** Issues that touch different files can be worked on at the same time, each in its own worktree and branch. Two writers never share a checkout. Shared files (`Cargo.toml`, `Cargo.lock`, `apps/desktop/src-tauri/src/main.rs`, `ROADMAP.md`) are changed by one branch at a time.

## Issues, labels and milestones

One standard for every issue and pull request, open or closed:

- **Type**, exactly one: `type:bug`, `type:feature`, `type:docs`, `type:test`, `type:ci`, `type:build` (packaging and release tooling), `type:perf`, `type:chore` (refactoring, maintenance, planning). A pull request takes the type of its title prefix (`fix:` is `type:bug`, `refactor:` is `type:chore`).
- **Area**, one or more where it helps: `area:core`, `area:desktop`, `area:cli`, `area:formatting`, `area:search`, `area:testing`, `area:a11y`, `area:i18n`, `area:privacy`, `area:security`, `area:ux`, `area:compatibility`, `area:integration`, `area:extensions`, `area:health`.
- **Platform** and **source**, only when it applies to some of them: `platform:linux`, `platform:macos`, `platform:windows`, `platform:web`; `source:zotero`, `source:bibtex`.
- **Status**: `status:ready` (can be picked up), `status:blocked`, `status:needs-design` (a short plan is reviewed before code).
- **Needs**: `needs:triage` (new finding, not yet placed), `needs:review`, `needs:maintainer` (a reserved decision only).
- **Specials**: `tracking`, `good first issue`, `help wanted`, `upstream`. Release Please and Dependabot add their own labels (`autorelease: pending`, `dependencies`, `rust`, `github_actions`); leave them alone.

No other labels. Each family has one colour.

**Titles.** An issue title is a plain sentence about the outcome or the problem, with no commit prefix or bracket tag: "Paste into the previous application", "Zotero mapper silently drops malformed CSL text". A pull request title is a Conventional Commit (`fix: keep Typst citation notes as literal text`), because it becomes the commit on `main`.

**Milestones.** Every open issue has one milestone: `0.N Name` for a planned release (matching `ROADMAP.md`), `1.0`, or `Later` for anything not scheduled. Each open milestone's description is one line saying what users can do when it ships, and names its tracking issue. A milestone closes when its release ships.

**Versions and releases.** Version numbers, release and nightly titles, tags and download links follow [RELEASING.md](../RELEASING.md#releases).

**Tracking issues.** Each active milestone has one, labelled `tracking` and titled `<milestone>: plan, queue and decisions`. Its body holds the current plan: what done means, **Now** (work in progress and where it is), the ordered **Queue**, what waits on the maintainer, what is deferred, and decisions taken. History goes in comments. After each merge, add one line saying what users can now do and what is next.

## Setting up a computer

First install the build tools for your platform (below). Then every platform needs the same tools; their versions come from the repository, so a fresh clone matches CI:

- Rust through [rustup](https://rustup.rs/). `rust-toolchain.toml` selects Rust 1.99.1 with rustfmt and clippy on first use; do not install it by hand.
- [Node.js](https://nodejs.org/) 22 or newer; CI uses 22.
- pnpm 11.25.0, pinned in `apps/desktop/package.json` (`packageManager`). Install it with `npm install --global pnpm@11.25.0` (or `corepack enable pnpm` on Node.js 22–24; newer Node.js no longer bundles Corepack).
- cargo-deny: `cargo install --locked cargo-deny`.
- actionlint 1.7.12 on `PATH` (see "Workflow linting").
- Optional: [Pandoc](https://pandoc.org/installing.html), for the citation fidelity test. Without it the test is skipped; `IBIS_REQUIRE_PANDOC=1` makes it fail instead, as in CI. `cargo install --locked cargo-insta` for `cargo insta review`.

Then, from the clone, run the full check:

```sh
node scripts/check.mjs
```

### macOS setup

Install the Xcode Command Line Tools: `xcode-select --install`.

### Linux setup

Install a C compiler, pkg-config and the WebKitGTK development packages. On Debian and Ubuntu (the AppIndicator library is for the tray icon):

```sh
sudo apt-get install build-essential pkg-config libwebkit2gtk-4.1-dev librsvg2-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev libayatana-appindicator3-dev patchelf
```

Other distributions: see the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/#linux).

### Windows setup

CI runs every check on Windows too. Install, in this order:

1. [Git for Windows](https://git-scm.com/download/win). Then clone to a short path, because build output paths are long:

   ```powershell
   git config --global core.longpaths true
   git config --global core.autocrlf false
   git clone https://github.com/ibiscite/ibis C:\src\ibis
   ```

   `.gitattributes` checks out every text file with LF line endings whatever `core.autocrlf` says. Use an editor that follows `.editorconfig` so new files keep LF too.

2. [Visual Studio 2022 Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the "Desktop development with C++" workload (MSVC and a Windows SDK).
3. WebView2. It is part of Windows 11; on Windows 10 install the Evergreen runtime. See the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/#windows).
4. rustup, accepting the default MSVC host (`x86_64-pc-windows-msvc`, or `aarch64-pc-windows-msvc` on Arm).
5. Node.js, pnpm, cargo-deny and, optionally, Pandoc and cargo-insta, as listed above. Prefer `npm install --global pnpm@11.25.0`; `corepack enable pnpm` may be refused for lack of permissions.
6. [actionlint 1.7.12](https://github.com/rhysd/actionlint/blob/v1.7.12/docs/install.md): install the official binary for your architecture in a per-user executable folder and add it to `PATH`. Verify its SHA-256 against the official release checksum manifest before extracting or running it; no administrator terminal is needed.

Open a new terminal so the `PATH` changes apply, then, in `C:\src\ibis`:

```powershell
node scripts/check.mjs
cargo test -p ibis-desktop --locked
pnpm -C apps/desktop install
pnpm -C apps/desktop tauri dev
```

The other shell snippets in this document are POSIX shell. On Windows, run them in Git Bash, which comes with Git for Windows.

## Running the desktop app

```sh
cd apps/desktop
pnpm install
pnpm tauri dev
```

Choose a library from the picker or from the menu bar (tray) item. The choice and other preferences are saved in `settings.json` in the app's configuration folder:

- macOS: `~/Library/Application Support/io.github.ibiscite.ibis/`
- Windows: `%APPDATA%\io.github.ibiscite.ibis\`
- Linux: `$XDG_CONFIG_HOME/io.github.ibiscite.ibis/`, or `~/.config/io.github.ibiscite.ibis/` when `XDG_CONFIG_HOME` is not set

```json
{ "schema_version": 2, "shortcut": "CommandOrControl+Shift+Y", "library": { "bibtex": "/path/to/refs.bib" }, "copy_format": "pandoc", "citation_style": "apa", "density": "compact", "larger_text": true }
```

Every field is optional. `citation_style`, `density` and `larger_text` remember the picker's style, row density and text size (ADR 0020); each is written only once chosen. A file from an earlier version (format 1, where `library` was a bare path) is read as before and upgraded on the next save, with the old file kept as `settings.json.bak` until the save after that; an older Ibis refuses the upgraded file rather than overwriting it (ADR 0018). On Windows, write the library path with forward slashes or doubled backslashes, as JSON requires. Edits made while the app runs are kept, and a file that cannot be read is reported and never overwritten.

The picker shortcut defaults to Control+Command+R on macOS and Ctrl+Shift+Y on Windows and Linux. For layout limitations, conflicts and custom shortcuts, see [compatibility.md](compatibility.md#picker-shortcuts).

For development, `IBIS_LIBRARY=/path/to/file.bib` opens a library without saving it.

The picker opens without a title bar near the top of the usable area of the display with the pointer (the nearest display when the pointer is between displays, and where the pointer position is unavailable, such as on Wayland, the display it was on or the primary one). If you move it, it keeps that place on that display while it remains fully visible. Drag the search bar edge or search icon to move it; the input remains editable. This uses Tauri's built-in drag region and grants only the window drag permission. Its system colours follow light/dark mode and high contrast; text and controls wrap at larger text sizes. Enter still copies and hides, Shift+Enter selects, and Escape steps back one layer: it closes the More menu, then clears the search and selection, then hides. A search comes back if the picker returns within a minute; it is kept in memory only. The pin beside the format keeps the picker open and on top beside your document (on Wayland it only stays open); unpinned, the picker hides when you click elsewhere or after copying. Copy confirmations expire after 2.4 seconds; copy warnings stay until dismissed. The output format sits beside the search box; the footer shows library status, key hints and More (Ctrl/⌘ K), which holds Choose library, Use Zotero, Reload, Rows (compact, comfortable or spacious), Larger text and Quit. The chosen citation style, rows and text size are remembered across restarts (ADR 0020).

## Commits and releases

Commit conventions and disclosure are in [AGENTS.md](../AGENTS.md#version-control).
Versioning, release automation, nightly builds and recovery are in
[RELEASING.md](../RELEASING.md).

## Workflow linting

Install [actionlint](https://github.com/rhysd/actionlint/blob/v1.7.12/docs/install.md) and make it available on `PATH`. Standard options include `brew install actionlint` on macOS, `go install github.com/rhysd/actionlint/cmd/actionlint@v1.7.12` with Go installed, or the official release binaries for Linux, macOS and Windows. CI uses the official 1.7.12 Docker image pinned by digest in the required Dependency policy job; updates should review the release and refresh that pin together.

Run `actionlint -shellcheck= -pyflakes=` to validate every workflow's GitHub-specific contexts, expressions and action usage. The full check uses the same flags; optional shellcheck and pyflakes integrations are disabled so installed auxiliary tools do not change the result. Workflow checks read files without editing them.

## Dependencies

`Cargo.lock` is committed. The Rust toolchain is pinned in `rust-toolchain.toml`. Dependabot proposes grouped updates monthly; each is reviewed and merged only with passing checks. `cargo deny` enforces license compatibility and blocks known vulnerabilities.

## Definition of done

- Acceptance criteria are covered by automated tests (or, for platform behaviour that cannot be automated yet, a recorded manual check in the pull request).
- Documentation, `ROADMAP.md` and snapshots are updated.
- No private data in code, tests, logs or text.
