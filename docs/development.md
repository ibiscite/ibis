# Development process

This project favours small, verified, reversible steps.

## Contribution standard

One standard for humans and agents. [AGENTS.md](../AGENTS.md) says who may do what; this section says how.

### Issues

- **One outcome per issue**, small enough for one pull request (about 300 changed lines). Split bigger work into GitHub sub-issues before it starts. After a partial pull request, rewrite the issue to what remains. Never open an issue only so a pull request has something to close; use `Part of #N` instead.
- **Title:** a plain sentence about the outcome or the problem, at most 70 characters, with no commit prefix or bracket tag: "Paste into the previous application", "Zotero mapper drops malformed name fields".
- **Body** (at most 200 words): **Problem** (what a user sees now), **Done when** (one to five testable checkboxes) and optional **Notes** (code pointers, decisions). The Task issue form has this shape. Long investigations go in comments.
- **Review follow-ups:** one issue per reviewed pull request, titled "Follow-ups to <thing> (#PR review)", with one checkbox per P3 finding.
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
- Each open milestone's description is one line saying what users can do when it ships, and names its tracking issue.

### Tracking issue

Each active milestone has one, labelled `tracking` and titled `<milestone>: plan, queue and decisions`. Its body is a dashboard of at most 60 lines:

```markdown
**Goal:** one line. Scope: ROADMAP.md.

## Done means
- [ ] … (at most 6)

## Now
- one line each, with an issue or pull request number (at most 5)

## Queue
1. **Lane name** (files it owns)
   1. #N short title

## Waiting on the maintainer
- #N what is needed, recommended answer

## Decisions
- **YYYY-MM-DD** one line (newest 10; older ones move to a "Decisions archive" comment)
```

Every issue in the Queue is in this milestone. History, run links and evidence go in comments. After each merge, comment one line: what users can now do, what is next.

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

The subject is a Conventional Commit (`feat`, `fix`, `perf`, `docs`, `test`, `ci`, `build`, `refactor`, `chore`, `revert`; optional scope; `!` or a `BREAKING CHANGE:` footer for breaking changes) in the imperative mood, at most 72 characters, lower case after the colon, with no full stop. Sign off every commit (`git commit -s`, Developer Certificate of Origin). Disclose AI help with an `Assisted-by:` trailer or the tool's co-author line (ADR 0011). No hashes, run IDs or conversation notes. Squash your own fixups before review so a branch has a few meaningful commits. Release notes come from these commits; see [RELEASING.md](../RELEASING.md).

### Pull requests

- **Title:** a Conventional Commit, because it becomes the commit on `main`: `fix: keep good entries when a .bib file has a duplicate key`.
- **Body:** the template in `.github/pull_request_template.md`, **at most 250 words** excluding images: `Closes #N` (or `Part of #N`), What and why, How it works (plain words for a maintainer who did not write it), Verification, and Screenshots for interface changes. A pull request closes at most one issue.
- **Evidence** (CI run links, hashes, logs, measurement tables) goes in one comment titled `Evidence for <short SHA>`, never in the body or the commits.
- **Drafts:** open the pull request as a draft early; mark it ready when `node scripts/check.mjs` has passed on the final commit.
- **Interface changes** show before/after screenshots with synthetic data, light and dark, for each affected operating system, labelled with the source commit and "browser" or "native", naming any applicable views still missing. Link them from the `evidence` branch or a comment attachment; do not commit them to `docs/images/` unless a document uses them. Detect image format and size from the file bytes before choosing an extension or writing metadata.

### Review and merge

- **Code:** an independent review of the exact head commit by another session, preferably another model, posted as a comment that starts `Independent review of <full SHA> by <tool and model>` and gives the verdict, findings (each P1 and P2 with the commit that fixed it; P3s in one follow-up issue) and the checks run, in at most 150 words plus optional `<details>`. After a small fix the reviewer re-checks only the changed lines and names the new commit; a large fix or a design change needs a new full review. A rebase with an identical tree keeps the review.
- **Documentation only, or tests and synthetic fixtures only:** the author's adversarial check (step 5 of the loop in AGENTS.md), recorded in a comment, replaces the independent review (ADR 0017).
- **Merge** by squash only when all required checks pass on that commit. Never use `--admin` or another bypass. While a release is going out (a hold announced on the tracking issue), nobody merges to `main` until the release pull request has merged and the hold is lifted, because every merge rebuilds the release pull request.

### Versions and releases

