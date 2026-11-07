# Ibis’s words, for the command line, agents (MCP) and the app (#1675).
# The engine returns codes; each code has one sentence here, which every front
# end shows. A `.cli`, `.agent` or `.app` attribute says what to do next in that
# front end. Ids come from the codes’ wire names, and a test fails on a code
# without words or words without a code. People’s words, not the engine’s
# (design principle 12), and typographic apostrophes.

## Engine errors: what went wrong, by code.

error-usage = The command is not valid.
    .cli = See: ibis --help
    .agent = The arguments do not fit this tool’s input schema; check the names, types, limits and combinations it allows.
error-bad-input = The input is not valid.
    .cli = See: ibis --help
    .agent = The arguments do not fit this tool’s input schema; check the names, types, limits and combinations it allows.
error-not-found = Nothing matched.
    .agent = Use a ref that search returned.
error-source-unavailable = A library could not be read just now.
    .cli = Try again.
    .agent = Try again shortly.
    .app = Try again.
error-resource-limit = The request or its result is too large.
    .cli = Ask for less.
    .agent = Ask for fewer results.
error-newer-format = A newer version of Ibis wrote this data. Update Ibis.
error-internal = Ibis could not finish this.
    .cli = Please report it at https://github.com/ibiscite/ibis/issues
    .agent = Try again.
    .app = Try again.
error-cancelled = The request was cancelled.

## Why, when the code alone cannot say.

reason-unknown-style = No style has that name.
    .cli = List the styles with: ibis styles
    .agent = The styles tool lists them.
reason-unknown-output = No output has that id.
    .cli = List them with: ibis outputs
    .agent = Use a format from this tool’s input schema.
reason-unknown-locale = That citation language is not available.
    .cli = List them with: ibis styles --locales
    .agent = The styles tool lists the locales.
reason-duplicate-key = The library gives this citation key to more than one entry, so Ibis skips them all. Give each entry its own key.
reason-ambiguous-key = Several references use this citation key.
    .cli = Give each one its own key in your library.
    .agent = Use the ref that search returned.
    .app = Give each one its own key in your library.
reason-missing-key = This reference has no citation key.
    .cli = Give it one in your library, or use --style.
    .agent = Use a style, or ask the person to give it one.
    .app = Give it one in your library, or choose a style.
reason-app-missing = The Ibis app was not found.
    .cli = Install it, or give its location with --app <path>.
reason-not-ibis = The program given as the Ibis app is not Ibis.
    .cli = Give the Ibis app’s location with --app <path>.
reason-no-answer = The Ibis app started but did not answer.
    .cli = Open Ibis once by hand, then try again.
reason-zotero-sharing-off = Zotero is open but is not sharing your library with other apps.
    .cli = In Zotero, open Settings, Advanced, and turn on Allow other applications on this computer to communicate with Zotero.
    .agent = Ask the person to turn on Allow other applications in Zotero’s Advanced settings.
reason-zotero-profile-needed = Zotero is closed, and Ibis needs to know which of its profiles to read.
    .cli = Open Zotero, or name one with --zotero-profile NAME.
    .agent = Ask the person to open Zotero, or to choose a profile in Ibis or pass --zotero-profile to ibis mcp.
    .app = Open Zotero, or choose a profile.
reason-zotero-needs-opening = Your Zotero library cannot be read while Zotero is closed.
    .cli = Open Zotero, then try again.
    .agent = Ask the person to open Zotero.
reason-zotero-not-found = No Zotero library was found on this computer.
    .cli = Check that Zotero is installed and has been opened once, or use --file.
reason-zotero-no-space = There is not enough free space to read Zotero while it is closed.
    .cli = Free some space, or open Zotero.
    .app = Free some space, or open Zotero.

## A reference a citation could not include. { $reference } names it.

refusal-empty-selection = There is nothing to cite.
refusal-too-many-items = One citation can hold at most { $limit } references.
refusal-invalid-locator = The page or location for { $reference } is empty or has control characters.
refusal-invalid-rich-text = The text before or after { $reference } has characters Ibis cannot write.
refusal-output-too-large = The citation is too large to write.
refusal-unsupported-form = { $output } cannot write { $reference } in this form or with these options.
    .cli = Cite it on its own, or choose another output.
    .agent = Try form normal, or another format or style.
    .app = Choose Name in brackets, cite it alone, or choose another output.
