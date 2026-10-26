# Development process

Until 1.0 this project moves fast and keeps only the rules that prevent real damage: a broken `main`, lost work, silent reverts, wrong citations, and security or privacy mistakes ([ADR 0027](adr/0027-fast-until-1-0.md)).

## Contribution standard

One standard for humans and agents. [AGENTS.md](../AGENTS.md) says who may do what; this section says how.

### Issues

- **One outcome per issue**, small enough for one pull request (one user-visible feature). After a partial pull request, rewrite the issue to what remains. Never open an issue only so a pull request has something to close; use `Part of #N` instead.
- **Title:** a plain sentence about the outcome or the problem, with no commit prefix: "Zotero mapper drops malformed name fields".
- **Body:** what a user sees now, and when it is done. The Task issue form has this shape.
- **Ideas and questions** start in [GitHub Discussions](https://github.com/ibiscite/ibis/discussions); an issue is opened when the work is agreed.

### Labels

- **Type**, exactly one on every issue: `type:bug`, `type:feature`, `type:task` (tests, CI, build, performance, refactoring, maintenance) or `type:docs`.
- `status:needs-maintainer` marks a reserved decision or something needing the maintainer's own hands; `tracking` marks the tracking issue; `good first issue` and `help wanted` invite outside help.
- Area, platform and other status labels are optional; add one only when it helps someone find the issue.
- Release Please and Dependabot add their own labels; leave them alone. Pull requests need no labels: the title's type says it. There are no priority labels: the milestone and the order of the Queue say what comes first.

### Milestones

- Milestones are releases, named as in `ROADMAP.md` (`0.2 Cite anywhere, trust the key`, `1.0 Stable`); `Later` holds anything not scheduled. A milestone closes when its release ships.
- Closed work is filed in the release that shipped it: every merged pull request and every issue closed as completed carries that release's milestone. Issues closed as not planned or duplicate, and pull requests closed without merging, carry none.
- A release milestone holds only work that blocks that release. If it would not stop the release, move it to the next milestone or `Later`.
- **An open issue without a milestone is untriaged.** Triage means: fix the title and labels, then set a milestone or close the issue.

### Tracking issue

There is one, labelled `tracking` and titled `Ibis plan, queue and decisions`. Its body fits on one screen:

```markdown
**Goal:** one line. Scope: ROADMAP.md.

## Now
- one line per open pull request: what it does, what it waits on

## Queue
**Lane name** (the files it owns)
1. #N short title

## Waiting on the maintainer
- #N what is needed, recommended answer

## Decisions
- **YYYY-MM-DD** one line
```

Keep the body current (Now and Queue), with at most five items per lane. Decisions older than a week leave the body; the edit history keeps them. Comment on the issue only for decisions and handoffs.

### Branches and worktrees

- One short-lived branch per pull request from `origin/main`, named `<type>/<issue>-<short-name>` where possible, for example `feat/22-setup-library`.
- One writer per worktree: `git fetch origin main && git worktree add ../wt/<name> -b <branch> origin/main`. Remove the worktree after merging.
- Long-lived branches: `main` and the bots' `release-please--*` and `dependabot/*`.
- Merged branches are deleted automatically. `main` history is never rewritten.

### Commits

```text
fix: keep Typst citation notes as literal text

Optional body: at most five lines on behaviour.

Signed-off-by: Name <email>
Assisted-by: <tool and model>
```

The subject is a Conventional Commit (`feat`, `fix`, `perf`, `docs`, `test`, `ci`, `build`, `refactor`, `chore`, `revert`; optional scope; `!` or a `BREAKING CHANGE:` footer for breaking changes) in the imperative mood, at most 72 characters, lower case after the colon, with no full stop. Sign off every commit (`git commit -s`, Developer Certificate of Origin). Disclose AI help with an `Assisted-by:` trailer or the tool's co-author line (ADR 0011). No hashes, run IDs or conversation notes. Release notes come from these commits; see [RELEASING.md](../RELEASING.md).

### Pull requests

- **Title:** a Conventional Commit, because it becomes the commit on `main` and the release notes: `fix: keep good entries when a .bib file has a duplicate key`. This is the one pull request rule CI enforces.
- **Body:** `Closes #N` or `Part of #N`, then what changes for a user and how it was checked, in plain words. The template is a suggestion, not a form. Include screenshots with synthetic data for interface changes.
- `node scripts/pr-lint.mjs --title … --branch … --body-file …` runs the same check as the "Pull request conventions" job: errors only for the title, commit identities and weakened tests; everything else is a warning.

### Review and merge

- **Risky areas** need one independent review of the final head by a session that did not write the change, preferably another model ([ADR 0027](adr/0027-fast-until-1-0.md)): the updater, release signing and CI permissions; settings and other files Ibis writes; the clipboard, escaping and pasting; window permissions, native input and the shortcut portal; BibTeX and Zotero reading, citation syntaxes and formatting; untrusted input, network and security boundaries. Post it as a comment starting `Independent review of <full SHA>`, with the verdict and the P1 and P2 findings. After a small fix the reviewer re-checks only the changed lines.
- **Everything else** merges on green required checks plus the author's own adversarial check (step 5 of the loop in AGENTS.md), recorded in the pull request.
- Every P1 and P2 is fixed with a test before merging; filing it does not count.
- **Merge** by squash only when all required checks pass on that commit, with the pull request title plus ` (#N)` as the commit title and a short message: `Closes #N`, the `Assisted-by:` trailer, and `Signed-off-by:` naming whoever actually signed the branch's commits. Never use `--admin` or another bypass. The release pull request goes through the merge queue like any other.
- **Review tools:** free ones only, before and after the CodeRabbit trial ends on 2026-11-06. Never add payment details, start a paid plan, use credits or enable overages. `@coderabbitai full review`, `@codex review` and `@codex security review` are optional extra eyes for a risky change; check each finding against the code before acting on it.

### Versions and releases

Version numbers, release and nightly titles, tags and download links follow [RELEASING.md](../RELEASING.md#releases).

## Checking a change

The full check is the required "CI result" on the exact head commit, then the merge queue, which always validates the complete app. While editing, run the focused tests; run `node scripts/check.mjs` locally to diagnose a CI failure or when changing the check itself. It exits 1 when a check fails and 2 when a tool is missing. It skips the web builds; run `pnpm build` in `apps/desktop` when you change the build setup. Which jobs a pull request needs is decided by `scripts/ci-paths.mjs`. Native UI and browser gallery jobs run only on pushes to `main` and manual dispatch; neither is required by "CI result".

To verify a commit without stale build output ([#387](https://github.com/ibiscite/ibis/issues/387)), check it out in a clean detached worktree with its own Cargo target, and confirm the intended tests actually ran, because exit 0 alone does not prove it:

```sh
dir=$(mktemp -d) sha=$(git rev-parse HEAD)
git worktree add --detach "$dir/source" "$sha" && (cd "$dir/source" &&
  CARGO_TARGET_DIR="$dir/target" CARGO_INCREMENTAL=0 node scripts/check.mjs --expect-commit "$sha")
git worktree remove "$dir/source"
```

## Setting up a computer

First install the build tools for your platform (below). Then every platform needs the same tools; their versions come from the repository, so a fresh clone matches CI:

- Rust through [rustup](https://rustup.rs/). `rust-toolchain.toml` selects Rust 1.99.1 with rustfmt and clippy on first use; do not install it by hand.
- [Node.js](https://nodejs.org/) 22 or newer; CI uses 22.
- pnpm 11.25.0, pinned in `apps/desktop/package.json` (`packageManager`). Install it with `npm install --global pnpm@11.25.0` (or `corepack enable pnpm` on Node.js 22–24; newer Node.js no longer bundles Corepack).
- cargo-deny: `cargo install --locked cargo-deny`.
- [actionlint](https://github.com/rhysd/actionlint/blob/v1.7.12/docs/install.md) 1.7.12 on `PATH` (for example `brew install actionlint`). The full check runs `actionlint -shellcheck= -pyflakes=`; CI uses the official image pinned by digest.
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
6. actionlint 1.7.12: the official binary in a per-user folder on `PATH`, after checking its SHA-256 against the release checksums.

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
{ "schema_version": 2, "shortcut": "CommandOrControl+Shift+Y", "library": { "bibtex": "/path/to/refs.bib" }, "copy_format": "pandoc", "citation_style": "apa", "density": "compact", "larger_text": true, "update_checks": false }
```

Every field is optional. `citation_style`, `density`, `larger_text`, `show_paste_preview` and `search_mark` remember the picker's style, row density, text size, output visibility and search glyph (ADR 0020); defaults stay unwritten. The preview is on unless `show_paste_preview` is `false`. A file from an earlier version (format 1, where `library` was a bare path) is read as before and upgraded on the next save, with the old file kept as `settings.json.bak` until the save after that; an older Ibis refuses the upgraded file rather than overwriting it (ADR 0018). On Windows, write the library path with forward slashes or doubled backslashes, as JSON requires. Edits made while the app runs are kept, and a file that cannot be read is reported and never overwritten.

`citation_style` accepts `apa`, `chicago-author-date`, `harvard-cite-them-right`,
`ieee` or `modern-language-association`; omitted or `null` uses `copy_format`.
`update_checks` is `true` after opting in to GitHub update checks, or `false`
after declining. Omitted or `null` means the question has not been answered;
Ibis makes no update check until the person opts in (ADR 0019).

The picker shortcut defaults to Command+Shift+Y on macOS and Ctrl+Shift+Y on Windows and Linux. Defaults remain unwritten; every explicitly stored shortcut is kept, including the old macOS chord. For layout limitations, conflicts and switching back to the default, see [compatibility.md](compatibility.md#picker-shortcuts).

For development, `IBIS_LIBRARY=/path/to/file.bib` opens a library without saving it. A development build registers for start at login as "Ibis (development)", so it never replaces an installed Ibis's entry. For a debug preview on a particular display, pass Tauri's `--config` with finite `x` and `y` and `center: false` on the `main` window.

How the picker behaves (keys, pinning, the footer, More, placement) is in [design.md](design.md) and the user guide in [README.md](../README.md); `?` or Ctrl/⌘ / in the app lists every key from `apps/desktop/src/keys.ts`.

## Dependencies

`Cargo.lock` is committed. The Rust toolchain is pinned in `rust-toolchain.toml`. Dependabot proposes grouped updates monthly; each is reviewed and merged only with passing checks. `cargo deny` enforces license compatibility and blocks known vulnerabilities.

## Definition of done

- Acceptance criteria are covered by automated tests (or, for platform behaviour that cannot be automated yet, a recorded manual check in the pull request).
- Documentation, `ROADMAP.md` and snapshots are updated.
- Integration changes (Zotero, citation styles, operating-system behaviour) are checked against current upstream documentation and release notes, and test failure as well as success.
- No private data in code, tests, logs or text.
