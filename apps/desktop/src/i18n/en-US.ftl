# Interface messages only. Citation language and text direction have separate owners.
# Settings interface copy; native guidance and key notation keep their owners.
settings-pages = Settings pages
settings-general = General
settings-shortcuts = Shortcuts
settings-about = About Ibis

general-login-label =
    { $platform ->
        [macos] Open at Login
        [windows] Start when you sign in
       *[linux] Start on Login
    }
general-login-description = Keeps the shortcut ready after a restart
general-login-place =
    { $platform ->
        [macos] System Settings > General > Login Items
        [windows] Settings > Apps > Startup
       *[linux] your desktop's startup applications settings
    }
general-login-when =
    { $platform ->
        [windows] sign in
       *[other] log in
    }
general-login-unsupported = This system cannot start { $name } when you { general-login-when }. You can add it in { general-login-place }.
general-login-needs-approval = macOS is waiting for your approval. Turn on { $name } in { general-login-place }.
general-login-refused = The system did not let { $name } change this. You can change it in { general-login-place }.
general-login-unreadable = { $name } could not check whether it starts when you { general-login-when }. Look in { general-login-place }.

general-menu-bar-label = Show in Menu Bar
general-menu-bar-description = If hidden, open Ibis from Applications to come back here
general-paste-title = Pasting
general-paste-label = Paste into the app you came from
general-paste-key =
    { $platform ->
        [macos] ⌘V
       *[other] Ctrl+V
    }
general-paste-description = Otherwise Ibis copies and you press { general-paste-key }
# $list is the system's permission-list name, not an interface message code.
general-paste-allowed = Allowed in Privacy & Security › { $list }
general-paste-allowed-note = Ibis presses ⌘V once, only in the app you opened it from.
general-paste-guide-title = Allow Ibis to paste for you
general-paste-guide-why = To paste safely, Ibis needs Accessibility access to check which window has the keyboard, as well as permission to press ⌘V. It never reads what you type.
general-paste-step-open = Click Open System Settings.
general-paste-step-allow = In Privacy & Security › { $list }, turn on { $name }.
general-paste-step-return = Return to your document and open Ibis again. Press Enter to try again.
general-paste-pane = Privacy & Security › { $list }
general-paste-open = Open System Settings
general-paste-drag = Drag { $name } to { $list }
general-paste-drag-keyboard = Press to open System Settings.
general-paste-drag-fallback = If Ibis is not in the list, drag it there.
general-paste-drag-failed = Couldn't drag this copy of Ibis. Use the + button in System Settings to add it.
general-paste-waiting = Waiting for your OK…
general-paste-unreadable = Ibis could not read this permission, so pasting stays off until it can.
general-paste-granted = Ibis can paste now.
general-paste-return-failed = Couldn't return to your citation. Open Ibis with your shortcut to continue.
general-paste-stale = Already on, but still waiting? In System Settings, remove { $name } from the list, add this copy again, then turn it on.

general-rows = Rows
general-density-compact = Compact
general-density-comfortable = Comfortable

shortcuts-open = Open Ibis
shortcuts-description = Click the shortcut, then type a new one. Esc cancels.
shortcuts-record = Type a shortcut…
shortcuts-use-default = Use Default
shortcuts-change = Change…
shortcuts-not-assigned = Not assigned
shortcuts-configure-failed = The desktop could not open its shortcut settings. Try again.
shortcuts-failed = Ibis could not change the shortcut. Try again.
shortcuts-read-failed = Ibis could not check the shortcut. Try reopening Settings.
shortcuts-refusal-invalid = Ibis cannot use that key combination. Try another shortcut.
shortcuts-refusal-needs-modifier = Include Control, Alt or Command. Shift alone is not enough.
shortcuts-refusal-app-shortcut = That shortcut is used for app commands. Add another modifier.
shortcuts-refusal-alt-gr = Ctrl+Alt with a typing key is used for AltGr on Windows. Try another shortcut.
shortcuts-refusal-in-use = Another app or the system may be using that shortcut. Try another.
shortcuts-refusal-unavailable = Global shortcuts are unavailable on this desktop. Open Ibis from your application launcher.
shortcuts-refusal-not-saved = The shortcut works now, but Ibis could not save it for next time. Try again.
shortcuts-command-format = Change Format
shortcuts-command-pin = Keep Window Open
shortcuts-command-keys = Keyboard Shortcuts
shortcuts-command-more = More
shortcuts-command-change-last = Change Last Citation
shortcuts-command-source = Open the Source
shortcuts-command-settings = Settings
shortcuts-command-quit = Quit Ibis
shortcuts-picker-conflict = That shortcut is used for { $command }. Try another shortcut.
shortcuts-region-search = While searching
shortcuts-region-library = Your library
shortcuts-region-picker = Anywhere in Ibis
shortcuts-region-menu = In a menu
shortcuts-region-system = From any app
shortcuts-action-change-last = Change the last citation
shortcuts-action-source = Open the source
shortcuts-action-choose-source = Choose a library
shortcuts-action-reveal-source = Show the source in its folder
shortcuts-action-copy = Paste the citation
shortcuts-action-copy-only = Copy the citation
shortcuts-action-add = Add another reference
shortcuts-action-move = Move through results
shortcuts-action-escape = Step back or hide
shortcuts-action-format = Change the format
shortcuts-action-more = More actions
shortcuts-action-pin = Keep the window open
shortcuts-action-keys = Show this list
shortcuts-action-settings = Open Settings
shortcuts-action-quit = Quit Ibis
shortcuts-action-menu-move = Move through the menu
shortcuts-action-menu-ends = Go to the first or last item
shortcuts-action-menu-choose = Choose the item
shortcuts-action-menu-leave = Leave the menu and return to the search
shortcuts-or = or