refusal-missing-item = { $reference } changed or was removed while citing.
    .cli = Try again.
    .agent = Search again.
    .app = Search again.
refusal-changed-item = { $reference } changed while citing.
    .cli = Try again.
    .agent = Search again.
    .app = Search again.
refusal-missing-key = { $reference } has no citation key.
    .cli = Cite it with --style instead.
    .agent = A key syntax cannot name it; use a style.
    .app = Give it one in your library, or choose a style.
refusal-ambiguous-key = Several references share the citation key of { $reference }.
    .cli = Give each one its own key in your library.
    .agent = Use a style, or ask the person to give each its own key.
    .app = Give each one its own key in your library.
refusal-conflicting-key = { $reference } has more than one citation key in its library.
    .cli = Keep one.
    .agent = Use a style, or ask the person to keep one key.
    .app = Keep one in your library, or choose a style.
refusal-invalid-key = The citation key of { $reference } has characters { $output } cannot write.
    .agent = Use a style, or ask the person to change the key.
refusal-unsupported-output = There is no style { $output }.
    .cli = List the styles with: ibis styles
    .agent = The styles tool lists them.
refusal-invalid-locale = That citation language is not available.
    .cli = List them with: ibis styles --locales
    .agent = The styles tool lists the locales.
refusal-cancelled = The citation was cancelled.
refusal-formatter-unavailable = The citation could not be written just now. Try again.
refusal-annotation-mismatch = The annotation given for { $reference } belongs to another reference.
refusal-not-citable = { $reference } is not a reference, such as a note, so it cannot be cited.

## What a citation could not say in full.

warning-source-field = A field of { $reference } could not be fully converted. Check it in your library.
warning-sentence-case-expected = APA expects sentence case for the title of { $reference }. Its capitalization was kept as stored, so check it in your document.
loss-locator = the page or location
loss-prefix = the text before
loss-suffix = the text after
loss-author-suppression = the omitted author
loss-rich-text = text formatting
loss-order = the order of the references
notice-needs-document-context = This style numbers citations or places notes across the whole document, which Ibis does not see. Check the result in your document.
notice-reference-entry = This is the full reference entry.
notice-full-first-note = This is the full form a first note uses; later notes to the same work are usually shorter.
notice-year-suffix-withheld = Two of these works share author and year. Add the year letters (such as 2026a and 2026b) in your document, because they depend on its whole reference list.
notice-input-language-unsupported = Ibis does not recognize locator words in this language. Give the page or location in English, such as “p. 12”.

## Citation forms, by their one name.

form-normal = Name in brackets
form-narrative = Name in the sentence
form-author-only = Name only
form-year-only = Year only
form-full = Full reference
form-bibliography-only = Reference list

## The command line’s own sentences about a refusal.

cli-form-refused = Cannot cite { $reference } in { $output } with --form { $form }. Try --form normal.
cli-options-refused = Cannot cite { $reference } with { $options } in { $output }. Try without them.
cli-together-refused = { $output } cannot write these references together as asked. Cite them one at a time to see which one fails.
cli-loss-left-out = { $output } cannot show { $what }{ $target }, so it was left out.
cli-loss-flattened = { $output } wrote { $what }{ $target } as plain text.
cli-project-without-bibliography = no .bib file
cli-doctor-about = Check your libraries and settings offline, without writing or sending anything

## The app’s own sentence about a citation.

app-loss = This output cannot fully show { $what }{ $target }.
app-tray-open = Open Ibis
app-tray-open-shortcut = Open Ibis ({ $shortcut })
app-saved-title = Save References Cited Since Opening
app-saved-file = Cited since opening
app-saved-page = References cited since opening

## The app’s own words when a library cannot be read or a command fails.
# Most are joined into a longer line, so they start in lower case.

