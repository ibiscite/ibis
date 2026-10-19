# Development process

This project favours small, verified, reversible steps.

## Flow of work

1. **Issue.** Every change starts from an issue that states the user-visible outcome and acceptance criteria. Issues belong to a milestone (`0.1 Picker`, `0.2 Shared preview`, ...).
2. **Branch.** One short-lived branch per issue, named `<type>/<issue>-<short-description>`, e.g. `feat/12-zotero-snapshot-reader`.
3. **Test first.** Add a failing test or fixture that captures the acceptance criteria, then implement.
4. **Check.** `node scripts/check.mjs` must pass locally. It checks GitHub Actions expressions with actionlint before the Rust and desktop checks, and runs every desktop crate test (`cargo test -p ibis-desktop`), which plain `cargo test` skips because the desktop crate is not a default workspace member. A failing check exits 1; a missing tool reports incomplete (exit 2) on every platform.
5. **Pull request.** Title in Conventional Commits form (`feat: read Zotero libraries while Zotero is closed`), linked to its issue (`Closes #12`). CI runs on Windows, macOS and Linux; when only documentation changed, the Desktop jobs report success without building the native app. A newer push to a pull request or to `main` cancels the older run, so on `main` the newest commit (which contains every earlier merge) is the one tested; merges that change only documentation start no run on `main`. Only `main` saves Rust build caches. The description says what users can now do, how it was verified, and what was not verified.
6. **Review.** One independent review of the final diff, recorded as a pull request comment naming the reviewed commit. Pull requests that change only documentation, or only tests and fixtures, skip it (ADR 0017), and a small fix after review needs only its changed lines re-checked. P1 and P2 findings are fixed before merging (filing them as issues does not count); P3 findings may become issues. Which changes an agent may merge itself is set by ADR 0015 and `AGENTS.md`.
7. **Merge.** Squash-merge when checks pass. The pull request title becomes the commit on `main`. `main` is protected and always releasable; its history is never rewritten.

## Working quickly and safely

CI takes a few minutes per pull request. Most delay comes from waiting, so work is arranged to overlap:

- **Verify the exact commit.** Before asking for review or merging, run the full check in a clean checkout of the commit (unfinished work may be pushed earlier as a draft), so uncommitted files cannot make it pass. Reusing one build directory keeps it fast:

  ```sh
  root=$(git rev-parse --show-toplevel); sha=$(git rev-parse HEAD)
  dir="$(mktemp -d)/ibis-verify"   # a new, empty path: never someone else's work
  git worktree add --detach "$dir" "$sha" &&
    (cd "$dir" && CARGO_TARGET_DIR="$root/target/verify" node scripts/check.mjs) &&
    git push
  git worktree remove --force "$dir" 2>/dev/null; git worktree prune
  ```

  This is POSIX shell; on Windows, run it in Git Bash.

- **Stack instead of waiting.** Start the next issue on a branch based on the previous one (one level at most: never build on a chain of unmerged branches). After the previous pull request is squash-merged, move the branch onto `main` and confirm the content did not change:

  ```sh
  tree=$(git rev-parse next-branch^{tree})
  git fetch origin
  git rebase --onto origin/main previous-branch next-branch
  test "$(git rev-parse next-branch^{tree})" = "$tree" && echo unchanged
  ```

- **Review in parallel.** Run the one independent review of the final diff while CI runs, and save findings in the pull request (fixed, P3 filed as an issue, or rejected with a reason).
- **Independent work in parallel.** Issues that touch different files can be worked on at the same time, each in its own worktree and branch. Two writers never share a checkout. Shared files (`Cargo.toml`, `Cargo.lock`, `apps/desktop/src-tauri/src/main.rs`, `ROADMAP.md`) are changed by one branch at a time.

## Progress reports

