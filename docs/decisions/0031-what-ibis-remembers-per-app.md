# 0031 What Ibis remembers per app

2026-11-04 (maintainer, #1697): an optional writing project remembers its output
and optional checked bibliography together. Each app remembers its own selected
project; selecting No Writing Project restores that app's ordinary output.
The selected project's output takes precedence over the app's format, and a
format pick changes the project while it is selected. The picker continuously
names that project and offers a quick explicit switch. A project is chosen by
the person, never inferred from a document, window title or browser URL.
Selection grants no paste permission. Explicit editor and command-line requests
keep their own output and do not change the GUI selection. Project switches
replace format and bibliography together and retire pending copy authority;
failed saves keep the previous context whole. Search continues across connected
libraries; a project saves no search scope (2026-11-05, decision 0028). The checked bibliography remains one read-only file, not proof about
the actual document. Implementation and acceptance scenarios: #1697.

2026-11-05 (maintainer): a project is only a `.bib` file and its format, picked
from the format menu. There is no Apply Updates step: the next citation uses the
project as saved.

2026-11-02 (maintainer): with no writing project selected, Ibis remembers a
format per app, and the person changes it in the format menu. One default in
Settings › Citations applies to apps with no remembered format. "Just this
once", "Automatic", "Use as global default" and "Forget formats for apps" are
gone, along with their Undo; a later pick replaces the old one. A format is
remembered only for a captured app. When Ibis can't tell the app, the default
applies and a pick changes it, so no choice is lost; when the destination could
not be read, the app may be a real one, so a pick holds for that paste alone. A
pick that cannot be saved also holds for that paste, and the control says "This
paste". The app table's starting formats stay, as suggestions the menu names, so
a new install still pastes APA into Word.

`apps.json`, beside `settings.json`, keeps
only the operating system, normalised app ID, output, optional delivery choice
(`paste` or `copy-only`), the identity binding described below and whether the
one-time plugin note was shown, for at most 200 apps;
the least recently chosen app is dropped first. Never a document title, URL,
window title, document text or citation. Browser and editor IDs cannot identify
a project's syntax, so the first paste in each browser or code editor opens
the output chooser once and remembers the pick (maintainer, 2026-10-28).
Remembered choices beat app-table suggestions, then the
default; a format save and a delivery save each leave the other alone, except that
answering Paste or Copy only in an open browser or editor first-paste question
accepts the format it showed, unless a format is being held for that paste. A new default never erases app choices.

2026-10-31 (#1486): Recent citations keep only opaque reference identities and
known originating app IDs in the channel's WebView storage, bounded to 20 apps,
30 identities per app and 256 Ki characters. More can forget them. Unknown
destinations and sample references are not remembered. Rows resolve against the
current snapshot; no page, form, query or citation text is restored. Failed
history writes retain in-memory recents; a failed forget remains available for
retry. The Recent citations switch (`recent_citations`, written only when off)
forgets on off and records only while known to be on, never on an unread
setting; only a new receipt records, so Forget or turning it back on never
replays an older one. Each saved "off" forgets as it arrives, so off and
quickly on again still starts empty. BibTeX added dates come from `date-added`
(BibDesk), `creationdate` (JabRef), `timestamp` or `dateadded`, first valid
wins; values with an offset are shifted to UTC, and those without one (JabRef's
local time) are read as UTC, so cross-source order can be off by the offset. Recently added uses validated source-added dates, never publication years
or invented import times. These display dates do not change cite fingerprints.

The bounded file is written whole, flushed and swapped in atomically. A file
Ibis cannot read means suggested/default outputs and automatic paste is
unavailable: a lost Copy only rule must not become paste permission.
Subsequent writes preserve a recovery flag in `apps.json`, including
after restart. A format save never clears it. An explicit Paste or Copy only
choice resolves recovery for that app alone; every other app keeps the refusal
until its own delivery choice, with no global reset. The warning shows only for
an app that can answer, so it never sticks on a refused app or with no app. Its read failure
is frozen in the copy proof, so a later successful save cannot release that copy.
Unknown apps ask for delivery approval
separately from format, in the same chooser (0009); recognised apps also offer
Copy only there and in More for the current ordinary app. A failed save leaves the previous durable choices intact and says
so. Preference writes from different windows apply only deliberate field
patches under the native owner lock, returning the authoritative saved state.
Settings and the picker share one frontend patch queue for ordering, field
acknowledgments, stale reads and recovery. Settings restores the saved choice
after a failed save; the picker retains explicit choices for retry. Each window
keeps its own queue; the native lock coordinates windows (2026-10-28).

Unlisted-app Paste approval compares the normalised OS app ID and a SHA-256
fingerprint of that ID, platform and exact OS-reported app location. Windows
uses the full executable path already read by `QueryFullProcessImageName`;
macOS uses the bundle URL path on the same captured `NSRunningApplication`
record. Only the fingerprint is retained and sent through IPC; raw app paths
stay transient in capture. Identity binding adds no document, executable-content,
signing or filesystem reads. Path case
is preserved; paths are never canonicalised through filesystem reads. A missing
or changed saved binding requires approval again; unavailable capture identity
only copies. A delayed save must match the captured
fingerprint too. Format and Copy only remain keyed by app ID. Capturing no
signing team means replacement at exactly the same location remains
indistinguishable; normal updates at that location keep the choice.

App-ID evidence for the 2026-10-31 destination-table additions:

- Read `CFBundleIdentifier` and `CFBundleName` from the installed bundles for
  Claude (`com.anthropic.claudefordesktop`), ChatGPT (`com.openai.codex`, whose
  bundle name is ChatGPT), Notes (`com.apple.Notes`), Notion (`notion.id`), Craft
  (`com.lukilabs.lukiapp`), OneNote (`com.microsoft.onenote.mac`), Thunderbird
  (`org.mozilla.thunderbird`), Logseq (`com.logseq.logseq`), Jump Desktop
  (`com.p5sys.jump.mac.viewer`) and Termius (`com.termius-dmg.mac`). These are
  app metadata only, with no document or library reads.
- ChatGPT Classic's `com.openai.chat` is recorded in an
  [original application crash report](https://community.openai.com/t/chatgpt-app-for-macos-crashing-on-open-november-3/1002242).
  Parsec's `tv.parsec.www` is recorded in a
  [firsthand native-paste reproduction](https://github.com/Muesli-HQ/muesli/issues/493);
  its Windows `parsecd.exe` is in [Parsec's launch instructions](https://support.parsec.app/hc/en-us/articles/32381836182932-Run-Parsec-App-From-Command-Line).
  Both Jump Desktop Mac variants are named in
  [the vendor's configuration instructions](https://support.jumpdesktop.com/hc/en-us/articles/4408192384909-Mac-How-can-I-prevent-users-on-Jump-Desktop-for-Mac-from-saving-or-sending-computer-passwords).
- NoMachine's `com.nomachine.nxdock` comes from its
  [extracted 7.4.1 Info.plist](https://develop.doesitarm.com/app/nomachine/);
  Windows `nxplayer.exe` and `nxplayer.bin` appear in a
  [firsthand installed-file listing](https://forum.nomachine.com/archive/topic/trying-to-make-a-portable-version-of-nomachine-enterprise-client).
  Horizon's `com.omnissa.horizon.client.mac` is in its
  [recorded bundle ID and signing requirement](https://appcatalog.cloud/apps/omnissa-horizon-client);
  `vmware-view.exe` is in the [vendor's Windows client guide](https://docs-be.omnissa.com/bundle/Horizon-Client-for-Windows_2006_Horizon-Client-for-Windows-Installation-and-Setup-Guide/raw/resource/enus/Horizon_Client_for_Windows_2006_Horizon_Client_for_Windows_Installation_and_Setup_Guide.pdf?save_local=true).
- Windows Emacs and gVim use their real executable names from
  [Emacs's build source](https://github.com/emacs-mirror/emacs/blob/master/src/Makefile.in)
  and [Vim's build source](https://github.com/vim/vim/blob/master/src/Make_mvc.mak).
  Their default shortcuts differ: [Emacs clipboard commands](https://www.gnu.org/software/emacs/manual/html_node/emacs/Clipboard.html)
  and [Vim's Windows clipboard instructions](https://github.com/vim/vim/blob/master/runtime/doc/gui_w32.txt).
  Neovide's [bundle metadata](https://github.com/neovide/neovide/blob/main/Cargo.toml)
  and [Windows release workflow](https://github.com/neovide/neovide/blob/main/.github/workflows/nightly.yml)
  verify its IDs; its [FAQ](https://neovide.dev/faq.html) requires explicit
  paste mappings on macOS and Windows.

Not verified: legacy VMware Horizon Mac bundle IDs, separate NoMachine viewer
helper bundle IDs and Jump Desktop's Windows executable. No guessed entries
were added for these. The added table rows and per-app choice lookups are bounded;
no new file, network access, polling or hidden activity is introduced.

Upgrading from 0.2 (2026-11-07, decision 0016): 0.2 saved each app's format as
an object this Ibis does not read, so the defaults apply. The first time Ibis
finds one, it keeps `apps.json` whole as `apps.json.earlier`, before anything
rewrites it, and says once: "Format choices from an earlier Ibis could not be
read, so Ibis uses its defaults. Choose them again." Choosing a format ends the
notice. If the copy cannot be made, Ibis never rewrites `apps.json`. The 0.2
default format in `settings.json` stays in the file, unread, and is said the
same way until a default is chosen.