app-library-file-unavailable = the library file is unavailable; download any online-only file to this computer first
app-library-file-invalid = the library file does not contain valid reference data
app-library-file-too-large = the library file is larger than 64 MiB
app-library-file-unfinished = the library file contains an unfinished entry
app-library-work-limit = the library exceeded the load work limit
app-library-file-not-utf8 = the library file is not UTF-8 text; save it as UTF-8
app-library-zotero-sharing-off = Zotero is open but is not sharing your library with other applications
app-library-zotero-profile-needed = choose which Zotero profile to read
app-library-zotero-closed = Zotero’s library cannot be read while it is closed
app-library-zotero-not-found = no Zotero library was found on this computer
app-library-zotero-no-space = there is not enough free space to read Zotero while it is closed; free some space or open Zotero
app-library-zotero-cancelled = reading Zotero was cancelled
app-library-zotero-work-limit = reading Zotero exceeded its work limit
app-library-zotero-newer = this Zotero database is newer than Ibis can read
app-library-zotero-group-not-found = a chosen Zotero group library was not found; check that you are still a member
app-library-zotero-try-again = Zotero could not be read just now; try again
app-library-not-local = the chosen file is not on this computer
app-library-stopped = loading the library stopped unexpectedly
# $error is one of the sentences above.
app-library-kept = { $error }; keeping the last loaded references
# $name is the library’s name; $error is one of the sentences above.
app-library-retained-not-updated = the retained library { $name } could not be updated: { $error }
app-library-not-refreshed = The library could not be refreshed; keeping the last loaded references.
app-library-retained-not-refreshed = The retained library could not be refreshed; keeping the last loaded references.
app-library-state-unavailable = library state is unavailable
app-command-snapshot-changed = library snapshot changed
app-command-library-loading = library is loading
app-command-cancelled = cancelled
app-command-shared-citation-key = the citation key `{ $key }` belongs to more than one reference
app-command-not-prepared = The citation could not be prepared. Try again.
app-command-source-not-opened = The source could not be opened. Try again.

## The app’s interface.

# Citation language and text direction have separate owners.
# Settings interface copy; native guidance and key notation keep their owners.
settings-pages = Settings pages
settings-general = General
settings-shortcuts = Shortcuts
settings-about = About Ibis

general-login-label =
    { $platform ->
        [macos] Open at login
        [windows] Start when you sign in
       *[linux] Start on login
    }
