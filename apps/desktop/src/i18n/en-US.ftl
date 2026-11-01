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
general-paste-description = Otherwise Ibis copies and you press ⌘V
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
general-paste-granted = Ibis can paste now.
general-paste-return-failed = Couldn't return to your citation. Open Ibis with your shortcut to continue.
general-paste-stale = Already on, but still waiting? In System Settings, remove { $name } from the list, add this copy again, then turn it on.

general-rows = Rows
general-density-compact = Compact
general-density-comfortable = Comfortable
general-density-spacious = Spacious
general-preview = Show what will be pasted
general-preview-description = The line above the footer

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
shortcuts-command-format = Choose Output Format
shortcuts-command-pin = Keep Window Open
shortcuts-command-keys = Keyboard Shortcuts
shortcuts-command-more = More
shortcuts-command-preview = Show What Will Be Pasted
shortcuts-command-source = Open the Source
shortcuts-command-settings = Settings
shortcuts-command-quit = Quit Ibis
shortcuts-picker-conflict = That shortcut is used for { $command }. Try another shortcut.
shortcuts-region-search = While searching
shortcuts-region-picker = Anywhere in Ibis
shortcuts-region-menu = In a menu
shortcuts-region-system = From any app
shortcuts-action-preview = Show what will be pasted
shortcuts-action-source = Open the source
shortcuts-action-choose-source = Choose a library
shortcuts-action-reveal-source = Show the source in its folder
shortcuts-action-copy = Paste the citation
shortcuts-action-add = Add another reference
shortcuts-action-move = Move through results
shortcuts-action-escape = Step back or hide
shortcuts-action-format = Choose the format
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
about-copied = Copied
about-copy-failed = The version could not be copied.
about-version = Version { $version }
about-mission = One shortcut to cite from your library in any app.
about-why = Why an ibis
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
look-accent-multicolor = Follow the Mac · blue when the Mac is on Multicolor
look-accent-inside = { $name } · for Ibis only
look-density-compact = Title and authors on one line
look-density-comfortable = Title, then authors and journal
look-density-spacious = Adds the kind of reference
look-accent-system = Follow the Mac
look-accent-ibis = Ibis scarlet
look-accent-blue = Blue
look-accent-purple = Purple
look-accent-pink = Pink
look-accent-red = Red
look-accent-orange = Orange
look-accent-yellow = Yellow
look-accent-green = Green
look-accent-graphite = Graphite
look-window-size = Window size
look-window-small = Small
look-window-medium = Medium
look-window-large = Large
look-window-custom = Custom
look-window-custom-note = Drag an edge of the picker to set your own size. Ibis remembers it on each display.
look-search-icon = Search icon
look-ibis = Ibis mark
look-magnifier = Magnifier
look-none = None
look-text-size = Text size
look-default-text = Default
look-larger-text = Larger
look-sample-title = The quiet garden
look-sample-authors = Mira Vale and Noah Reed
look-sample-journal = Journal of Imagined Studies
citations-default = Default format

citations-default-description = Used when an app has no remembered format
citations-forget = Forget formats for apps
citations-forget-description = Ibis asks again the next time you paste into each app
citations-forget-button = Forget formats
citations-forget-failed = Ibis could not forget formats for apps. Try again.
citations-forget-done = Formats forgotten for apps.
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
delight-thoth = Ancient Egyptians associated the ibis with Thoth, god of writing.
delight-tip = ⇧⌘P keeps Ibis open
delight-library100 = 100 references in { $library }. A good start.
delight-library1000 = 1,000 references in { $library }. A solid library.
delight-library10000 = 10,000 references in { $library }. A remarkable library.
delight-empty = Choose a reference to paste.
delight-loading = Your library is still opening.
delight-no-library = Open a library to paste a reference.
delight-zoom = { $percent }%
delight-citation = { $count ->
    [1] First citation
   *[other] Citation { NUMBER($count) }
    }
delight-anniversary = One year with Ibis
delight-about-words = ibis version about thoth
canvas-hovered = Hovered
canvas-pressed = Pressed
canvas-focused = Keyboard focus

general-small-surprises-label = Small surprises
general-small-surprises-description = Bird facts, milestones and anniversaries.
delight-name = Named for the bird ancient Egyptians linked with Thoth, god of writing and keeper of records.
delight-fact-touch = White ibises find food by touch as they probe the mud.
delight-fact-scarlet = Scarlet ibises get their colour from pigments in the shellfish they eat.
delight-fact-mummies = Ancient Egyptians mummified ibises as offerings to Thoth.
delight-fact-national = The scarlet ibis is a national bird of Trinidad and Tobago.
delight-fact-flight = Northern bald ibises take turns leading their V formation.
delight-birthday = Ibis turns two today.
undo-action = Undo { $action }
undo-remove = Remove { $reference }
undo-reset-window = Reset Window Position
undo-shortcut = Restore Default Shortcut

undo-removed = Removed { $reference } from citation.

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