about-automatic = Check for updates automatically
about-automatic-description = Reads a small update file from GitHub a few times a day
about-check = Check now
about-checking = Checking…
about-check-failed = Ibis could not check for updates. Try again.
about-check-newest = You have the newest version.
about-check-busy = Ibis is already checking, or an update is waiting for your answer.
about-copied = Copied
about-copy-failed = The version could not be copied.
about-version = Version { $version }
about-mission = One shortcut to cite from your library in any app.
about-updates = Updates
about-links = Links
about-website = Website
about-source = Source Code
about-problem = Report a Problem
about-licences = Licences
about-free = Free software under the AGPL 3.0
about-unavailable = This build does not update itself.


# Settings window pages and visual choices.
settings-look = Look
settings-citations = Citations
settings-search = Search Settings
settings-no-results = No matching settings
settings-results = Search results
settings-results-count = { $count ->
    [one] 1 result
   *[other] { $count } results
    }
settings-read-failed = Ibis could not read your settings; reopen Settings to try again.
settings-choice-failed = Ibis could not save this choice; the saved value is shown.
settings-startup-words = startup launch restart login
settings-paste-words = paste copy clipboard Enter
settings-appearance-words = dark light theme system
look-appearance = Appearance
look-appearance-description = Follow the system, or choose Light or Dark for Ibis.
look-system = System
look-light = Light
look-dark = Dark
look-accent = Accent
look-density-compact = Title and authors on one line
look-density-comfortable = Title, then authors and journal
look-accent-system =
    { $platform ->
        [macos] Follow the Mac
       *[other] Follow the system
    }
look-accent-ibis = Ibis scarlet
look-accent-words = colour color scarlet red
look-window-size = Window size
look-window-small = Small
look-window-medium = Medium
look-window-large = Large
look-window-custom = Custom
look-window-custom-note = Drag an edge of the picker to set your own size. Ibis remembers it on each display.
look-text-size = Text size
look-default-text = Default
look-larger-text = Larger
look-sample-title = The quiet garden
look-sample-authors = Mira Vale and Noah Reed
look-sample-journal = Journal of Imagined Studies
citations-default = Default format

citations-default-description = Used when an app has no remembered format
citations-recent = Recent citations
citations-recent-description = Shows what you cited recently in each app. Turning this off forgets them
citations-recent-words = history recents cited here forget privacy
citations-editor = Editor endpoint
citations-editor-description = Lets an editor on this computer open the picker and receive the citation you choose, through Better BibTeX's Cite as You Write. Web pages can't use it, and nothing leaves this computer.
citations-editor-words = cayw cite as you write vs code vscode better bibtex zotero endpoint port url editor
citations-editor-address = Address for your editor
citations-editor-address-description = Paste this into the editor's Cite as You Write URL setting. Anyone with it can open the picker on this computer; switching off and on makes a new one.
citations-editor-copy = Copy Address
citations-editor-copied = Copied
citations-editor-port-in-use = Another program is using the endpoint's port, so it is off.
citations-editor-bad-setting = The saved endpoint can't be used. Switch it off and on to make a new address.
citations-editor-not-saved = The endpoint could not be changed. Try again.
citations-format-pandoc = Pandoc / Quarto
citations-format-autocite = BibLaTeX \autocite
citations-format-parencite = BibLaTeX \parencite
citations-format-textcite = BibLaTeX \textcite
citations-format-cite = LaTeX \cite
citations-format-typst = Typst
citations-format-keys = Citation keys
citations-format-apa = APA (7th edition)
citations-format-chicago-author-date = Chicago (author-date)
citations-format-harvard-cite-them-right = Harvard (Cite Them Right)
citations-format-ieee = IEEE reference entry
citations-format-modern-language-association = MLA (9th edition)

settings-shortcut-restored = Default shortcut restored


