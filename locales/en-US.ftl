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

## The app’s own sentence about a citation.

app-loss = This output cannot fully show { $what }{ $target }.

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