Each milestone has a tracking issue (label `tracking`). Its body holds the current plan: what done means, **Now** (work in progress and where it is), the ordered **Queue**, what waits on the maintainer, what is deferred, and decisions taken. History goes in comments. After each merge, add one line saying what users can now do and what is next. Questions that need the maintainer carry the label `needs:maintainer`.

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
6. actionlint 1.7.12: download and verify the official release, then install it
   in a per-user executable folder. Run this in PowerShell; use `arm64` instead
   of `amd64` on Arm Windows. No administrator terminal is needed.

   ```powershell
   $ErrorActionPreference = 'Stop'
   $arch = 'amd64'
   $asset = "actionlint_1.7.12_windows_$arch.zip"
   $release = 'https://github.com/rhysd/actionlint/releases/download/v1.7.12'
   $downloadDir = Join-Path ([System.IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString())
   New-Item -ItemType Directory $downloadDir | Out-Null
   $zip = Join-Path $downloadDir $asset
   $manifest = Join-Path $downloadDir 'actionlint_1.7.12_checksums.txt'
   Invoke-WebRequest "$release/$asset" -OutFile $zip
   Invoke-WebRequest "$release/actionlint_1.7.12_checksums.txt" -OutFile $manifest
   $line = Get-Content $manifest | Where-Object { ($_ -split '\s+')[-1] -eq $asset }
   if (!$line) { throw 'The official checksum manifest does not list this ZIP.' }
   $expected = ($line -split '\s+')[0]
   if ((Get-FileHash $zip -Algorithm SHA256).Hash -ne $expected) {
     throw 'actionlint checksum mismatch; do not extract or run this ZIP.'
   }
   $toolsDir = Join-Path $env:LOCALAPPDATA 'Programs\IbisTools'
   New-Item -ItemType Directory -Force $toolsDir | Out-Null
   Expand-Archive -LiteralPath $zip -DestinationPath $toolsDir -Force
   [string]$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
   if (($userPath -split ';') -notcontains $toolsDir) {
     $newPath = ($userPath.TrimEnd(';') + ';' + $toolsDir).TrimStart(';')
     [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
   }
   $env:Path = "$toolsDir;$env:Path"
   actionlint -version
   ```

   The checksum comes from the [official release manifest](https://github.com/rhysd/actionlint/releases/download/v1.7.12/actionlint_1.7.12_checksums.txt).
   The example preserves existing user PATH entries and updates this terminal;
   new terminals pick up the saved user PATH.

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
{ "schema_version": 2, "shortcut": "CommandOrControl+Shift+Y", "library": { "bibtex": "/path/to/refs.bib" }, "copy_format": "pandoc" }
```

Every field is optional. A file from an earlier version (format 1, where `library` was a bare path) is read as before and upgraded on the next save, with the old file kept as `settings.json.bak` until the save after that; an older Ibis refuses the upgraded file rather than overwriting it (ADR 0018). On Windows, write the library path with forward slashes or doubled backslashes, as JSON requires. Edits made while the app runs are kept, and a file that cannot be read is reported and never overwritten.

The picker shortcut defaults to Control+Command+R on macOS and Ctrl+Shift+Y on Windows and Linux. Cmd+Shift+Space is not used on macOS because it opens the Siri typing field, which takes the keystrokes meant for the picker; Ctrl+Shift+Space is not used elsewhere because it types a non-breaking space in Word and LibreOffice Writer. Ctrl+Shift+Y takes only rarely used commands (the Google Docs dictionary, the Firefox Downloads window, the VS Code Debug Console). On Windows and X11 it follows the key labelled Y, so on German layouts it is not Redo. A shortcut is modifiers and one key joined by `+`; `CommandOrControl` means Cmd on macOS and Ctrl elsewhere. An invalid shortcut is reported and the default is used. On Windows, avoid Ctrl+Alt combinations: many keyboard layouts use them as AltGr to type characters.

For development, `IBIS_LIBRARY=/path/to/file.bib` opens a library without saving it.

The picker opens without a title bar near the top of the usable area of the display with the pointer (the nearest display when the pointer is between displays, and where the pointer position is unavailable, such as on Wayland, the display it was on or the primary one). If you move it, it keeps that place on that display while it remains fully visible. Drag the search bar edge or search icon to move it; the input remains editable. This uses Tauri's built-in drag region and grants only the window drag permission. Its system colours follow light/dark mode and high contrast; text and controls wrap at larger text sizes. Enter still copies and hides, Shift+Enter selects, and Escape hides. Copy confirmations expire after 2.4 seconds; copy warnings stay until dismissed. Library controls, output format and Quit remain in the footer.

Visual evidence for #88 uses synthetic references: [browser light](images/picker-browser-light.jpg), [browser dark](images/picker-browser-dark.jpg), and [native macOS dark](images/picker-macos-dark.jpg). These previews predate the review fixes to contrast, focus, window edges and the first-run chooser. The browser checks also covered forced colours and 200% text at a 360-pixel viewport (one vertical scroller keeps controls reachable without horizontal overflow). Theme tokens retain system-colour fallbacks for older system webviews. The native screenshot is a macOS development build; Windows/Linux native screenshots and release-build blur checks remain to be recorded before #88 is complete.

## Desktop command contract

`apps/desktop/src/bindings.ts` is generated from Rust command signatures and
Serde-aware type definitions. `src-tauri/src/bindings.rs` registers the commands
once for both the runtime handler and Tauri Specta's exporter. After changing a
command or a wire type, regenerate with:

```sh
cargo test -p ibis-desktop bindings::tests::export_contract -- --ignored
```

The normal check script and desktop CI compare a fresh temporary export with the
checked-in file and fail if it is stale; checks never rewrite it. `api.ts` keeps
event adapters, UI labels and the search limit default. Its format-label map is
exhaustive over the generated Rust enum. The core's optional `type-export`
feature enables schema derives only, without Tauri or other GUI dependencies.

Tauri Specta is used because it derives wrappers as well as types, removing the
duplicate command contract without a handwritten schema or generator. Its Tauri
2 support is currently a prerelease, so Specta and Tauri Specta are pinned to
matching exact versions as upstream requires. All added dependencies pass the
existing licence and advisory policy.

## Commit messages

[Conventional Commits](https://www.conventionalcommits.org/): `feat:`, `fix:`, `perf:`, `docs:`, `test:`, `refactor:`, `ci:`, `chore:`. Breaking changes use `!` (`feat!:`) or a `BREAKING CHANGE:` footer. Commits are signed off (`git commit -s`).

## Versions and releases

- [Semantic Versioning](https://semver.org/). While the version is `0.x`, features and fixes raise the patch version (`0.1.3`), and completing a roadmap milestone raises the minor version (`0.2.0`), so version numbers match `ROADMAP.md`. A milestone release is requested by adding `Release-As: 0.N.0` to the body of a commit on `main`.
- [release-please](https://github.com/googleapis/release-please) keeps a release pull request open on GitHub. Merging it updates `CHANGELOG.md` and the version in `Cargo.toml`, creates the tag and the GitHub release. Versions are never edited by hand.
- Release builds are produced only by CI from a tag. Preview builds may be published as GitHub pre-releases.
- Every push to `main` also replaces the rolling `nightly` pre-release, so the newest merged state can always be downloaded and tried.
- Data formats that users keep (settings, destination profiles, history) carry a schema version and a tested migration.

## Workflow linting

Install [actionlint](https://github.com/rhysd/actionlint/blob/v1.7.12/docs/install.md) and make it available on `PATH`. Standard options include `brew install actionlint` on macOS, `go install github.com/rhysd/actionlint/cmd/actionlint@v1.7.12` with Go installed, or the official release binaries for Linux, macOS and Windows. CI uses the official 1.7.12 Docker image pinned by digest in the required Dependency policy job; updates should review the release and refresh that pin together.

Run `actionlint -shellcheck= -pyflakes=` to validate every workflow's GitHub-specific contexts, expressions and action usage. The full check uses the same flags; optional shellcheck and pyflakes integrations are disabled so installed auxiliary tools do not change the result. Workflow checks read files without editing them.

## Dependencies

`Cargo.lock` is committed. The Rust toolchain is pinned in `rust-toolchain.toml`. Dependabot proposes grouped updates monthly; each is reviewed and merged only with passing checks. `cargo deny` enforces license compatibility and blocks known vulnerabilities.

## Definition of done

- Acceptance criteria are covered by automated tests (or, for platform behaviour that cannot be automated yet, a recorded manual check in the pull request).
- Documentation, `ROADMAP.md` and snapshots are updated.
- No private data in code, tests, logs or text.