settings-read-only = The settings file is read-only; the saved value is shown.

citations-key-formats = Citation keys
citations-style-formats = Citation styles
citations-filter-formats = Filter formats
look-sample-title-2 = Paths through the orchard
look-sample-title-3 = A map of quiet places

# Quiet feedback, counted on this device only.
delight-ibis = Ibis
delight-tip = { $keys } keeps Ibis open between citations.
delight-library100 = 100 references. A good start.
delight-library1000 = 1,000 references. A solid library.
delight-library10000 = 10,000 references. A remarkable library.
delight-empty = Choose a reference to paste.
delight-loading = Your library is still opening.
delight-no-library = Open a library to paste a reference.
delight-zoom = { $percent }%
delight-citation = { $count ->
    [1] First citation
   *[other] Citation { NUMBER($count) }
    }
delight-anniversary = One year with Ibis today.
delight-about-words = ibis version about thoth
canvas-hovered = Hovered
canvas-pressed = Pressed
canvas-focused = Keyboard focus

general-small-surprises-label = Small surprises
general-small-surprises-description = Bird facts, milestones and anniversaries.
settings-surprises-words = delight easter egg fun
delight-name = Named for the bird ancient Egyptians linked with Thoth, god of writing and keeper of records.
delight-fact-touch = White ibises find food by touch as they probe the mud.
delight-fact-scarlet = Scarlet ibises get their colour from pigments in the shellfish they eat.
delight-fact-mummies = Ancient Egyptians mummified ibises as offerings to Thoth.
delight-fact-national = The scarlet ibis is a national bird of Trinidad and Tobago.
delight-fact-flight = Northern bald ibises take turns leading their V formation.
undo-action = Undo { $action }
undo-remove = Remove { $reference }
undo-reset-window = Reset Window Position
undo-shortcut = Restore Default Shortcut

undo-removed = Removed { $reference }

choice-use-zotero = Use Zotero
choice-zotero-help = Your Zotero library, open or closed
choice-library-file = Choose a Library File…
choice-library-file-help = BibTeX, BibLaTeX or CSL-JSON
choice-drop-file = Or drag a .bib or CSL-JSON file into this window.
shelf-references = { $count } references
keys-title = Keyboard Shortcuts
keys-close = Close
keys-action = Action
keys-shortcut = Shortcut
shortcuts-filter = Filter shortcuts
shortcuts-global = Open Ibis (global)
shortcuts-system = System
shortcuts-empty = No matching actions

choice-use-file = Use { $name }
choice-back = Choose Another Source

backup-words = backup restore export import move migrate new computer sync iCloud OneDrive
backup-title = Another computer
backup-label = Back Up Settings
backup-description = A file to restore on another computer. Paste approvals and window places stay on this one.
backup-button = Back Up…
backup-saved = Saved to { $file }.
restore-label = Restore Settings
restore-description = Use a backup's settings here. Ibis shows what changes first and keeps the current settings as settings.json.bak.
restore-button = Restore…
restore-from = Backup from { $system }, made by Ibis { $version }
restore-same = This computer already has these settings.
restore-confirm = Restore and Restart
restore-cancel = Cancel
restore-restarting = Restarting Ibis with the restored settings…
backup-change = { $setting }: { $from } → { $to }
backup-note-library = The backup's bibliography is not on this computer. It is kept, and Ibis finds it once the file is there.
backup-note-shortcut = The backup has no opening shortcut for this system, so this computer's stays.
backup-note-repairs = The settings file on this computer is damaged or missing. Restoring writes it again, and keeps any damaged file beside it.
backup-setting-library = Library
backup-setting-copy-format = Key format
backup-setting-citation-style = Citation style
backup-setting-larger-text = Larger text
backup-setting-shortcut = Opening shortcut on { $system }
backup-value-default = Default
backup-value-on = On
backup-value-off = Off
backup-value-zotero = Zotero, My Library
backup-value-zotero-many = { $count } Zotero libraries
backup-value-zotero-group = Zotero group { $id }
backup-problem-not-downloaded = The backup isn't downloaded to this computer yet. Let your cloud folder download it, then try again.
backup-problem-unreadable = Ibis couldn't read that file. Nothing changed.
backup-problem-not-a-backup = That file isn't an Ibis settings backup. Nothing changed.
backup-problem-settings-file = That's Ibis's own settings file, not a backup. Choose a file made with Back Up Settings.
backup-problem-newer = That backup is from a newer version of Ibis. Update Ibis, then try again.
backup-problem-invalid = That backup is damaged. Nothing changed.
backup-problem-settings-unusable = Ibis can't use its settings file as it is. Nothing changed.
backup-problem-changed = Settings changed after the backup was shown. Choose it again.
backup-problem-not-saved = Ibis couldn't save. Nothing changed.