general-login-description = Keeps the shortcut ready after a restart.
general-login-place =
    { $platform ->
        [macos] System Settings > General > Login Items
        [windows] Settings > Apps > Startup
       *[linux] your desktop’s startup applications settings
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

general-menu-bar-label = Show in menu bar
general-menu-bar-description = If hidden, open Ibis from Applications to come back here.
general-paste-title = Pasting
general-paste-label = Paste into the app you came from
general-paste-key =
    { $platform ->
        [macos] ⌘V
       *[other] Ctrl+V
    }
general-paste-description = Otherwise Ibis copies and you press { general-paste-key }.
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
general-paste-drag-failed = Couldn’t drag this copy of Ibis. Use the + button in System Settings to add it.
general-paste-waiting = Waiting for your OK…
general-paste-unreadable = Ibis could not read this permission, so pasting stays off until it can.
general-paste-granted = Ibis can paste now.
general-paste-return-failed = Couldn’t return to your citation. Open Ibis with your shortcut to continue.
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
shortcuts-region-system = Open Ibis
shortcuts-region-citation = In the citation
shortcuts-action-change-last = Edit the citation you just pasted
shortcuts-action-source = Open the source
shortcuts-action-choose-source = Choose a library
shortcuts-action-reveal-source = Show the source in its folder
shortcuts-action-open = From any app
shortcuts-action-copy = Paste or copy the citation
shortcuts-action-paste = Paste the citation
shortcuts-action-copy-only = Copy the citation
shortcuts-action-add = Add another reference
shortcuts-action-move = Move through results
shortcuts-action-peek = Peek at the highlighted reference
shortcuts-action-escape = Step back or hide
shortcuts-action-format = Change the format
shortcuts-action-more = More actions
shortcuts-action-pin = Keep the window open
shortcuts-action-keys = Show keyboard shortcuts
shortcuts-action-settings = Open Settings
shortcuts-action-quit = Quit Ibis
shortcuts-action-undo = Undo
shortcuts-action-text-size = Make text larger or smaller
shortcuts-action-citation-move = Move between references
shortcuts-action-citation-remove = Remove the reference
shortcuts-action-menu-move = Move through the menu
shortcuts-action-menu-ends = Go to the first or last item
shortcuts-action-menu-choose = Choose the item
shortcuts-action-menu-leave = Leave the menu and return to the search
shortcuts-or = or

about-automatic = Check for updates automatically
about-automatic-description = Reads a small update file from GitHub a few times a day.
about-check = Check for Updates
about-check-label = Check for updates
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

citations-default-description = Used when an app has no remembered format.
citations-recent = Recent citations
citations-recent-description = Shows what you cited recently in each app. Turning this off forgets them.
citations-recent-words = history recents cited here forget privacy
citations-editor = Editor endpoint
citations-editor-description = Lets an editor on this computer open the picker and receive the citation you choose, through Better BibTeX’s Cite as You Write. Web pages can’t use it, and nothing leaves this computer.
citations-editor-words = cayw cite as you write vs code vscode better bibtex zotero endpoint port url editor
citations-editor-address = Address for your editor
citations-editor-address-description = Paste this into the editor’s Cite as You Write URL setting. Anyone with it can open the picker on this computer; switching off and on makes a new one.
citations-editor-copy = Copy Address
citations-editor-copied = Copied
citations-editor-port-in-use = Another program is using the endpoint’s port, so it is off.
citations-editor-bad-setting = The saved endpoint can’t be used. Switch it off and on to make a new address.
citations-editor-not-saved = The endpoint could not be changed. Try again.

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
shortcuts-empty = No matching actions

choice-use-file = Use { $name }
choice-back = Choose Another Source

backup-words = backup restore export import move migrate new computer sync iCloud OneDrive
backup-title = Another computer
backup-label = Back up settings
backup-description = A file to restore on another computer. Paste approvals and window places stay on this one.
backup-button = Back Up…
backup-saved = Saved to { $file }.
restore-label = Restore settings
restore-description = Use a backup’s settings here. Ibis shows what changes first and keeps the current settings as settings.json.bak.
restore-button = Restore…
restore-from = Backup from { $system }, made by Ibis { $version }
restore-same = This computer already has these settings.
restore-confirm = Restore and Restart
restore-cancel = Cancel
restore-restarting = Restarting Ibis with the restored settings…
backup-change = { $setting }: { $from } → { $to }
backup-note-library = The backup’s bibliography is not on this computer. It is kept, and Ibis finds it once the file is there.
backup-note-shortcut = The backup has no opening shortcut for this system, so this computer’s stays.
backup-note-repairs = The settings file on this computer is damaged or missing. Restoring writes it again, and keeps any damaged file beside it.
backup-setting-library = Library
backup-setting-larger-text = Larger text
backup-setting-shortcut = Opening shortcut on { $system }
backup-value-default = Default
backup-value-on = On
backup-value-off = Off
backup-value-zotero = Zotero, My Library
backup-value-zotero-many = { $count } Zotero libraries
backup-value-zotero-group = Zotero group { $id }
backup-problem-not-downloaded = The backup isn’t downloaded to this computer yet. Let your cloud folder download it, then try again.
backup-problem-unreadable = Ibis couldn’t read that file. Nothing changed.
backup-problem-not-a-backup = That file isn’t an Ibis settings backup. Nothing changed.
backup-problem-settings-file = That’s Ibis’s own settings file, not a backup. Choose a backup file instead.
backup-problem-newer = That backup is from a newer version of Ibis. Update Ibis, then try again.
backup-problem-invalid = That backup is damaged. Nothing changed.
backup-problem-settings-unusable = Ibis can’t use its settings file as it is. Nothing changed.
backup-problem-changed = Settings changed after the backup was shown. Choose it again.
backup-problem-not-saved = Ibis couldn’t save. Nothing changed.

## The picker, setup, menus, notices and receipts, from messages.ts.

reader-show = Show PDF
reader-hide = Hide PDF
reader-cite = Cite This Page
reader-page-in-view = Page in view
reader-thumbnails = Pages
reader-pages = Pages of the PDF
reader-region = Pages of { $title }
reader-unknown = PDF page { $index } of { $count } · no printed page
reader-ask = What number is printed on PDF page { $index }?
reader-use-nearby = Use for Nearby Pages
peek-label = Details of { $title }
peek-container = Published in
peek-year = Year
peek-type = Type
peek-key = Citation key
citation-details-page = Page
citation-details-none = None
citation-details-page-placeholder = Add page
citation-details-page-hint = A page, a range or several, as printed
citation-details-form = Show as
citation-details-examples-unavailable = Examples are unavailable
citation-details-edit = Edit { $reference }
citation-details-details = Citation details for { $reference }
operation-failures-recent-history = Recent citations could not be forgotten. Try again.
operation-failures-project = The .bib file to check against could not be changed. Try again.
operation-failures-row-context = The reference menu could not open. Use Enter to copy or Shift+Enter to add.
operation-failures-status = Ibis could not check your library. Choose the file again.
operation-failures-shortcut = Ibis could not check the shortcut. Open Ibis from the menu bar or tray.
operation-failures-choose = The file chooser did not open. Try again.
operation-failures-drop-refused = Ibis opens one .bib, .biblatex or CSL-JSON file at a time.
operation-failures-zotero = Ibis could not start reading Zotero. Try again.
operation-failures-samples = Ibis could not change the sample library. Try again.
operation-failures-zotero-groups = Ibis could not list your Zotero groups. Check that Zotero is open, then try again.
operation-failures-reload = Ibis could not reload your library. Choose the file again.
operation-failures-hide = Ibis could not hide this window. Try again.
operation-failures-pin = Ibis could not change the pin. Try again.
operation-failures-preferences = Ibis could not read or save your picker choices. They apply until Ibis restarts.
operation-failures-window-size = Ibis could not save the window size. It will be kept until you quit.
operation-failures-quit = Ibis could not quit. Try again.
operation-failures-update-events = Update progress is unavailable.
operation-failures-picker-events = Ibis could not focus the search box. Restart Ibis to try again.
operation-failures-library-events = Your library may not update automatically. Use Reload, or restart Ibis.
operation-failures-find-sources = Ibis could not look for libraries on this computer. Choose a file instead.
operation-failures-open-found = That file could not be opened from the list. Pick it again from the new list.
operation-failures-settings = Settings could not open. Try again.
operation-failures-app-choices = Ibis needs your paste choice for this app because its saved app choices were damaged. Choose Paste or Copy Only in the format menu.
operation-failures-app-read = Ibis could not tell which app this is for. Your default applies, and a format you pick is for this paste only. Reopen Ibis to try again.
operation-failures-app-choice = Ibis could not save the choice for this app. Try again.
setup-sources-title = Where do your references live?
setup-sources-line = Ibis finds a reference and pastes its citation where you’re typing. It only reads your library, never changes it.
setup-return-hint = After this, open Ibis from any app with { $shortcut }.
setup-shortcut-unavailable = The shortcut is unavailable. Open Ibis from the menu bar or tray.
setup-no-shortcut = No shortcut is set.
setup-open-without-shortcut = Open Ibis from the menu bar or tray.
setup-opening-line = Reading your library. Zotero can stay closed.
setup-samples = Try Sample References
setup-samples-help = Three invented ones to try
setup-sample-label = Sample
setup-sample-title = Sample references
setup-sample-search-help = Search for “lantern”, choose the lantern gardens reference, then press Enter.
setup-own-library = Use Your Own Library
setup-own-library-help = Choose Zotero or a library file instead of samples
setup-empty-line = Choose a library with references.
setup-zotero = Zotero
setup-find-libraries = Find Them on This Computer
setup-found-in = Found in { $folder }
setup-choose-found = Choose a Found Library
setup-found-title = Libraries found on this computer
setup-find-help = Zotero and .bib files nearby
setup-look-again = Look Again
setup-looking = Looking for Zotero and .bib files…
setup-cut-short = Some folders were too large to search fully. Choose the file yourself if it is not listed.
setup-nothing-found = No Zotero or .bib files found nearby. Choose the file yourself.
setup-missing-title = Can’t find { $name }
setup-missing-line = It may have moved, or its drive is disconnected.
setup-find = Find { $name }
setup-find-file-help = Choose where it is now
setup-unreadable-title = Can’t read { $name }
setup-zotero-closed-title = Open Zotero to read your library
setup-zotero-closed-line = Ibis usually reads it with Zotero closed, but not this time.
setup-zotero-missing-title = Ibis can’t find Zotero’s library
setup-zotero-missing-line = If Zotero isn’t installed, install it and open it once. Ibis will carry on by itself.
setup-zotero-access-off-title = Zotero isn’t sharing your library
setup-zotero-access-off-waiting = Ibis will carry on by itself once it’s on.
setup-open-zotero-help = Ibis carries on by itself once it’s open
setup-open-file-help = See the file in its own app
setup-add-references-help = Add references there
setup-zotero-profiles-title = Which Zotero profile?
setup-zotero-profiles-line = Zotero is closed. Choose the profile to read.
setup-zotero-profiles-failed = Ibis could not list your Zotero profiles. Open Zotero and Ibis will carry on by itself.
setup-other-title = Can’t open { $name }
setup-the-library = the library
setup-try-again = Try Again
setup-try-again-help = Read the library again
setup-another = Use Another Library
setup-another-help = Zotero or another library file
session-menu = Cited Since Opening…
session-title = Cited since opening
session-about = What you copied with Ibis since it opened, or since you reset this list, in every app. Ibis can’t see your document, so check this against it.
session-empty = Nothing yet. References you copy appear here.
session-close = Close
session-remove = Remove
session-reset = Reset List
session-undo-reset = Undo Reset List
session-full = This list is full, so later references were not added.
session-copy = Copy Bibliography
session-save = Save…
session-copied = Copied the bibliography.
session-changed = The list changed. Check it, then try again.
session-not-read = The references could not be read. Try again.
session-not-formatted = These references could not be written as a bibliography. Try another style.
session-not-copied = The bibliography could not be copied. Try again.
session-not-saved = The bibliography could not be saved. Try again.
writing-project-title = Writing Projects
writing-project-loading = Loading writing projects…
writing-project-saving = Saving project…
writing-project-menu = Writing Projects…
writing-project-about = A writing project keeps its citation format and optional bibliography together. Your choice is remembered for this app; changes apply wherever you use this project. Ibis does not identify the document.
writing-project-session-only = Ibis can’t identify the app, so this project applies only until Ibis quits.
writing-project-close = Close
writing-project-none = No Writing Project
writing-project-empty = No writing projects yet. Your app’s format applies until you choose one.
writing-project-use = Use { $name }
writing-project-selected = Selected writing project
writing-project-selected-label = Selected
writing-project-format = Format: { $name }
writing-project-bibliography = Bibliography: { $name }
writing-project-no-bibliography = No bibliography selected.
writing-project-choose-bibliography = Choose Bibliography…
writing-project-remove-bibliography = Remove Bibliography
writing-project-create = New Writing Project…
writing-project-confirm-create = Create Project
writing-project-starts-with = Starts with { $format }. Change it in the format menu after creating the project.
writing-project-name = Project name
writing-project-rename = Rename Project…
writing-project-save-name = Save Name
writing-project-remove = Remove Project…
writing-project-confirm-remove = Remove Project
writing-project-cancel = Cancel
writing-project-unsupported = This app version cannot use this project’s format.
writing-project-not-read = The writing projects could not be read. Close this sheet, then try again.
writing-project-not-saved = The project could not be saved. Your previous choice still applies. Try again.
writing-project-bibliography-not-saved = The bibliography choice could not be saved. Try again.
writing-project-pick = Your pick changes this project’s format.
project-check-retry = Retry
project-check-locate = Locate Bibliography…
density-compact = Compact
density-comfortable = Comfortable
picker-choose-project = Check Against a .bib File…
picker-choose-other-project = Check Against Another Bibliography…
picker-open-zotero = Open Zotero
picker-show-in-zotero = Show in Zotero
picker-source-changed = The library changed; select the reference again.
picker-search-label = Search references
picker-search-placeholder = Search references…
picker-drag-search-bar = Drag the search bar edge to move the picker
picker-search-error = Ibis could not search your library. Use Reload, then try again.
picker-copy-error = Nothing was copied. Try again, or use Reload.
picker-snapshot-changed = The library changed. Search again and reselect the references before copying.
picker-shared-key = More than one reference has the key { $key }, so it can’t be cited by key. Give one of them a different key in Zotero or your .bib file.
picker-using-locator = Using { $label } { $value }
picker-search-as-typed = Search as typed
picker-apply-locator = Apply locator to { $key }
picker-copied = Copied
picker-key-formats = Key formats
picker-formatted-styles = Citation styles
picker-filter-formats = Filter formats
picker-format-for-app = { $format } · { $app }
picker-format-this-paste = { $format } · This paste
picker-format-remembered = Remembered for { $app }.
picker-format-suggested = Suggested for { $app }.
picker-format-app-default = { $app } has no remembered format, so your default applies.
picker-format-no-app = Ibis can’t tell which app this is for, so your default applies.
picker-format-held = { $format } applies to this paste only.
picker-pick-replaces = Your pick replaces it.
picker-pick-remembered = Your pick is remembered for this app.
picker-pick-default = Your pick becomes your default.
picker-pick-this-paste = Your pick applies to this paste only.
picker-pick-to-remember = Pick a format to remember it for this app.
picker-pick-to-set-default = Pick a format to make it your default.
picker-for-app = For { $app }
picker-pick-heading-default = Default format
picker-pick-heading-this-paste = This paste only
picker-paste-from-now-on = Paste into { $app } from Now On
picker-copy-only = Copy Only
picker-copy-only-here = Copy Only in { $app }
picker-preview = Preview
picker-citation-details = Citation details
picker-edit = Edit
picker-details = Details
picker-citation-warnings = Citation warnings
picker-full-citation = Full citation
picker-window-reset = Window position reset.
picker-undo = Undo
picker-notices = Notices
picker-undo-failed = Could not undo. Try again.
picker-preview-pending = Formatting…
picker-preview-error = This citation could not be formatted. Choose another style, or copy citation keys.
picker-preview-refused = This citation could not be prepared. Close and reopen Ibis to try again.
picker-preview-style-error = Couldn’t format in { $style }. Choose another style, or copy keys.
picker-entry-only = { $style } numbers depend on your whole document, so Ibis copies the reference entry instead.
picker-entry-only-short = Reference entry
picker-first-note = Later notes to the same work are shortened, and which note comes first depends on your whole document, so Ibis copies the full first note.
picker-first-note-short = Full first note
picker-shelf-stale = The library changed. Reading it again…
picker-all-chosen = Show all { $n } selected references
picker-dismiss = Dismiss
picker-dismiss-warnings = Dismiss copy warnings
picker-copy-warnings = Copy warnings
picker-citation-selection = Citation selection
picker-selected-references = Selected references
picker-filter-references = Filter references
picker-references = References
picker-search-results = Search results
picker-results = Results
picker-no-date = No date
picker-added-to-citation = Added to citation
picker-add-named = Add { $name } to citation
picker-remove-named = Remove { $name } from citation
picker-best-matches = Best matches
picker-pasted-list = Pasted list
picker-paste-into = Paste into { $app } · { $output }
picker-last-outcome = Last outcome. { $outcome }
picker-closest-matches = Closest matches
picker-closest-match = Closest match
picker-closest-match-warning = “{ $title }” is a closest match: only some search words matched.
picker-no-matches = No matching references
picker-no-matches-hint = Try a surname, year or fewer words.
picker-filter-obstacle = No matches with this filter
picker-filter-obstacle-hint = A tag, year or field in your search leaves nothing.
picker-remove-filter = Remove Filter
picker-scope-obstacle-hint = Some of your Zotero libraries are left out of this search.
picker-not-in-libraries = Not in your libraries
picker-not-in-libraries-hint = Add it to your library, then search again.
picker-clear-search = Clear Search
picker-search-all-libraries = Search All Zotero Libraries
picker-type-to-find = Type to find a reference
picker-type-to-find-hint = Search by title, author, year, citation key or DOI.
picker-more = More
picker-change-last-citation = Change Last Citation
picker-change-citation = Change Citation · { $format }
picker-keep-open = Keep Window Open
picker-keep-open-menu = Keep Window Open
picker-change-format = Change Format…
picker-drag-citation = Drag the citation into any app
picker-commands = Commands
picker-no-commands = No matching commands
picker-more-title = More actions ({ $keys })
picker-choose-library = Choose Library…
picker-use-zotero = Use Zotero
picker-library-region = Library
picker-picker-actions = Picker actions
picker-library-status = Library status
picker-reload = Reload
picker-reload-library = Reload Library
picker-choose-zotero-profile = Choose Zotero Profile…
picker-zotero-profile = Zotero Profile
picker-choose-library-menu = Choose Library…
picker-copy-as = Copy as
picker-quit-label = Quit Ibis
picker-settings = Settings…
picker-density = Rows: { $name }
picker-rows = Rows
picker-density-examples-compact = 1 line
picker-density-examples-comfortable = 2 lines
picker-density-title = Show less or more of each reference
picker-view-changes = View changes
picker-larger-text = Larger text
picker-zotero-libraries = Zotero libraries
picker-search-in = Search in
picker-all-libraries = All Libraries
picker-my-library = My Library
picker-group-library = Group { $id }
picker-keep-one-library = Keep at least one Zotero library on.
picker-remove-reference = Remove { $key } from the selection
picker-remove-reference-in = Remove { $key } ({ $library }) from the selection
key-title = Keyboard Shortcuts
key-close = Close
key-regions-search = While searching
key-regions-library = Your library
key-regions-picker = Anywhere in Ibis
key-regions-menu = In a menu
key-regions-system = Open Ibis
key-regions-citation = In the citation
key-actions-source = Open the source
key-actions-choose-source = Choose a library
key-actions-reveal-source = Show the source in its folder
key-actions-copy = Paste or copy the citation
key-actions-add = Add another reference
key-actions-move = Move through results
key-actions-peek = Peek at the highlighted reference
key-actions-escape = Step back or hide
key-actions-format = Change the format
key-actions-more = More actions
key-actions-change-last = Edit the citation you just pasted
key-actions-pin = Keep the window open
key-actions-keys = Show keyboard shortcuts
key-actions-settings = Open Settings
key-actions-quit = Quit Ibis
key-actions-undo = Undo
key-actions-text-size = Make text larger or smaller
key-actions-citation-move = Move between references
key-actions-citation-remove = Remove the reference
key-actions-menu-move = Move through the menu
key-actions-menu-ends = Go to the first or last item
key-actions-menu-choose = Choose the item
key-actions-menu-leave = Leave the menu and return to the search
key-actions-open = From any app
key-footer = Keys
key-copy = Copy
key-copy-as = Copy as { $format }
key-copy-several-as = Copy { $count } as { $format }
key-add-to-citation = Add to Citation
key-remove-from-citation = Remove from Citation
key-close-menu = Close Menu
key-clear-search = Clear Search
key-hide = Hide Ibis
library-live-updates-off = Live updates are unavailable. Reload the library after editing it.
row-et-al = et al.
copied-notice-title = Copied
copied-notice-plugin-note = If this document uses a reference manager’s plugin (Zotero, Mendeley, EndNote, Citavi, Paperpile and others), insert with the plugin.
copied-notice-source = Ibis
copied-notice-notice-failed = The Copied notice could not be shown.
update-title = Updating Ibis
update-downloading = Downloading
update-installing = Checking and installing
update-failed = The update could not be installed. Use Check for Updates in Settings › About to try again later.
library-entries-to-check =
    { $count ->
        [one] 1 entry to check
       *[other] { $count } entries to check
    }
library-more-not-listed =
    { $count ->
        [one] 1 more is not listed.
       *[other] { $count } more are not listed.
    }
