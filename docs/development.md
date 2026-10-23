# Development process

This project favours small, verified, reversible steps.

## Contribution standard

One standard for humans and agents. [AGENTS.md](../AGENTS.md) says who may do what; this section says how.

### Issues

- **One outcome per issue**, small enough for one pull request (about 300 changed lines). After a partial pull request, rewrite the issue to what remains. Never open an issue only so a pull request has something to close; use `Part of #N` instead.
- **Title:** a plain sentence about the outcome or the problem, with no commit prefix: "Zotero mapper drops malformed name fields".
- **Body:** what a user sees now, and when it is done. The Task issue form has this shape.
- **Ideas and questions** start in [GitHub Discussions](https://github.com/ibiscite/ibis/discussions); an issue is opened when the work is agreed.

### Labels

| Family | Labels | Rule |
| --- | --- | --- |
| Type | `type:bug`, `type:feature`, `type:task`, `type:docs` | Exactly one on every issue. `task` covers tests, CI, build, performance, refactoring and maintenance. |
| Area | `area:core`, `area:desktop`, `area:cli`, `area:formatting`, `area:zotero`, `area:infra`, `area:a11y`, `area:security` | Only when it narrows the issue down. `infra` is CI, test harnesses, packaging and release tooling; `security` includes privacy. |
| Platform | `platform:linux`, `platform:macos`, `platform:windows` | Only when it affects some platforms, not all. |
| Status | `status:needs-design`, `status:blocked`, `status:needs-maintainer` | Exceptions only. `needs-design`: a short plan is reviewed before code. `needs-maintainer`: a reserved decision or the maintainer's own hands. |
| Other | `tracking`, `upstream`, `good first issue` | |
| Bots | `autorelease: pending`, `autorelease: tagged`, `dependencies` | Added by Release Please and Dependabot; leave them alone. |

No other labels; each family has one colour. Pull requests need no labels: the title's type says it. There are no priority labels: the milestone and the order of the tracking issue's Queue say what comes first.

### Milestones

- Release milestones are named `0.N Name` to match `ROADMAP.md`, plus `1.0`; `Later` holds anything not scheduled. A milestone closes when its release ships.
- A release milestone holds only work that blocks that release. If it would not stop the release, move it to the next milestone or `Later`.
- **An open issue without a milestone is untriaged.** Triage means: fix the title and labels, then set a milestone or close the issue.

### Tracking issue

Each active milestone has one, labelled `tracking` and titled `<milestone>: plan, queue and decisions`. Its body:

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

Keep the body current (Now and Queue). Comment on the issue only for decisions and handoffs. Every issue in the Queue is in this milestone.

### Branches and worktrees

- One short-lived branch per pull request, `<type>/<issue>-<short-name>` from `origin/main`, for example `feat/22-setup-library`; `<type>` is the pull request title's type. No other prefixes.
- One writer per worktree: `git fetch origin main && git worktree add ../wt/<name> -b <branch> origin/main`. Remove the worktree after merging.
- Long-lived branches: `main`, the bots' `release-please--*` and `dependabot/*`, and `evidence`, a data branch where CI publishes synthetic native screenshots (`scripts/publish-evidence.mjs`); never open pull requests from `evidence` or delete it.
- Merged branches are deleted automatically, and the weekly hygiene run deletes head branches of closed pull requests. `main` history is never rewritten.

### Commits

```text
fix: keep Typst citation notes as literal text

Optional body: at most five lines on behaviour.

Signed-off-by: Name <email>
Assisted-by: <tool and model>
```

The subject is a Conventional Commit (`feat`, `fix`, `perf`, `docs`, `test`, `ci`, `build`, `refactor`, `chore`, `revert`; optional scope; `!` or a `BREAKING CHANGE:` footer for breaking changes) in the imperative mood, at most 72 characters, lower case after the colon, with no full stop. Sign off every commit (`git commit -s`, Developer Certificate of Origin). Disclose AI help with an `Assisted-by:` trailer or the tool's co-author line (ADR 0011). No hashes, run IDs or conversation notes. Release notes come from these commits; see [RELEASING.md](../RELEASING.md).

### Pull requests

- **Title:** a Conventional Commit, because it becomes the commit on `main`: `fix: keep good entries when a .bib file has a duplicate key`.
- **Body:** the template in `.github/pull_request_template.md`, **at most 250 words** excluding images: `Closes #N` (or `Part of #N`), What and why, How it works (plain words for a maintainer who did not write it), Verification, and Screenshots for interface changes. A pull request closes at most one issue.
- **Evidence** (CI run links, hashes, logs) goes in a pull request comment or the review, never in the body or the commits.
- **Interface changes** link CI's synthetic screenshots on the `evidence` branch, or attach before/after images made with synthetic data. Do not commit them to `docs/images/` unless a document uses them.
- **Checked automatically:** `node scripts/pr-lint.mjs --title … --branch … --body-file …` checks the title, branch, issue link, length, evidence and template sections before you open the pull request; the "Pull request conventions" job runs the same check on every pull request and re-runs when the description is edited.

### Review and merge

- **Code:** an independent review of the exact head commit by another session, preferably another model, posted as a comment that starts `Independent review of <full SHA> by <tool and model>`, gives the verdict and lists the P1 and P2 findings. After a small fix the reviewer re-checks only the changed lines and names the new commit; a large fix or a design change needs a new full review. A rebase with an identical tree keeps the review.
- **Documentation only, or tests and synthetic fixtures only:** the author's adversarial check (step 5 of the loop in AGENTS.md), recorded in the pull request body or a comment, replaces the independent review (ADR 0017).
- **Merge** by squash only when all required checks pass on that commit. The squash commit's title is the pull request title plus ` (#N)`, and its message is short: `Closes #N`, the `Assisted-by:` trailer, and `Signed-off-by:` naming whoever actually signed the branch's commits, never someone else. Never use `--admin` or another bypass. When the merge queue is enabled, the release pull request merges like any other. Until then, while a release hold is announced on the tracking issue, nobody merges to `main` until the release pull request has merged, because every merge rebuilds it.

### Versions and releases

Version numbers, release and nightly titles, tags and download links follow [RELEASING.md](../RELEASING.md#releases).

## Checking a change

The full check is the required "CI result" on the exact head commit, then the merge queue, which always validates the complete app. Run focused tests locally while editing; run `node scripts/check.mjs` locally to diagnose a CI failure or before changing the check itself. It checks GitHub Actions expressions with actionlint before the Rust and desktop checks, and runs every desktop crate test (`cargo test -p ibis-desktop`), which plain `cargo test` skips because the desktop crate is not a default workspace member. A failing check exits 1; a missing tool reports incomplete (exit 2) on every platform. It does not run the web builds that CI's Web interface job checks (`pnpm build`, and `pnpm exec vite build --mode evidence` for the screenshot gallery); run those in `apps/desktop` when you change the build setup.

Interface-only pull requests run script tests, TypeScript, interface tests and both web builds, without native app builds or GUI checks. `scripts/ci-paths.mjs` lists those browser inputs, including the gallery geometry JSON fixture; native, build and unknown paths retain native validation. Both names of a rename count. Missing, incomplete or changed API metadata enables every check. Documentation keeps its existing fast path; main and the merge queue always validate the complete app.

## Working quickly and safely

- **Verify the exact commit locally when you need to.** Run the full check in a clean checkout of the commit. Allocate one verification target per agent/session and keep exclusive ownership through package cleanup, checking and inspection of its captured output. Serialize runs; never let another worktree build into that target meanwhile. [#387](https://github.com/ibiscite/ibis/issues/387) observed stale project test binaries even with serialized shared-target builds.

  Allocate this directory once, then retain it for successive verification runs:

  ```sh
  verification_dir=$(mktemp -d)
  ```

  For each commit, use a clean detached worktree, disable incremental compilation and invalidate the four checked application packages before checking. The observed #387 remedy preserved dependency artifacts; disabling incremental compilation alone is not the recipe.

  ```sh
  sha=$(git rev-parse HEAD)
  git worktree add --detach "$verification_dir/source" "$sha" &&
    (
      cd "$verification_dir/source" &&
      export CARGO_TARGET_DIR="$verification_dir/target" CARGO_INCREMENTAL=0 &&
      cargo clean --locked -p ibis -p ibis-core -p ibis-desktop -p ibis-zotero-local &&
      node scripts/check.mjs --expect-commit "$sha"
    )
  ```

  The guarded preflight gives each Git command five seconds, then kills it and reports a fixed refusal without raw diagnostics. It disables configured filesystem monitors for these calls, because terminating Git does not terminate a monitor's descendants. The repository's configuration is unchanged. Filesystem waits that the operating system cannot interrupt can still delay termination.

  Retain the output from this run while the target is still exclusively owned. Check that the intended enabled regressions actually ran, including their names, counts and ignored status; exit 0 alone did not establish that in #387. Inspecting a later mutable shared binary is not evidence about this run. This procedure records the observed remedy, not a newly measured speedup or a Cargo correctness guarantee. Use a fresh target if ownership or invalidation is uncertain.

  Keep failed output for diagnosis. After inspecting the result, remove the detached source worktree before reusing its path; retain the target:

  ```sh
  git worktree remove "$verification_dir/source"
  ```

  Do not force removal of a dirty worktree. When finished with the session, remove only its owned verification directory. This is POSIX shell; on Windows, run it in Git Bash.

- **Branch from `main`, never from another branch.** Stacked branches merged by squash have silently reverted changes on `main`. After merging `main` into a branch, `git diff origin/main` must show only that branch's change.
- **Review while CI runs**, and record each finding in the pull request as fixed, filed or rejected with a reason.

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
{ "schema_version": 2, "shortcut": "CommandOrControl+Shift+Y", "library": { "bibtex": "/path/to/refs.bib" }, "copy_format": "pandoc", "citation_style": "apa", "density": "compact", "larger_text": true, "update_checks": false }
```

Every field is optional. `citation_style`, `density`, `larger_text` and `show_paste_preview` remember the picker's style, row density, text size and output visibility (ADR 0020); defaults stay unwritten. The preview is on unless `show_paste_preview` is `false`. A file from an earlier version (format 1, where `library` was a bare path) is read as before and upgraded on the next save, with the old file kept as `settings.json.bak` until the save after that; an older Ibis refuses the upgraded file rather than overwriting it (ADR 0018). On Windows, write the library path with forward slashes or doubled backslashes, as JSON requires. Edits made while the app runs are kept, and a file that cannot be read is reported and never overwritten.

`citation_style` accepts `apa`, `chicago-author-date`, `harvard-cite-them-right`,
`ieee` or `modern-language-association`; omitted or `null` uses `copy_format`.
`update_checks` is `true` after opting in to GitHub update checks, or `false`
after declining. Omitted or `null` means the question has not been answered;
Ibis makes no update check until the person opts in (ADR 0019).

The picker shortcut defaults to Control+Command+R on macOS and Ctrl+Shift+Y on Windows and Linux. For layout limitations, conflicts and custom shortcuts, see [compatibility.md](compatibility.md#picker-shortcuts).

For development, `IBIS_LIBRARY=/path/to/file.bib` opens a library without saving it.

The picker opens without a title bar near the top of the usable area of the display with the pointer (the nearest display when the pointer is between displays, and where the pointer position is unavailable, such as on Wayland, the display it was on or the primary one). If you move it, it keeps that place on that display while it remains fully visible. Drag the search bar edge or search icon to move it; the input remains editable. This uses Tauri's built-in drag region and grants only the window drag permission. Its system colours follow light/dark mode and high contrast; text and controls wrap at larger text sizes.

Enter in the search box copies; a clean, unpinned copy hides the picker. Shift+Enter adds or removes the highlighted reference from the citation. Escape closes an open menu or keyboard dialog first, then clears the search, then hides; chosen references survive clearing and hiding. Search and selection are kept in memory until the picker reopens after more than a minute hidden. A search cleared by Escape also returns on an earlier reopening unless new typing or a completed copy replaced it. Keep Window Open in More (⌘⇧P on macOS, Ctrl+Shift+P elsewhere) keeps the picker open and on top beside your document (on Wayland it only stays open); the footer pin turns it off. Unpinned, the picker hides when you click elsewhere or after a clean copy. Copy confirmations expire after 2.4 seconds; copy warnings stay until dismissed.

The output format sits beside the search box; the footer names the library source and reference count, shows freshness or recovery details when they matter, and says what Enter will copy (holding Alt shows what Shift+Enter and Escape do instead). More (Ctrl/⌘ K) holds Choose library, Use Zotero, Reload, Rows (compact, comfortable or spacious), Keep Window Open, Larger text, start at login ("Open at Login", "Start when you sign in" or "Start on Login", as in the menu bar or tray), Keyboard shortcuts and Quit. Starting at login is stored by the system, not in `settings.json` (see [design.md](design.md#settings)); a development build registers as "Ibis (development)", so it never replaces an installed Ibis's entry. `?` outside the search box, or Ctrl/⌘ / anywhere, lists every key in one table built from `apps/desktop/src/keys.ts`. When nothing matches, the picker offers to clear the search, and to search all Zotero libraries when only some are on. The chosen citation style, rows and text size are remembered across restarts (ADR 0020).

## Workflow linting

Install [actionlint](https://github.com/rhysd/actionlint/blob/v1.7.12/docs/install.md) and make it available on `PATH`. Standard options include `brew install actionlint` on macOS, `go install github.com/rhysd/actionlint/cmd/actionlint@v1.7.12` with Go installed, or the official release binaries for Linux, macOS and Windows. CI uses the official 1.7.12 Docker image pinned by digest in the required Dependency policy job; updates should review the release and refresh that pin together.

Run `actionlint -shellcheck= -pyflakes=` to validate every workflow's GitHub-specific contexts, expressions and action usage. The full check uses the same flags; optional shellcheck and pyflakes integrations are disabled so installed auxiliary tools do not change the result. Workflow checks read files without editing them.

## Dependencies

`Cargo.lock` is committed. The Rust toolchain is pinned in `rust-toolchain.toml`. Dependabot proposes grouped updates monthly; each is reviewed and merged only with passing checks. `cargo deny` enforces license compatibility and blocks known vulnerabilities.

## Definition of done

- Acceptance criteria are covered by automated tests (or, for platform behaviour that cannot be automated yet, a recorded manual check in the pull request).
- Documentation, `ROADMAP.md` and snapshots are updated.
- No private data in code, tests, logs or text.