Version numbers, release and nightly titles, tags and download links follow [RELEASING.md](../RELEASING.md#releases).

## Checking a change

`node scripts/check.mjs` must pass locally. It checks GitHub Actions expressions with actionlint before the Rust and desktop checks, and runs every desktop crate test (`cargo test -p ibis-desktop`), which plain `cargo test` skips because the desktop crate is not a default workspace member. A failing check exits 1; a missing tool reports incomplete (exit 2) on every platform. It does not run the web builds that CI's Web interface job checks (`pnpm build`, and `pnpm exec vite build --mode evidence` for the screenshot gallery); run those in `apps/desktop` when you change the build setup.

## Working quickly and safely

CI takes a few minutes per pull request. Most delay comes from waiting, so work is arranged to overlap:

- **Verify the exact commit.** Run the full check in a clean checkout of the commit. Use a fresh target directory as well: sharing a target across checkouts can reuse stale project test binaries (#387).

  ```sh
  sha=$(git rev-parse HEAD)
  verification_dir=$(mktemp -d)
  git worktree add --detach "$verification_dir/source" "$sha" &&
    (cd "$verification_dir/source" && CARGO_TARGET_DIR="$verification_dir/target" node scripts/check.mjs --expect-commit "$sha")
  ```

  The guarded preflight gives each Git command five seconds, then kills it and reports a fixed refusal without raw diagnostics. It disables configured filesystem monitors for these calls, because terminating Git does not terminate a monitor's descendants. The repository's configuration is unchanged. Filesystem waits that the operating system cannot interrupt can still delay termination.

  Keep failed output for diagnosis. Remove the verification worktree after inspecting the result. This is POSIX shell; on Windows, run it in Git Bash.

- **Stack instead of waiting.** Start the next issue on a branch based on the previous one (one level at most: never build on a chain of unmerged branches). After the previous pull request is squash-merged, move the branch onto `main` and confirm the content did not change:

  ```sh
  tree=$(git rev-parse next-branch^{tree})
  git fetch origin
  git rebase --onto origin/main previous-branch next-branch
  test "$(git rev-parse next-branch^{tree})" = "$tree" && echo unchanged
  ```

- **Review in parallel.** Run the one independent review of the final diff while CI runs, and save findings in the pull request (fixed, P3s in one follow-up issue, or rejected with a reason).
- **Independent work in parallel.** Issues that touch different files can be worked on at the same time, each in its own worktree and branch. Two writers never share a checkout. Shared files (`Cargo.toml`, `Cargo.lock`, `apps/desktop/src-tauri/src/main.rs`, `ROADMAP.md`) are changed by one branch at a time.

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

Every field is optional. `citation_style`, `density` and `larger_text` remember the picker's style, row density and text size (ADR 0020); each is written only once chosen. A file from an earlier version (format 1, where `library` was a bare path) is read as before and upgraded on the next save, with the old file kept as `settings.json.bak` until the save after that; an older Ibis refuses the upgraded file rather than overwriting it (ADR 0018). On Windows, write the library path with forward slashes or doubled backslashes, as JSON requires. Edits made while the app runs are kept, and a file that cannot be read is reported and never overwritten.

`citation_style` accepts `apa`, `chicago-author-date`, `harvard-cite-them-right`,
`ieee` or `modern-language-association`; omitted or `null` uses `copy_format`.
`update_checks` is `true` after opting in to GitHub update checks, or `false`
after declining. Omitted or `null` means the question has not been answered;
Ibis makes no update check until the person opts in (ADR 0019).

The picker shortcut defaults to Control+Command+R on macOS and Ctrl+Shift+Y on Windows and Linux. For layout limitations, conflicts and custom shortcuts, see [compatibility.md](compatibility.md#picker-shortcuts).

For development, `IBIS_LIBRARY=/path/to/file.bib` opens a library without saving it.

The picker opens without a title bar near the top of the usable area of the display with the pointer (the nearest display when the pointer is between displays, and where the pointer position is unavailable, such as on Wayland, the display it was on or the primary one). If you move it, it keeps that place on that display while it remains fully visible. Drag the search bar edge or search icon to move it; the input remains editable. This uses Tauri's built-in drag region and grants only the window drag permission. Its system colours follow light/dark mode and high contrast; text and controls wrap at larger text sizes.

Enter in the search box copies; a clean, unpinned copy hides the picker. Shift+Enter adds or removes the highlighted reference from the citation. Escape closes an open menu or keyboard dialog first, then clears the search, then hides; chosen references survive clearing and hiding. Search and selection are kept in memory until the picker reopens after more than a minute hidden. A search cleared by Escape also returns on an earlier reopening unless new typing or a completed copy replaced it. The pin beside the format keeps the picker open and on top beside your document (on Wayland it only stays open); unpinned, the picker hides when you click elsewhere or after a clean copy. Copy confirmations expire after 2.4 seconds; copy warnings stay until dismissed.

The output format sits beside the search box; the footer names the library source and reference count, shows freshness or recovery details when they matter, and says what Enter will copy (holding Alt shows what Shift+Enter and Escape do instead). More (Ctrl/⌘ K) holds Choose library, Use Zotero, Reload, Rows (compact, comfortable or spacious), Larger text, Start Ibis when you log in, Keyboard shortcuts and Quit. Starting at login is stored by the system, not in `settings.json` (see [design.md](design.md#settings)); a development build registers as "Ibis (development)", so it never replaces an installed Ibis's entry. `?` outside the search box, or Ctrl/⌘ / anywhere, lists every key in one table built from `apps/desktop/src/keys.ts`. When nothing matches, the picker offers to clear the search, and to search all Zotero libraries when only some are on. The chosen citation style, rows and text size are remembered across restarts (ADR 0020).

## Workflow linting

Install [actionlint](https://github.com/rhysd/actionlint/blob/v1.7.12/docs/install.md) and make it available on `PATH`. Standard options include `brew install actionlint` on macOS, `go install github.com/rhysd/actionlint/cmd/actionlint@v1.7.12` with Go installed, or the official release binaries for Linux, macOS and Windows. CI uses the official 1.7.12 Docker image pinned by digest in the required Dependency policy job; updates should review the release and refresh that pin together.

Run `actionlint -shellcheck= -pyflakes=` to validate every workflow's GitHub-specific contexts, expressions and action usage. The full check uses the same flags; optional shellcheck and pyflakes integrations are disabled so installed auxiliary tools do not change the result. Workflow checks read files without editing them.

## Dependencies

`Cargo.lock` is committed. The Rust toolchain is pinned in `rust-toolchain.toml`. Dependabot proposes grouped updates monthly; each is reviewed and merged only with passing checks. `cargo deny` enforces license compatibility and blocks known vulnerabilities.

## Definition of done

- Acceptance criteria are covered by automated tests (or, for platform behaviour that cannot be automated yet, a recorded manual check in the pull request).
- Documentation, `ROADMAP.md` and snapshots are updated.
- No private data in code, tests, logs or text.
