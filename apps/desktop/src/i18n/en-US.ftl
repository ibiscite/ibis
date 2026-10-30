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
general-login-description = Keeps the shortcut ready after you restart
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

general-paste-title = Pasting
general-paste-label = Paste into the app you came from
general-paste-description = Enter pastes the citation where you were typing. Off, Enter only copies it.
# $list is the system's permission-list name, not an interface message code.
general-paste-allowed = Allowed in Privacy & Security › { $list }
general-paste-allowed-note = Ibis presses ⌘V once, only in the app you opened it from.
general-paste-guide-title = Allow Ibis to paste for you
general-paste-guide-why = To paste, allow Ibis in macOS; it only presses ⌘V in the app you came from and never reads what you type.
general-paste-step-open = Click Open System Settings.
general-paste-step-allow = In Privacy & Security › { $list }, turn on { $name }.
general-paste-step-return = Come back. Ibis notices on its own.
general-paste-pane = Privacy & Security › { $list }
general-paste-open = Open System Settings
general-paste-drag = Drag { $name } to { $list }
general-paste-drag-keyboard = Press to open System Settings.
general-paste-drag-fallback = If Ibis is not in the list, drag it there.
general-paste-drag-failed = Couldn't drag this copy of Ibis. Use the + button in System Settings to add it.
general-paste-waiting = Waiting for your OK…
general-paste-granted = Ibis can paste now.
general-paste-return-failed = Couldn't return to your citation. Open Ibis with your shortcut to continue.
general-paste-stale = Already on, but still waiting? An update gave { $name } a new identity. Open System Settings again: Ibis clears the old switch so you can turn on the new one.

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
shortcuts-in-picker = In the picker
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
shortcuts-command-settings = Settings
shortcuts-command-quit = Quit Ibis
shortcuts-picker-conflict = That shortcut is used for { $command }. Try another shortcut.
shortcuts-region-search = In the search box
shortcuts-region-picker = Anywhere in Ibis
shortcuts-region-menu = In a menu
shortcuts-region-system = From any app
shortcuts-action-preview = Show what will be pasted
shortcuts-action-copy = Paste or copy the highlighted reference, or the ones added to the citation, as the footer says
shortcuts-action-add = Add the highlighted reference to the citation, or remove it
shortcuts-action-move = Move through the results
shortcuts-action-escape = Step back: close a menu, clear the search, then hide Ibis
shortcuts-action-format = Choose the output format and where to remember it
shortcuts-action-more = Open or close More
shortcuts-action-pin = Keep Window Open, or stop keeping it open
shortcuts-action-keys = Show this list (? works outside the search box)
shortcuts-action-settings = Open Settings
shortcuts-action-quit = Quit Ibis
shortcuts-action-menu-move = Move through the menu
shortcuts-action-menu-ends = Go to the first or last item
shortcuts-action-menu-choose = Choose the item
shortcuts-action-menu-leave = Leave the menu and return to the search
shortcuts-or = or

about-updates = Updates
about-automatic = Check for updates automatically
about-automatic-description = Reads a small update file from GitHub a few times a day
about-check = Check now
about-version = { $name } { $version }
about-unavailable = This build does not update itself.


# Settings window pages and visual choices.
settings-look = Look
settings-libraries = Libraries
settings-citations = Citations
settings-advanced = Advanced
settings-search = Search Settings
settings-no-results = No matching settings
settings-coming-soon = Coming soon
settings-read-failed = Ibis could not read your settings; reopen Settings to try again.
settings-choice-failed = Ibis could not save this choice; the saved value is shown.
settings-startup-words = startup launch restart login
settings-paste-words = paste copy clipboard Enter
settings-appearance-words = dark light theme system
general-menu-bar = Show in Menu Bar
general-menu-bar-description = When hidden, your shortcut still opens Ibis; open Ibis from Applications to return to Settings.
general-welcome = Show the welcome again
look-appearance = Appearance
look-appearance-description = Follow the system, or choose Light or Dark for Ibis.
look-system = System
look-light = Light
look-dark = Dark
look-accent = Accent
look-accent-description = Applies inside Ibis only.
look-accent-system = Follow the Mac
look-accent-blue = Blue
look-accent-purple = Purple
look-accent-pink = Pink
look-accent-red = Red
look-accent-orange = Orange
look-accent-yellow = Yellow
look-accent-green = Green
look-accent-graphite = Graphite
look-window-size = Window size
look-small = Small
look-medium = Medium
look-large = Large
look-compact-window = Compact · Coming soon
look-custom = Custom
look-search-icon = Search icon
look-ibis = Ibis mark
look-magnifier = Magnifier
look-none = None · Coming soon
look-text-size = Text size
look-default-text = Default
look-larger-text = Larger
look-largest-text = Largest · Coming soon
look-sample-title = The quiet garden
look-sample-authors = Mira Vale and Noah Reed
look-sample-journal = Journal of Imagined Studies
libraries-sources = Library sources and status
libraries-search-in = Search in Zotero libraries
libraries-add = Add Library…
libraries-remove = Remove Library
citations-default = Default format
citations-per-app = Remember the format for each app
citations-printed = Use printed page numbers
citations-forget = Remembered formats · Forget
citations-styles = Styles in the format menu
shortcuts-choose = Choose shortcuts from a list
advanced-placement = Where Ibis opens
advanced-reset-position = Reset Window Position
advanced-file = Settings file · Open or Show in Finder
advanced-command = Install the ibis command
advanced-report = Copy a problem report
advanced-reset = Reset All Settings
about-report = Report a problem
about-suggest = Suggest an idea
about-licences = Licences

citations-default-description = Used when an app has no remembered format
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

settings-undo = Undo
settings-shortcut-restored = Default shortcut restored

general-menu-bar-unavailable = { settings-coming-soon }. { general-menu-bar-description }

settings-read-only = The settings file is read-only; the saved value is shown.

citations-key-formats = Citation keys
citations-style-formats = Citation styles
citations-filter-formats = Filter formats
look-sample-title-2 = Paths through the orchard
look-sample-title-3 = A map of quiet places
