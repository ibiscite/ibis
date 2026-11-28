# Ibis’s words, for the command line, agents (MCP) and the app (#1675).
# The engine returns codes; each code has one sentence here, which every front
# end shows. A `.cli`, `.agent` or `.app` attribute says what to do next in that
# front end. Ids come from the codes’ wire names, and a test fails on a code
# without words or words without a code. People’s words, not the engine’s
# (design principle 12), and typographic apostrophes.

-library-formats = BibTeX, BibLaTeX, CSL-JSON, RIS or CSV

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
reason-damaged-entry = The library has an entry with this citation key, but Ibis could not read it, so it skips that entry.
    .cli = Fix the entry; ibis libraries says which line and why.
    .agent = Tell the person the entry with this key is damaged and needs fixing in their library.
    .app = Fix the entry in your library.
reason-ambiguous-key = Several references use this citation key.
    .cli = Give each one its own key in your library.
    .agent = Use the ref that search returned.
    .app = Give each one its own key in your library.
reason-missing-key = This reference has no citation key.
    .cli = Give it one in your library, or use --style.
    .agent = Use a style, or ask the person to give it one.
    .app = Choose a citation style.
reason-missing-file = The library file was moved, renamed or removed.
    .cli = Choose its current location with --file.
    .agent = Ask the user to locate the library file, then start a new session.
    .app = Locate the library file.
reason-app-missing = The Ibis app was not found.
    .cli = Install it, or give its location with --app <path>.
reason-not-ibis = The program given as the Ibis app is not Ibis.
    .cli = Give the Ibis app’s location with --app <path>.
reason-no-answer = The Ibis app started but did not answer.
    .cli = Open Ibis once by hand, then try again.
reason-settings-read-timeout = Reading the Ibis app’s library settings timed out.
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
reason-csv-template = The first row of this CSV file does not match Ibis’s template: it must name a type and a title column, and only columns the template lists.
    .cli = Rename the columns in its first row; the CSV template in Ibis’s compatibility guide lists them.
    .agent = Ask the person to rename the columns in the file’s first row to the template’s.
    .app = Rename the columns in its first row; the CSV template in Ibis’s compatibility guide lists them.
reason-library-cut-short = No reference has this key in what Ibis read, but the library is too large to read in full (Ibis stops at 200,000 entries), so the key may be in the part it did not read.
    .cli = Split the library into smaller files and give each with --file.
    .agent = Ask the person to split the source library into smaller files so Ibis can read the missing references.
    .app = Split the library into smaller files.
reason-unknown-format = This file is not in a format Ibis reads: { -library-formats }.
    .cli = Choose another file with --file.
    .agent = Ask the person to choose another file.
    .app = Choose another file.
reason-duplicate-record = Two records sent with the request have the same id, counting 1 and "1" as one, so Ibis cannot tell which is meant.
    .cli = Give each record its own id.
    .agent = Give each record its own id.
reason-rejected-record = A record sent with the request is not valid CSL-JSON, so Ibis cites none rather than leave it out.
    .cli = Each record needs an id and a CSL type; fix or remove it.
    .agent = Each record needs an id and a CSL type such as book or article-journal; fix or remove the invalid one.
reason-no-attachment-text = Ibis cannot read this attachment’s text: Zotero has not indexed it, it is a link, or its file is missing, damaged or one Ibis cannot read.
    .cli = Run ibis outline on its reference to see which; in Zotero, Reindex Item adds the text.
    .agent = Tell the user; they can add the text in Zotero with Reindex Item. Do not guess what it says.
reason-attachment-outside-library = This attachment file is outside the library’s folder. You can allow it in Settings › AI Apps › Attachment text.
reason-no-such-pages = This attachment has no text for those pages, or where its pages begin is not known.
    .cli = Run ibis outline on its reference to see which pages have text, or read without --pages.
    .agent = Use the pages outline gives, or read with offset instead when its pages are not known.
reason-destination-exists = Something is already there with that name. Choose another name or place; Ibis never replaces it.
reason-destination-unwritable = Ibis could not write there. Choose a folder that exists and that you can write to, with room left on its disk.
reason-source-not-found = Ibis could not start a source adapter by this name.
    .cli = Check the name after --source and the absolute executable path in source-adapters.json. Run ibis doctor to see the configured adapters.
    .agent = Ask the person to check the source name and its configuration in ibis doctor.
reason-source-manager-closed = The reference manager is not open, and this source reads it only while it is.
    .cli = Open the reference manager and its library, then try again.
    .agent = Ask the person to open the reference manager and its library.
reason-source-library-needed = Several of the reference manager’s libraries are open, so Ibis needs to know which to read.
    .cli = Name one after the source, as in --source endnote=NAME.
    .agent = Ask the person which library to read.
reason-source-library-not-found = None of the reference manager’s open libraries has that name.
    .cli = Open the library in the reference manager, or check its name after --source.
    .agent = Ask the person to open the library in the reference manager.
reason-source-permission = The operating system did not let Ibis ask the reference manager for its library.
    .cli = On macOS, allow it in System Settings, Privacy & Security, Automation, then try again.
    .agent = Ask the person to allow Ibis to control the reference manager in their system’s privacy settings.
reason-source-failed = The source adapter did not read the library: it could not, it stopped, it took too long, or it answered in a way Ibis does not accept.
    .cli = Try again; ibis sources check shows what the adapter answers.
    .agent = Ask the person to run ibis sources check for this source.

## The reading list page in a pack of references (#1953).
app-share-page-title = Reading list
app-share-page-import = References.bib loads these references into Zotero, EndNote or any other reference manager.
app-share-page-pdfs = The PDFs folder holds the PDFs that came with them.

## What a citation could not say in full.

warning-source-field = A field of { $reference } could not be fully converted. Check it in your library.
warning-sentence-case-expected = APA expects sentence case for the title of { $reference }. Its capitalisation was kept as stored, so check it in your document.
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
notice-input-language-unsupported = Ibis does not recognise locator words in this language. Give the page or location in English, such as “p. 12”.

## Citation forms, by their one name.

form-normal = Name in brackets
form-narrative = Name in the sentence
form-author-only = Name only
form-year-only = Year only
form-full = Full reference
form-bibliography-only = Reference list

## Why a library skipped a record or asks for a field to be checked. `.field`
## is used when the source names the field.

cause-not-closed = it never closes
cause-no-key = it has no citation key
cause-missing-value = a field has no value
    .field = { $field } has no value
cause-unbalanced = a field’s braces or quotes do not pair up
    .field = the braces or quotes in { $field } do not pair up
cause-unexpected-text = it has text where a field should be
    .field = { $field } has text Ibis cannot read
cause-unknown-type = its type is not one Ibis knows
cause-conflicting = it has repeated or conflicting field values
    .field = { $field } is given more than once
cause-wrong-cells = it has more or fewer cells than the first row
cause-missing-field = a required field is empty
    .field = { $field } is empty
cause-no-start = its fields come before any TY line
cause-string-definition = an @string definition cannot be read
cause-unreadable = it cannot be read
cause-abbreviation = a field uses an abbreviation that is not defined or cannot be expanded
    .field = { $field } uses an abbreviation that is not defined or cannot be expanded
cause-date = a date cannot be read in full
    .field = { $field } cannot be read in full as a date
cause-names = a list of names cannot be read
    .field = the names in { $field } cannot be read
cause-role = a role has no CSL equivalent, so those names count as contributors
    .field = { $field } has no CSL equivalent, so those names count as contributors
cause-link = a link is private or not a web address, so it is left out
    .field = { $field } is private or not a web address, so it is left out
cause-inheritance = a cross-reference is missing or loops back
    .field = { $field } names an entry that is missing or loops back
cause-inheritance-limit = Ibis reached its limit for reading linked entries
    .field = { $field } reaches Ibis’s limit for reading linked entries
cause-too-large = a field is too large to read
    .field = { $field } is too large to read
cause-identifier = an identifier is not valid
    .field = { $field } is not a valid identifier
cause-language = its language is not one Ibis recognises
    .field = { $field } names a language Ibis does not recognise
cause-tex-command = it uses a TeX command Ibis does not understand, kept as written
cause-unknown-field = a field is not one the format has, so it is left out
    .field = { $field } is not a field the format has, so it is left out
cause-value = a value cannot be read as written
    .field = { $field } cannot be read as written

## The command line’s own sentences about a refusal.

cli-together-refused = { $output } cannot write these references together as asked. Cite them one at a time to see which one fails.
cli-loss-left-out = { $output } cannot show { $what }{ $target }, so it was left out.
cli-loss-flattened = { $output } wrote { $what }{ $target } as plain text.
cli-writing-without-bibliography = no .bib file
cli-loss-unknown = { $output } changed { $what }{ $target } in a way this version of Ibis can’t name. Check the citation.
cli-loss-something = part of the citation
cli-doctor-about = Check your libraries and settings offline, without writing or sending anything
cli-doctor-summary = Ibis { $version } · contract { $contract } · { $methods } engine methods, { $outputs } key outputs, { $styles } citation styles
cli-spelling-match = (spelling match)
cli-no-references = No open library holds any references. Add references to it, or open another library with --file or --zotero. A file must be { -library-formats }. For CSV, use Ibis’s template.
cli-file-help = Open a { -library-formats } file, or a folder of PDFs Ibis has kept; repeat for several libraries
cli-folder-kept = { $folder }: kept { $kept } PDFs in ibis-references.bib ({ $unread } with no details Ibis could read); { $retry } to try again later
cli-folder-not-kept = Ibis has not kept this folder’s PDFs yet; run ibis sources keep --file FOLDER first.
cli-file-unreadable = It cannot be read, or it is not a { -library-formats } library; check its permissions and contents.
app-file-dialog-title = Choose a library
app-file-dialog-formats = { -library-formats }

## The app’s native windows and dialogs, and its words about other apps.

app-window-settings = Ibis Settings
picker-native-settings-picker = Picker
picker-native-window-menu = Window
picker-native-close-window = Close Window
# Read by screen readers; never the citation.
app-window-notice = Ibis notice
app-open-link-title = Open Link
app-open-link-copied = Ibis could not open your browser. The link was copied: paste it into your browser.
# The link follows on its own line.
app-open-link-failed = Ibis could not open your browser. Open this link in your browser:
# The list in System Settings › Privacy & Security that holds the switch, as macOS names it.
app-paste-list-before-27 = Accessibility
app-paste-list-since-27 = Device Control and Data Access
# ChatGPT's own menus, in the words its window uses.
app-agent-steps-chatgpt = In ChatGPT desktop: Settings › MCP servers › Add server › STDIO, command ibis, arguments mcp --app-libraries.
# A change an AI app proposes for the library. $title and $label name the reference.
app-proposal-add = Add “{ $title }” as { $key }.
app-proposal-fields = Change { $fields } of “{ $label }”.
app-proposal-tags =
    { $state ->
        [add] Add the tags { $add } on “{ $label }”.
        [remove] Remove the tags { $remove } on “{ $label }”.
       *[both] Add the tags { $add } and remove the tags { $remove } on “{ $label }”.
    }

# Why a proposed library change was not recorded, shown or applied. $field
# names a field or tag as the proposal did; $count and $kib are numbers.
-proposal-ask-again = Ask the app that made it to propose the change again.
-proposal-choose-library = Use `proposals` to find an open .bib library, or ask the user to open one.
-proposal-edit-input = Revise the proposal and try again.
refusal-unknown-proposal = No proposal has this id.
    .cli = Run `ibis proposals list` with the same options to find the id.
    .app = Return to Proposed Changes to refresh the list.
refusal-proposal-unreadable = This proposal’s file cannot be read.
    .cli = { -proposal-ask-again }
    .app = { -proposal-ask-again }
    .agent = Propose the change again.
refusal-store-unreadable = Ibis could not read the proposals folder. Check that Ibis can read it, then try again.
refusal-not-saved = The proposal could not be saved beside Ibis’s settings. Check that Ibis can write there, then try again.
refusal-too-many = { $count } proposals are waiting already.
    .cli = Use `ibis proposals list` to find proposals you can reject.
    .app = Apply or reject a waiting proposal to make room.
    .agent = Use `proposals` to list yours and ask the user to review them in Ibis.
refusal-too-large = A proposal holds at most { $kib } KiB.
    .agent = Shorten the proposed change or split it into smaller proposals.
refusal-decided = This proposal was applied or rejected already.
refusal-invalid = A field in this proposal cannot be used.
    .agent = { -proposal-edit-input }
refusal-invalid-field = `{ $field }` cannot be used in a proposal.
    .agent = { -proposal-edit-input }
refusal-no-target = Choose a library to add to.
    .agent = { -proposal-choose-library }
refusal-unknown-library = The named library is not open.
    .agent = { -proposal-choose-library }
refusal-unknown-reference = The reference is not in an open library; search again.
refusal-library-kind = Ibis can only propose adding references to .bib files.
    .app = Choose a .bib library for the new reference.
    .agent = { -proposal-choose-library }
refusal-library-kind-edit = Ibis can only propose edits to references in .bib files.
    .app = Edit the reference in the app you use to manage this library.
    .agent = Ask the user to edit the reference in the app they use to manage this library.
refusal-already-in-library = A reference with this DOI is in the library already.
    .agent = Search the library for the existing reference.
refusal-not-open = The library is not open in Ibis.
    .app = Open the proposal’s library in Ibis, then try again.
refusal-changed = The reference changed since this was proposed; nothing was written.
    .cli = { -proposal-ask-again }
    .app = { -proposal-ask-again }
    .agent = Search for the current reference and propose the change again.
refusal-folder-changed = The PDF folder or its bibliography changed while Ibis was keeping PDFs. Run the command again.
refusal-key-taken = Another entry now uses this citation key; nothing was written.
    .cli = { -proposal-ask-again }
    .app = { -proposal-ask-again }
refusal-no-free-key = All the candidate citation keys are in use in this library.
    .agent = { -proposal-choose-library }
refusal-damaged = Some entry in the file cannot be read, so Ibis leaves the file alone.
    .cli = Check the library for malformed entries.
    .app = Check the library for malformed entries.
    .agent = Ask the user to check the library for malformed entries.
refusal-repeated = Two entries in the file have this citation key.
    .cli = Give each entry a unique key in the library file.
    .app = Give each entry a unique key in the library file.
    .agent = Ask the user to give each entry a unique key in the library file.
refusal-not-literal = A field uses a @string macro or joins values, so Ibis does not edit it.
    .cli = Check the field in the library file.
    .app = Check the field in the library file.
    .agent = Ask the user to check the field in the library file.
refusal-not-literal-field = `{ $field }` uses a @string macro or joins values, so Ibis does not edit it.
    .cli = Check the field in the library file.
    .app = Check the field in the library file.
    .agent = Ask the user to check the field in the library file.
refusal-unreadable = The library file cannot be read as text.
    .cli = Check that the library file is still in place and readable.
    .app = Check that the library file is still in place and readable.
    .agent = Ask the user to check that the library file is still in place and readable.
refusal-backup-failed = A backup of the file could not be written; nothing was changed.
    .app = Check that Ibis can write to its settings folder and that there is free disk space, then try again.
refusal-not-written = The file could not be written; nothing was changed.
    .app = Check that the library file is writable and that there is free disk space, then try again.

# The updater's dialogs. $version and $current are version numbers.
app-update-question-title = Check for updates automatically?
app-update-question = Ibis can check GitHub for a newer version a few times a day. It asks GitHub for one small file; GitHub sees your IP address and the Ibis version. Nothing from your library is sent, and updates install only when you agree.
app-update-check-automatically = Check Automatically
app-update-not-now = Not Now
app-update-offer-title = An update is available
app-update-offer = Ibis { $version } is available; you have { $current }.
app-update-offer-steps = Ibis will download it, check its signature, install it and restart.
app-update-offer-question = Install this update and restart Ibis?
app-update-later = Later
app-update-install = Install and Restart
app-update-cancel = Cancel
app-update-install-failed = The update could not be installed. Try again later. If Ibis no longer opens, download and reinstall the newest version from GitHub.
app-updates-title = Updates

## The app’s own sentence about a citation.

app-loss = This output cannot fully show { $what }.
    .named = This output cannot fully show { $what } for { $reference }.
app-project-warning =
    { $state ->
        [missing] { $key } is missing from { $name }.
        [repeated] { $key } appears more than once in { $name }.
        [may-name-another] { $key } may name another reference in { $name }.
       *[could-not-check] Couldn’t check { $key } against { $name }.
    }
app-tray-open = Open Ibis
app-tray-open-shortcut = Open Ibis ({ $shortcut })
# The Mac's modifier keys by name, where a menu cannot draw its symbols.
app-key-control = Control
app-key-option = Option
app-key-shift = Shift
app-key-command = Command
app-tray-choose = Choose File…
app-tray-zotero = Zotero
app-tray-retry = Try Again
app-tray-settings = Settings…
app-tray-update = Check for Updates…
app-tray-help = Help
app-tray-quit = Quit { $name }
app-tray-shortcuts = Keyboard Shortcuts
app-tray-whats-new = What’s New
app-tray-problem = Report a Problem…
app-tray-idea = Suggest an Idea…
app-tray-reset = Reset Window Position
app-tray-fix-library = Fix in Ibis…
app-tray-library = Library
app-tray-no-library = No library yet
app-tray-opening = Opening { $name }…
app-tray-reading = Reading { $name }…
app-tray-count = { $name } · { $count } references
app-tray-count-one = { $name } · 1 reference
app-tray-zotero-newer = Zotero needs a newer Ibis
app-tray-file-missing = { $name } was moved or is unavailable
app-tray-file-unreadable = { $name } could not be read
app-tray-file-format = { $name } is not a supported library file
app-tray-file-csv = { $name } needs the Ibis CSV template
app-tray-file-too-large = { $name } is larger than 64 MiB
app-tray-file-invalid = { $name } does not contain valid references
app-tray-zotero-sharing = Zotero is not sharing your library
app-tray-zotero-profile = Choose which Zotero profile to read
app-tray-zotero-missing = No Zotero library was found
app-tray-zotero-closed = Open Zotero to read your library
app-tray-zotero-no-space = Free some space or open Zotero to read your library
app-tray-zotero-group-missing = A chosen Zotero group library could not be found
app-tray-zotero-work-limit = Zotero took too long to read
app-tray-not-updated = { $name } could not be updated
app-tray-not-opened = { $name } could not be opened
app-tray-version-copy-failed = The version could not be copied. Try again.
app-tray-fix-problem = Fix 1 Problem…
app-tray-fix-problems = Fix { $count } Problems…
app-tray-fix-failed = The library could not be opened for editing. Open it from the picker to try again.
app-tray-spoken-reading = Ibis, reading { $name }
app-tray-spoken-problem = Ibis, { $problem }
app-tray-spoken-empty = Ibis, no library yet
app-tray-spoken-ready = Ibis, ready
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
app-library-not-saved = the library opened but could not be saved for next time: { $error }
app-command-snapshot-changed = The library changed. Search again.
app-command-library-loading = library is loading
app-command-cancelled = cancelled
app-command-shared-citation-key = the citation key `{ $key }` belongs to more than one reference
app-command-citation-too-large = This citation has too much text. Shorten its fields or remove references.
app-command-not-prepared = The citation could not be prepared. Try again.
app-command-source-not-opened = The source could not be opened. Try again.

## The app’s interface.

# Citation language and text direction have separate owners.
# Settings interface copy; native guidance and key notation keep their owners.
settings-pages = Settings pages
settings-general = General
settings-shortcuts = Shortcuts
settings-about = About Ibis
settings-about-description = The version, updates and where Ibis comes from.

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
general-language = Language
general-language-description = Menus, Settings and messages, from the next time Ibis opens. Citations keep their style’s language.
general-language-system = Same as the system
general-language-words = language interface translation english german deutsch arabic
# Each language by its own name, the same in every catalogue.
language-english = English
language-german = Deutsch
language-arabic = العربية
general-placement = Where Ibis opens
general-placement-description = On the display you’re working on, unless you’ve moved Ibis there.
general-placement-words = position screen display corner centre center opens
general-placement-centre = Centre
general-placement-top-left = Top left
general-placement-top-right = Top right
general-placement-bottom-left = Bottom left
general-placement-bottom-right = Bottom right
general-menu-bar-description = If hidden, open Ibis from Applications to come back here.
general-paste-title = Pasting
general-behavior-title = Behaviour
general-paste-label = Paste into the app you came from
general-paste-key =
    { $platform ->
        [macos] ⌘V
       *[other] Ctrl+V
    }
general-paste-description = Otherwise Ibis copies and you press { general-paste-key }.
# $list is the system’s permission-list name, not an interface message code.
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
general-paste-granted = Pasting is on.
general-paste-return-failed = Couldn’t return to your citation. Open Ibis with your shortcut to continue.
general-paste-stale = Already on, but still waiting? In System Settings, remove { $name } from the list, add this copy again, then turn it on.

general-rows = Rows
general-rows-description = Compact rows fit more references, one line each; comfortable rows give every reference two lines.
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
shortcuts-action-pick = Paste or copy a result by its number
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
shortcuts-action-citation-reorder = Move the reference
shortcuts-action-citation-open = Show its details
shortcuts-action-citation-focus = Go to the citation
shortcuts-action-inspector = Show citation details
shortcuts-action-send = Send to your editor
shortcuts-action-menu-move = Move through the menu
shortcuts-action-menu-ends = Go to the first or last item
shortcuts-action-menu-choose = Choose the item
shortcuts-action-menu-leave = Leave the menu and return to the search
shortcuts-or = or
# Keys named by a word, as the system's own menus and keyboards write them.
key-enter = Enter
key-escape = Esc
key-home = Home
key-end = End
key-tab = Tab
key-space = Space
key-backspace = Backspace
key-delete = Delete
key-plus = Plus
key-minus = Minus
key-ctrl = Ctrl
key-alt = Alt
key-shift = Shift
key-super = Super
key-click = Click
# After the keys held while clicking: ⌘-click, Ctrl+click.
key-click-held = click

about-automatic = Check for updates automatically
about-automatic-description = Reads a small update file from GitHub a few times a day.
about-check = Check for Updates
about-check-label = Check for updates
about-check-description = See whether a newer Ibis is ready to install.
settings-advanced-title = Advanced
settings-reset-words = reset default restore use default
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
settings-look = Appearance
settings-citations = Citations
attachment-text-title = Attachment text
attachment-text-label = Let AI apps read attachment text
attachment-text-description = This switch covers connected AI apps: they can read attachment text with page numbers, never the files. A terminal running ibis does not need this switch.
attachment-text-words = attachment full text PDF
attachment-text-empty = No library is open.
attachment-text-loading = Reading library choices…
attachment-text-outside-label = Allow attachment text outside the library’s folder
attachment-text-outside-description = Allow CLI and AI apps to read attachment files outside the library file’s folder or Zotero’s storage folder on this computer.
settings-search = Search Settings
settings-no-results = No settings match ‘{ $query }’
settings-results = Search results
settings-results-count = { $count ->
    [one] 1 result
   *[other] { $count } results
    }
settings-read-failed = Ibis could not read your settings; reopen Settings to try again.
settings-forget-failed = Ibis could not forget the app’s format; the saved choice is shown.
settings-open-failed = Ibis could not open the settings file.
settings-choice-failed = Ibis could not save this choice; the saved value is shown.
settings-startup-words = startup launch restart login
settings-paste-words = paste copy clipboard Enter
settings-appearance-words = dark mode light theme system
look-appearance = Appearance
look-appearance-description = Follow the system, or choose Light or Dark for Ibis.
look-system = System
look-light = Light
look-dark = Dark
look-accent = Accent
look-accent-description = The colour of selection and buttons: the Mac’s accent, or Ibis scarlet.
look-accent-system =
    { $platform ->
        [macos] Follow the Mac
       *[other] Follow the system
    }
look-accent-ibis = Ibis scarlet
look-accent-words = colour color scarlet red
look-window-size = Window size
look-window-size-description = How large the picker opens.
look-window-small = Small
look-window-medium = Medium
look-window-large = Large
look-window-custom = Custom
look-window-custom-note = Drag an edge of the picker to set your own size. Ibis remembers it on each display.
look-text-size = Text size
look-text-size-description = The size of the words in Ibis and in Settings, apart from the Mac’s own text size.
look-default-text = Default
look-larger-text = Larger
look-sample-title = The quiet garden
look-sample-authors = Mira Vale and Noah Reed
look-sample-journal = Journal of Imagined Studies
citations-default = Default format

citations-default-description = Used in word processors, mail and browsers when the app has no remembered format.
citations-recent = Recent citations
citations-recent-description = Shows what you cited recently in each app. Turning this off forgets them.
citations-recent-words = history recents cited here forget privacy
citations-editor = Cite from your editor
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


settings-newer-title = From a newer Ibis
settings-newer-description = This setting is read-only in this version of Ibis.
settings-read-only = The settings file is read-only; the saved value is shown.

citations-key-formats = Citation keys
citations-style-formats = Citation styles
# Output names and descriptions that are words, not proper names; English repeats the engine's own.
output-name-pandoc-bare = Pandoc without brackets
output-name-keys = Keys only
output-name-doi-links = DOI links
output-name-scannable-cite = Zotero RTF/ODF Scan
output-name-chicago-author-date = Chicago (author-date)
output-name-chicago-notes-bibliography = Chicago (notes)
output-name-gb-numeric = GB/T 7714 (numeric)
output-name-gb-author-date = GB/T 7714 (author-date)
output-name-gb-note = GB/T 7714 (notes)
output-description-apa = APA (7th edition)
output-description-chicago-notes-bibliography = Chicago (notes and bibliography)
output-description-mla = MLA (9th edition)
output-description-gb-numeric = GB/T 7714-2025 (numeric)
output-description-gb-author-date = GB/T 7714-2025 (author-date)
output-description-gb-note = GB/T 7714-2025 (notes)
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

general-small-surprises-label = Bird facts and milestones
general-small-surprises-description = A fact when you click the bird in About, and a short note when you reach a milestone.
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
choice-library-file-help = { -library-formats }
choice-drop-file = Or drag a { -library-formats } file into this window.
keys-title = Keyboard Shortcuts
keys-close = Back
keys-action = Action
keys-shortcut = Shortcut
shortcuts-filter = Filter shortcuts
shortcuts-empty = No matching actions

choice-use-file = Use { $name }
choice-back = Choose Another Source

backup-words = backup restore export import move migrate new computer sync iCloud OneDrive
backup-label = Back up settings
backup-description =
    { $platform ->
        [linux] A file to restore on another computer. Window places stay on this one.
       *[other] A file to restore on another computer. The apps you let Ibis paste into, and window places, stay on this one.
    }
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
backup-problem-busy = Settings are being saved by another Ibis. Wait for it to finish, then try again. Nothing changed.
backup-problem-changed = Settings changed after the backup was shown. Choose it again.
backup-problem-not-saved = Ibis couldn’t save. Nothing changed.

## The picker, setup, menus, notices and receipts.

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
locator-act = Act
locator-appendix = Appendix
locator-article-locator = Article
locator-book = Book
locator-canon = Canon
locator-chapter = Chapter
locator-column = Column
locator-elocation = Location
locator-equation = Equation
locator-figure = Figure
locator-folio = Folio
locator-issue = Issue
locator-line = Line
locator-note = Note
locator-opus = Opus
locator-page = Page
locator-paragraph = Paragraph
locator-part = Part
locator-rule = Rule
locator-scene = Scene
locator-section = Section
locator-sub-verbo = Sub verbo
locator-supplement = Supplement
locator-table = Table
locator-timestamp = Time
locator-title-locator = Title
locator-verse = Verse
locator-version = Version
locator-volume = Volume
citation-details-locator-type = Locator type
locator-short-page = p.
locator-short-chapter = ch.
locator-short-section = §
locator-short-paragraph = ¶
locator-short-line = l.
locator-short-verse = v.
locator-short-volume = vol.
locator-short-book = bk.
locator-short-part = pt.
locator-short-figure = fig.
locator-short-table = tbl.
locator-short-note = n.
locator-short-appendix = app.
locator-short-article-locator = art.
locator-short-canon = can.
locator-short-column = col.
locator-short-elocation = loc.
locator-short-equation = eq.
locator-short-folio = fol.
locator-short-issue = no.
locator-short-opus = op.
locator-short-rule = r.
locator-short-scene = sc.
locator-short-sub-verbo = s.v.
locator-short-supplement = supp.
locator-short-timestamp = at
locator-short-title-locator = tit.
locator-short-version = ver.
locator-short-act = act
citation-details-add-prefix = Text before
citation-details-add-suffix = Text after
citation-token-remove = Remove { $reference }
citation-details-locator-filter = Find a type
citation-details-locator-placeholder = Add a value
key-move-left = Move Left
key-move-right = Move Right
key-edit-citation = Show Citation Details
key-go-to-citation = Go to Citation
picker-order-kept = Keeping your order
picker-your-order = your order
citation-details-prefix = Text before
citation-details-suffix = Text after
citation-details-none = None
citation-details-page-placeholder = Add page
citation-details-page-hint = A page, a range or several, as printed
citation-details-form = Show as
citation-details-examples-unavailable = Examples are unavailable
# Why a citation form is left out of the Show as menu.
citation-details-unavailable-entries = Reference entries have no name forms. Choose another style.
citation-details-unavailable-keys-only = Keys only has no name forms. Choose another output.
citation-details-unavailable-textcite = Not with \textcite
citation-details-unavailable-cited-alone = Only when cited alone
citation-details-unavailable-output = Not in this output
citation-details-edit = Edit { $reference }
citation-details-details = Citation details for { $reference }
operation-failures-recent-history = Recent citations could not be forgotten. Try again.
operation-failures-project = The .bib file to check against could not be changed. Try again.
operation-failures-row-context = The row menu could not open. Enter copies, Shift+Enter adds.
operation-failures-status = Ibis could not check your library. Choose the file again.
operation-failures-shortcut = Could not check the shortcut. Open Ibis from the menu bar or tray.
operation-failures-choose = The file chooser did not open. Try again.
operation-failures-drop-refused = Drop one { -library-formats } file at a time.
operation-failures-zotero = Ibis could not start reading Zotero. Try again.
operation-failures-samples = Ibis could not change the sample library. Try again.
operation-failures-zotero-groups = Ibis could not list your Zotero groups. Check that Zotero is open, then try again.
operation-failures-reload = Ibis could not reload your library. Choose the file again.
operation-failures-hide = Ibis could not hide this window. Try again.
operation-failures-pin = Ibis could not change the pin. Try again.
operation-failures-preferences = Your picker choices could not be saved. They apply until restart.
operation-failures-window-size = Ibis could not save the window size. It will be kept until you quit.
operation-failures-quit = Ibis could not quit. Try again.
operation-failures-update-events = Update progress is unavailable.
operation-failures-picker-events = Ibis could not focus the search box. Restart Ibis to try again.
operation-failures-library-events = Your library may not update automatically. Use Reload, or restart Ibis.
operation-failures-find-sources = Ibis could not look for libraries here. Choose a file instead.
operation-failures-open-found = That file could not be opened. Pick it again from the new list.
operation-failures-settings = Settings could not open. Try again.
operation-failures-app-choices = App choices were damaged. Choose Paste or Copy Only again.
operation-failures-app-read = Ibis could not tell which app this is. Your default applies.
operation-failures-app-choice = Ibis could not save the choice for this app. Try again.
operation-failures-app-delivery = Ibis saved the format but not Paste or Copy Only. Choose it again.
setup-sources-title = Where do your references live?
setup-sources-line =
    { $platform ->
        [linux] Ibis finds a reference and copies its citation for you to paste where you’re typing. It only reads your library, never changes it.
       *[other] Ibis finds a reference and pastes its citation where you’re typing. It only reads your library, never changes it.
    }
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
setup-empty-line = Add references to it, or choose another library.
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
session-title = Cited Since Opening
session-about = What you copied with Ibis since it opened, or since you reset this list, in every app. Ibis can’t see your document, so check this against it.
session-empty = Nothing yet. References you copy appear here.
session-close = Back
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
writing-title = Projects
writing-loading = Loading projects…
writing-saving = Saving project…
writing-menu = New project…
writing-about = A project keeps its citation format and optional bibliography together. Your choice is remembered for this app; changes apply wherever you use this project. Ibis does not identify the document.
writing-session-only = Ibis can’t identify the app, so this project applies only until Ibis quits.
writing-close = Close
writing-none = No Project
writing-empty = No projects yet. Your app’s format applies until you choose one.
writing-use = Use { $name }
writing-selected = Selected project
writing-selected-label = Selected
writing-format = Format: { $name }
writing-bibliography = Bibliography: { $name }
writing-no-bibliography = No bibliography selected.
writing-choose-bibliography = Choose Bibliography…
writing-remove-bibliography = Remove Bibliography
writing-create = New Project…
writing-confirm-create = Create Project
writing-starts-with = Starts with { $format }. Change it in the format menu after creating the project.
writing-name = Project name
writing-rename = Rename Project…
writing-save-name = Save Name
writing-remove = Remove Project…
writing-confirm-remove = Remove Project
writing-cancel = Cancel
writing-unsupported = This app version cannot use this project’s format.
writing-not-read = The projects could not be read. Close this sheet, then try again.
writing-not-saved = The project could not be saved. Your previous choice still applies. Try again.
writing-bibliography-not-saved = The bibliography choice could not be saved. Try again.
writing-pick = Your pick changes this project’s format.
project-check-retry = Retry
project-check-locate = Locate Bibliography…
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
picker-using-shorthand = Using { $value }
picker-apply = Apply
# $word: a word that finds the person’s own cited reference, never a sample one.
picker-shorthand-hint = “{ $word } 417” for a page, “see { $word }” for “see”.
picker-kept-words = Kept as words. For a page, add a comma: “room, 22”.
picker-practice-offer = Try a paste here first: press Enter.
# $citation: exactly what Enter would paste.
picker-practice-pasted = { $citation } pasted here. Next, Enter pastes in your app.
picker-search-as-typed = Search as typed
picker-apply-locator = Apply locator to { $key }
picker-copied = Copied
picker-key-formats = Key formats
picker-formatted-styles = Citation styles
picker-share-formats = Share references
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
picker-preview = Preview
picker-citation-details = Citation details
picker-details = Details
picker-citation-warnings = Citation warnings
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
picker-dismiss = Dismiss
picker-dismiss-warnings = Dismiss copy warnings
picker-copy-warnings = Copy warnings
picker-citation-selection = Citation selection
picker-selected-references = Selected references
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
# The delivery bar's one action: what Enter does, naming where the citation goes and never its format.
picker-bar-paste = Paste into { $app }
picker-bar-paste-entries =
    { $count ->
        [one] Paste Entry into { $app }
       *[other] Paste { $count } Entries into { $app }
    }
picker-bar-copy =
    { $count ->
        [one] Copy
       *[other] Copy { $count } Citations
    }
picker-bar-copy-entries =
    { $count ->
        [one] Copy Entry
       *[other] Copy { $count } Entries
    }
picker-bar-send = Send to Editor
picker-bar-change = Change Citation
# The first screen's practice: Enter pastes into Ibis itself, to try it.
picker-bar-practice = Paste Here to Try
# Said to a screen reader when Enter is pressed on a refused citation. $reason: why, in the engine's words.
picker-bar-refused = Not pasted or copied. { $reason }
picker-last-outcome = Last outcome. { $outcome }
picker-closest-matches = Closest matches
picker-closest-match = Closest match
picker-spelling-match = Spelling match
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
picker-keep-my-order = Keep My Order
picker-change-last-citation = Change Last Citation
picker-paste-reference-list = Paste Reference List
picker-copy-reference-list = Copy Reference List
picker-reference-list-title = Reference list
# $app: the app's own name. The list holds this app's citations from this run of Ibis.
picker-reference-list-help =
    { $count ->
        [one] The reference you cited in { $app } since Ibis opened
       *[other] The { $count } references you cited in { $app } since Ibis opened
    }
picker-forget-recent = Forget Recent Citations
picker-change-citation = Change Citation · { $format }
picker-keep-open = Keep Window Open
picker-keep-open-menu = Keep Window Open
picker-change-format = Change Format…
picker-drag-citation = Drag the citation into any app
picker-more-title = More actions ({ $keys })
picker-use-zotero = Use Zotero
picker-library-region = Library
picker-picker-actions = Picker actions
picker-library-status = Library status
picker-reload-library = Reload Library
picker-choose-zotero-profile = Choose Zotero Profile…
picker-zotero-profile = Zotero Profile
picker-choose-library-menu = Choose Library…
picker-format-label = Citation format
picker-quit-label = Quit Ibis
picker-settings = Settings…
picker-zotero-libraries = Zotero libraries
picker-search-in = Search in
picker-all-libraries = All Libraries
picker-my-library = My Library
picker-group-library = Group { $id }
picker-keep-one-library = Keep at least one Zotero library on.
key-title = Keyboard Shortcuts
key-close = Close
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
# Names inside these sentences arrive isolated (⁨…⁩), so a right-to-left name keeps its place.
reader-indicator = p. { $printed } · PDF page { $index } of { $count }
reader-pdf-page = PDF { $index }
setup-found-on =
    { $platform ->
        [macos] Found on this Mac
        [windows] Found on this PC
       *[other] Found on this computer
    }
setup-zotero-access-off = To read your references, in Zotero › Settings › Advanced turn on “Allow other applications on this computer to communicate with Zotero”.
setup-zotero-open = Zotero is open.
setup-zotero-installed = Zotero is on this computer.
setup-bib-found =
    { $count ->
        [one] 1 .bib file found.
       *[other] { $count } .bib files found.
    }
session-remove-named = Remove { $name }
session-undo-remove = Undo Remove
session-undo-remove-named = Undo Remove { $name }
session-absent =
    { $count ->
        [one] 1 reference isn’t in the open library, so it is left out.
       *[other] { $count } references aren’t in the open library, so they are left out.
    }
session-left-out = Left out because this style can’t write them: { $names }.
session-bibliography-in = Bibliography in { $style }
session-saved = Saved { $name }.
writing-remove-question = Remove { $name } from Ibis? Its bibliography file stays where it is.
writing-remembered = Saved for { $name }.
picker-project-state =
    { $state ->
        [in] In { $name }
        [missing] Missing from { $name }
        [could-not-check] Couldn’t check { $name }
        [repeated] Key appears more than once
       *[may-name-another] Key may name another reference
    }
picker-project-lines =
    { $count ->
        [one] line { $lines }
       *[other] lines { $lines }
    }
picker-project-reading = Reading { $name }
picker-project-damaged = Couldn’t read all of { $name }, so nothing is called missing.
picker-project-failed = Couldn’t read { $name }.
# Why a project bibliography could not be read, after picker-project-failed.
picker-project-problem-unavailable = It is not there, or not on this computer yet.
picker-project-problem-not-a-file = It is not a file.
picker-project-problem-too-large = It is larger than 64 MiB.
picker-project-problem-not-utf8 = It is not UTF-8 text.
picker-project-problem-changing = It is still changing.
picker-project-problem-not-bibtex = It must be a BibTeX or BibLaTeX file.
picker-project-checking = Checking against { $name }
picker-project-check-lines =
    { $count ->
        [one] Check line { $lines }.
       *[other] Check lines { $lines }.
    }
picker-stop-project = Stop Checking { $name }
picker-copy-project-entry = Copy Entry for { $name }
picker-project-entry =
    { $outcome ->
        [copied] Copied the entry for { $key } as your file has it, including any file or note field. Check it, then add it to { $name }.
        [no-original] This reference has no original BibTeX entry here. Export it from your reference manager in your project’s dialect, keeping its key.
        [changed] The library or the bibliography changed. Search again, then try again.
        [repeated] The library file repeats this key, so Ibis can’t tell which entry to copy.
       *[dependency] This entry relies on a crossref, xdata or @string that can’t come with it. Export it from your reference manager, keeping its key.
    }
picker-editor-waiting = Citing for your editor in { $format }. Esc cancels.
# The header names the format an editor asked for; this line says only who asked.
picker-editor-asking = Citing for your editor. Esc cancels.
picker-send = Send · { $format }
picker-delivery-editor = Editor
picker-delivery-clipboard = Clipboard
picker-delivery-paste = Automatic paste
picker-find-source = Find { $name }…
picker-find-source-unnamed = Find…
picker-show-source-folder =
    { $platform ->
        [macos] Show in Finder
       *[other] Show in Folder
    }
picker-open-source = Open { $name }
picker-open-library = Open library
picker-source-open-failed =
    { $source ->
        [zotero] Zotero could not open. Try again.
       *[file] The source file could not open. Try again.
    }
picker-warning-count =
    { $count ->
        [one] 1 warning
       *[other] { $count } warnings
    }
picker-copy-citations =
    { $count ->
        [one] Copy 1 citation
       *[other] Copy { $count } citations
    }
picker-entry-output =
    { $count ->
        [one] { $style } entry
       *[other] { $count } { $style } entries
    }
picker-missing-citations = Not in your library: { $keys }
picker-matched-in =
    { $field ->
        [doi] Matched in the DOI
        [keyword] Matched in the keywords
        [container] Matched in the journal or book
        [abstract] Matched in the abstract
        [identifier] Matched in an ISBN, PubMed ID or arXiv ID
       *[other] Matched in { $field }
    }
# The list's heading when it shows recent references; $state is cited or added.
picker-recent-heading =
    { $state ->
        [cited] Cited here recently
       *[added] Recently added
    }
# After a copied citation's excerpt: how many more references it holds.
picker-more-references = +{ $count } more
picker-result-count =
    { $count ->
        [one] 1 result
       *[other] { $count } results
    }
picker-selection-count =
    { $count ->
        [one] 1 reference selected for citation
       *[other] { $count } references selected for citation
    }
picker-shortcut-not-used = Your shortcut setting was not used: { $reason }
picker-shortcut-unavailable = The shortcut { $shortcut } is not available: { $reason }
key-copy-entries =
    { $count ->
        [one] Copy { $style } entry
       *[other] Copy { $count } { $style } entries
    }
library-opening = Opening { $name }…
library-opening-unnamed = Opening library…
library-reloading = Reloading { $name }…
library-reloading-unnamed = Reloading library…
library-zotero-not-read = { $name } could not be read. { $reason }
library-open-zotero-then-reload = Open Zotero, then reload.
library-file-not-opened = The library could not be opened: { $name }. { $reason } Choose it again or reconnect the drive.
library-the-chosen-file = the chosen file
library-empty-title = No references in { $name }
library-empty-title-unnamed = No references in this library
library-checking = Checking library…
library-details-unavailable = Library details unavailable
library-none-chosen = No library chosen
library-not-opened = not opened
library-moved = moved
library-not-updated = not updated
library-no-references = no references
library-references =
    { $count ->
        [one] { $count } reference
       *[other] { $count } references
    }
library-skipped =
    { $count ->
        [one] { $count } entry skipped
       *[other] { $count } entries skipped
    }
library-to-check = { $count } to check
library-age-opening = opening…
library-age-reloading = reloading…
library-updated = updated { $ago }
library-checked-at = checked { $ago }, { $time }
library-last-read-at = last read { $ago }, { $time }
library-zotero-unreachable = Zotero could not be reached
library-file-unreadable = the file could not be read
library-where = ({ $lines })
library-key-users = { $count } use the key { $key }
library-others-share =
    { $count ->
        [one] 1 other shares a key
       *[other] { $count } others share a key
    }
library-could-not-read =
    { $skipped ->
        [one] it could not be read
       *[other] { $count } could not be read
    }
library-skipped-because =
    { $skipped ->
        [one] { $skipped } entry skipped: { $reasons }.
       *[other] { $skipped } entries skipped: { $reasons }.
    }
library-check = Check { $lines }.
library-check-library = Check the library.
library-line = line { $line }
library-line-why = line { $line }: { $why }
library-and-more = and { $count } more
library-lines = lines { $lines } and { $last }
library-lines-and-others = lines { $lines } and { $last }, and others
library-work-limit = The library is too large to read in full.
library-cloud-synced = It is in a cloud-synced folder, so changes can arrive late.
library-zotero-newer = Zotero is newer than this version of Ibis was tested with. Results look right, but update Ibis when you can.
library-keys-unavailable = Better BibTeX keys could not be read, so some items have none.
time-just-now = just now
time-on = on { $date }
time-short =
    { $unit ->
        [now] now
        [minute] { $n } min
        [hour] { $n } h
       *[day] { $n } d
    }
# Short word for a CSL item type, read by screen readers; other types say none.
row-item-type =
    { $type ->
        [article] Article
        [article-journal] Article
        [article-magazine] Article
        [article-newspaper] Article
        [book] Book
        [chapter] Chapter
        [dataset] Dataset
        [entry-dictionary] Entry
        [entry-encyclopedia] Entry
        [manuscript] Manuscript
        [paper-conference] Paper
        [patent] Patent
        [post] Post
        [post-weblog] Post
        [report] Report
        [review] Review
        [review-book] Review
        [software] Software
        [speech] Talk
        [standard] Standard
        [thesis] Thesis
        [webpage] Web page
       *[other] {""}
    }
copied-notice-pasted-into = Pasted into { $app }
copied-notice-pasted = Pasted
# Every "copied, not pasted" sentence says why and ends with its fix, written
# with ⌘V; other systems show Ctrl+V.
copied-notice-reason =
    { $reason ->
        [paste-off] Copied. Pasting is off in Settings. Press ⌘V to paste.
        [unsupported-system] Copied. Ibis can’t paste on this system yet. Press ⌘V to paste.
        [no-target] Copied. There was no app to paste into. Press ⌘V to paste.
        [unknown-app] Copied. Automatic paste is not approved for this app. Press ⌘V to paste.
        [copy-only] Copied. Copy Only is set for this app. Press ⌘V to paste.
        [nonstandard-shortcut] Copied. This app’s paste key does something else. Use its own paste command.
        [target-changed] Copied. The target app or window changed. Press ⌘V to paste.
        [clipboard-changed] Copied. The clipboard changed since. Copy the citation again.
        [paste-expired] Copied. The paste expired. Press ⌘V to paste.
        [no-permission] Copied. Ibis needs your OK to paste. Press ⌘V to paste.
        [accessibility-required] Copied. Ibis needs Accessibility access to paste safely. Press ⌘V to paste.
        [terminal] Copied. Not pasted into a terminal. Press ⌘V to paste.
        [remote-desktop] Copied. Not pasted into a remote desktop. Press ⌘V to paste.
        [virtual-machine] Copied. Not pasted into a virtual machine. Press ⌘V to paste.
        [secure-field] Copied. This field hides what you type. Press ⌘V to paste.
        [secure-input] Copied. Secure typing is on, possibly in another app. Press ⌘V to paste.
        [elevated-app] Copied. Not pasted into an administrator app. Press ⌘V to paste.
        [key-release-failed] Copied. Ibis couldn’t release the keys it pressed. Press and release Ctrl and V to paste.
        [selection-not-shown] Copied. Ibis replaces only text it showed you. Press ⌘V to replace it.
        [citation-not-selected] Copied. Ibis replaces a citation only when it is selected. Select it, then press ⌘V.
       *[paste-unavailable] Copied. Ibis couldn’t confirm where the text would go. Press ⌘V to paste.
    }
update-title = Updating Ibis
app-newer-installed = A newer Ibis is installed. Quit and reopen to use it.
update-downloading = Downloading
update-installing = Checking and installing
update-failed = Update not installed. Use Check for Updates in Settings › About.
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

citations-documents = In Word, LibreOffice and other documents
citations-keys = In LaTeX, Markdown and Obsidian
citations-keys-description = Used where you write citation keys, and in apps Ibis does not know.
citations-apps = Formats remembered for apps
citations-forget = Forget
citations-writings = Projects
settings-open-file = Open settings file
settings-open = Open
settings-file-description = Edits to settings.json apply while Ibis is running.
picker-use-usual = Use { $format } in { $app } again
picker-stop-writing = Stop using { $name } in { $app }

copied-notice-suggested = Pasted in { $format }
copied-notice-suggestion = Suggested { $format }
copied-notice-change = Change

picker-find-open = Open PDF, or show item in Zotero
shortcuts-action-find-open = Open PDF, or show item

citations-unlisted = Custom format

picker-format-suggestion = Suggested until you choose a format.
## The palette: the format chooser and More (ui/Palette).
palette-back = Back
palette-run = Run
palette-recent = Recent
palette-citation = This citation
palette-library = This library
palette-ibis = Ibis
palette-peek = Peek at Reference
palette-actions-title = Actions
palette-search-actions = Search actions
palette-no-actions = No matching actions
picker-format-title = Format
palette-search-formats = Search formats
palette-no-formats = No matching formats
palette-choose = Choose
palette-no-matches = No matches
palette-use-again = Use { $format } in { $app } Again
palette-use-for-app = Use for { $app }
palette-use-default = Use Everywhere
palette-use-this-paste = Use for This Paste
palette-style-unavailable = Not available just now
palette-preview-example = For example
palette-preview-key = What Ibis pastes
palette-preview-more = and { $count } more
palette-preview-unavailable = No preview for this format.
palette-words-source = open file folder zotero show library
palette-words-reveal-source = finder explorer folder file location
palette-words-choose-source = library file bib zotero open switch change
palette-words-project-entry = bibliography bib entry missing copy
palette-words-project-choose = bibliography bib check project
palette-words-project-stop = bibliography bib check stop
palette-words-format = style output apa pandoc latex syntax share export bibtex doi json
palette-words-share = share export send copy references
palette-words-peek = details preview quick look info
palette-words-change-last = edit replace previous citation again
palette-words-reference-list = bibliography references works cited list entries paste copy
palette-words-citation-focus = focus token reference citation
palette-words-citation-edit = edit page locator details citation
palette-words-citation-move-left = move reorder previous token reference citation
palette-words-citation-move-right = move reorder next token reference citation
palette-words-citation-remove = remove delete token reference citation
palette-words-keep-order = order sort sequence
palette-words-session-references = bibliography history cited list references
palette-words-forget-recent = clear history privacy recents
palette-words-pin = pin float stay keep open
palette-words-keys = keyboard shortcuts keys help hotkeys
palette-words-settings = preferences options configure
palette-words-quit = exit close stop
reason-strict-citation = Strict mode requires a complete citation without losses or source problems.
    .cli = Check the reported source or citation options, or retry without --strict to review the findings.
    .agent = Fix the source or citation options, or ask the user to review an ordinary citation’s findings.
    .app = Check the source and citation options before trying again.

cli-strict-help = Refuse citations with losses, refusals or source problems
mcp-strict-help = Refuse losses, refusals or source problems.
mcp-citation-hidden = This citation contains hidden characters and cannot be returned unchanged. Ask the user to fix the source or choose another output.
mcp-citation-source-failed = A source could not be refreshed; this citation uses its last good copy. The receipt records its last successful read.
mcp-citation-source-warnings = A cited source has warnings; its receipt records the warning count.
mcp-cite-description = Cite refs from search, in order, with optional locators. Use the returned text unchanged and tell the user about notices, losses and warnings.

# The desktop's bundled command. Never execute another program to check its version.
command-install = Install the ibis command
command-install-button = Install
command-install-description = Search and cite from a terminal with the same library and settings.
command-remove = Remove Command
command-checking = Checking the ibis command…
command-missing = Not installed
command-matching = ibis { $version } — matches this app
command-different = ibis { $version } — a different version from this app
command-foreign = Another ibis is used: { $path }. Version unavailable.
command-profile = Add this line to your shell profile, then reopen your terminal. Ibis checks the PATH available to the app.
command-copy = Copy
command-copied = Copied
command-failed = Could not change the ibis command. Check the folder permissions and try again.

# Source status and reading warnings; separate from citation warnings.
source-warning-zotero-newer-than-tested = Zotero is newer than this version of Ibis was tested with. Results look right, but update Ibis when you can.
source-warning-cloud-synced-directory = The library is in a cloud-synced folder, so changes can be late.
source-warning-citation-keys-unavailable = Better BibTeX keys could not be read, so some items have none.
source-warning-syntax = An entry could not be read, so Ibis skips it.
source-warning-duplicate-key = More than one entry uses the same key or id, so Ibis skips them all.
source-warning-field = A field could not be fully converted; check this entry.
source-warning-work-limit = The library is too large to read in full.
source-warning-invalid-attachment = An attachment link is not valid.
source-warning-pdfs-not-kept = This folder has PDFs Ibis has not kept yet.

# Per-setting reset actions name their setting for assistive technology.
settings-use-default = Use Default for { $setting }
settings-default-restored = Default restored for { $setting }
settings-undo-default = Restore Default for { $setting }
settings-details = Details…
settings-shortcut-words = keyboard hotkey keybinding

## Citation diagnostics carried to clients before rendering.
cli-ambiguous-suggestion = This key is ambiguous. Choose a result’s id from this search, then use ibis show --id ID_JSON with the same libraries.
cli-ambiguous-search-keys = note: { $count } hits have ambiguous citation keys and were omitted; search with --json and use the result’s id.
problem-empty-selection = There is nothing to cite.
problem-too-many-items = A citation can hold at most { $count } references.
problem-invalid-locator = Correct the page or location for this reference.
    .named = The page or location for { $reference } is empty or has control characters.
problem-invalid-rich-text = Correct the text before or after this reference.
    .named = The text before or after { $reference } has characters Ibis cannot write.
problem-output-too-large = The citation is too large to write.
problem-unsupported-form = This output cannot write the chosen citation form.
    .app-named = { $reference }: { $form } cannot be expressed in this output or group.
    .cli-form = Cannot cite { $reference } in { $output } with --form { $form }. Try --form normal.
    .cli-options = Cannot cite { $reference } with { $options } in { $output }. Try without them.
    .named = { $output } cannot write { $reference } in this form or with these options.
    .cli = Cite it on its own, or choose another output.
    .agent = Try form normal, or another format or style.
    .app = Choose Name in brackets, cite it alone, or choose another output.
    .bibliography = Remove the references this style cannot write from this list.
problem-missing-item = This reference was removed while citing.
    .named = { $reference } changed or was removed while citing.
    .cli = Try again.
    .agent = Search again.
    .app = Search again.
problem-changed-item = This reference changed while citing.
    .named = { $reference } changed while citing.
    .cli = Try again.
    .agent = Search again.
    .app = Search again.
problem-missing-key = This reference has no citation key.
    .named = { $reference } has no citation key.
    .cli = Cite it with --style instead.
    .agent = A key syntax cannot name it; use a style.
    .app = Give it one in your library, or choose a style.
problem-missing-details = This reference has no details to cite yet. Add them in its library.
    .named = { $reference } has no details to cite yet.
    .cli = Add a title or author to its entry, then cite it again.
    .agent = A formatted citation needs bibliographic details from its library.
    .app = Add a title or author to it in its library.

problem-ambiguous-key = Several references share this citation key.
    .named = Several references share the citation key of { $reference }.
    .cli = Give each one its own key in your library.
    .agent = Use a style, or ask the person to give each its own key.
    .app = Give each one its own key in your library.
problem-conflicting-key = This reference has conflicting citation keys.
    .named = { $reference } has more than one citation key in its library.
    .cli = Keep one.
    .agent = Use a style, or ask the person to keep one key.
    .app = Keep one in your library, or choose a style.
problem-invalid-key = This output cannot write the reference’s citation key.
    .named = The citation key of { $reference } has characters { $output } cannot write.
    .agent = Use a style, or ask the person to change the key.
problem-unsupported-output = This citation output is unavailable.
    .named = There is no style { $output }.
    .cli = List the styles with: ibis styles
    .agent = The styles tool lists them.
problem-invalid-locale = This citation language is unavailable.
    .named = That citation language is not available.
    .cli = List them with: ibis styles --locales
    .agent = The styles tool lists the locales.
problem-cancelled = The citation was cancelled.
problem-formatter-unavailable = The citation could not be written just now.
    .named = The citation could not be written just now. Try again.
problem-annotation-mismatch = This annotation belongs to another reference.
    .named = The annotation given for { $reference } belongs to another reference.
problem-not-citable = This item cannot be cited as a reference.
    .named = { $reference } is not a reference, such as a note, so it cannot be cited.
problem-not-from-zotero = This reference was not read from Zotero.
    .named = { $reference } was not read from a Zotero library, so Zotero cannot find it.
    .cli = Read your Zotero library with --zotero, or choose another output.
    .agent = Use another format, or ask the person to cite it from their Zotero library.
    .app = Cite it from your Zotero library, or choose another output.
problem-missing-doi = A selected reference has no usable DOI.
    .named = { $reference } has no usable DOI.
    .cli = Share it with --format bibtex instead.
    .agent = Use format bibtex instead.
    .app = Choose BibTeX in the citation format menu.

## Native command failures.
app-failure-cli-status = The command line setup could not be read.
app-failure-set-cli-installed = The command line setup could not be changed.
app-failure-copy-cli-profile = The shell setup could not be copied.
app-failure-hide-picker = The picker could not be closed.
app-failure-copy-editor-endpoint = The editor address could not be copied.
app-failure-copy-version = The version could not be copied.
app-failure-set-window-size = The window size could not be saved.
app-failure-undo-window-position = The window position could not be restored.
app-failure-copy-project-entry = The reference entry could not be copied.
app-failure-finish-paste-guide = Paste setup could not be finished.
app-failure-row-context = The reference details could not be read.
app-failure-find-zotero = Zotero could not be checked.
app-failure-find-sources = Libraries could not be found.
app-failure-open-found-library = This library could not be opened.
app-failure-configure-shortcuts = Keyboard shortcut setup could not be opened.
app-failure-use-sample-library = The sample references could not be opened.
app-failure-use-zotero-libraries = Choose at least one Zotero library, without repeats.
app-failure-recent-references = Recent references could not be read.
app-failure-search = The search could not be completed.
app-failure-drag-paste-app = The Ibis app could not be dragged.

app-command-no-library = Choose a library to search.
app-project-error-invalid-name = Give the project a name of 1 to 80 characters, without control characters.
app-project-error-duplicate-name = A project already has that name. Choose another name.
app-project-error-not-found = That project is no longer available. Choose a project again.
app-project-error-too-many = Ibis can remember at most 64 projects. Remove one before adding another.
app-project-error-invalid = The project definitions are not valid. Check Settings.
app-project-error-changed = The project or destination changed. Open the format menu again.
app-project-error-editor = This editor request chooses its own output. Finish it before changing projects.
app-project-error-output = This project’s format is not available in the picker. Choose another format.
app-settings-error-unreadable = The settings file could not be read or saved.
app-settings-error-invalid = The settings file is not valid. Open Settings to review it.
    .settings = settings.json isn’t valid, so Ibis is using your last good settings.
app-settings-error-newer = Settings were written by a newer version of Ibis. Update Ibis before changing them.
app-settings-error-read-only = Settings are read-only. Make the settings file writable before trying again.
app-command-output-not-saved = The format choice could not be saved.
app-command-delivery-not-saved = The format was saved, but the paste choice could not be saved.

app-command-page-missing = Reference { $number }: { $label } needs a page or range. Enter one after the label, or clear { $label }.
app-command-page-invalid = Reference { $number }: { $label } needs a valid page or range after its label. Correct it, or clear { $label }.
app-command-locator-missing = Reference { $number }: { $label } needs a value. Enter one after the label, or clear the locator.
app-command-locator-invalid = Reference { $number }: { $label } needs a valid value. Correct it, or clear the locator.
app-reference-number = Reference { $number }
app-command-locator-conflict = Reference { $number }: a typed locator and a captured locator cannot be combined.
app-command-bibliography-unsupported = This style can’t write these references: { $references }. Remove them from this list to continue.

## MCP validation facts.
mcp-error-arguments = arguments must be an object.
mcp-error-query = query must be a few words: an author surname, title words, a year or a key.
mcp-error-shorthand = shorthand must be true or false.
mcp-error-reference = ref must be a ref from search, as text.
mcp-error-items = items must be a list of objects, each with a ref from search.
mcp-error-app = for_app must be a macOS bundle ID or Windows executable name.
mcp-error-keep-order = keep_order must be true or false.
mcp-error-output-choice = Give either format or style, not both.
mcp-error-style = style must be a style id, as text.
mcp-error-locale = locale must be a language tag, as text, such as de-DE.
mcp-error-key-locale = locale applies only to a style; a key syntax leaves language to the document’s processor.
mcp-error-form = form must be one of the values in this tool’s input schema.
mcp-error-records-or-items = Give records or items, not both; for_app needs items.
mcp-error-records-order = keep_order needs items; it is not supported with records.
mcp-error-records = records must be a list of CSL-JSON objects, each with an id and a type.
mcp-error-item = each item must be an object with a ref from search.
mcp-error-locator = the locator is not readable; write it like "p. 12", "ch. 3" or "12-14".
mcp-error-omit-author = omit_author must be true or false.
mcp-error-empty-output = This output produced no text for these items; try another form, format or style.
mcp-error-style-query = query must be text of at most 1,024 bytes.
mcp-error-limit = limit must be a whole number, 1 or more; at most { $limit } results are returned.
mcp-error-unknown-argument = { $tool } has no argument '{ $name }'; it takes: { $names }.
mcp-error-format = format must be one of: { $formats }.
mcp-error-text = { $name } must be text.
mcp-error-number-range = { $name } must be a whole number from 1 to { $limit }.
mcp-error-number-minimum = { $name } must be a whole number, 0 or more.
mcp-error-include-receipt = include_receipt must be true or false.
mcp-error-stale-reference = This reference changed since search returned it; search again and check it is the one you meant.
mcp-error-attachment-text = The user has not let AI apps read attachment text from this library. Tell them; they can allow it for this library in Ibis settings.
mcp-error-no-library = No library is open. Ask the user to choose a library in Ibis, or to start `ibis mcp` with --file or --zotero, and then start a new session.
mcp-error-writes = This server was started without --allow-writes, so it cannot propose changes. Tell the user; they can add --allow-writes to the ibis mcp command in their AI app.
mcp-error-proposal = Ibis did not record this proposal: { $reason }
mcp-error-strict = { mcp-strict-help }
mcp-error-hidden-citation = { mcp-citation-hidden }
mcp-item-error = Item { $number }: { $reason }
mcp-item-name = Item { $number }
mcp-output-name = This output
mcp-tool-error = error: { $code }. { $reason }
mcp-error-busy = Ibis is busy. Retry this tool call in a moment.

cli-key-suggestions = Did you mean { $keys }?
cli-key-not-found = no reference has the key '{ $key }'{ $choices }
    Try: { $command }{ $route }

mcp-startup-failed = The library could not be opened when this session started. { $error } Resolve the problem, then start a new MCP session.

app-bibliography-unavailable = The bibliography could not be prepared. Try again.
app-bibliography-not-saved = The bibliography could not be saved. Try again.
app-bibliography-name-taken = A file with that name is already there. Choose another name.
app-bibliography-format = Save it as .rtf, .html or .txt.
app-clipboard-unavailable = The clipboard could not be written. Try again.

app-recent-history-not-saved = Recent citations could not be forgotten. Try again.
app-project-citation-conflict = Citation keys { $keys } identify different works in the bibliography. Check the bibliography before copying.

app-settings-error-changed = Settings changed while saving. Try again.
app-settings-error-busy = Settings are being saved by another Ibis. Wait for it to finish, then try again.
app-settings-error-document-format = Choose a style for documents.
app-settings-error-key-format = Choose a key format for LaTeX, Markdown and Obsidian.
app-project-error-context-unavailable = The document’s bibliography and citation style could not be read. Try again.
app-project-error-invalid-context = Name one bibliography and CSL style as plain values in the document.
app-project-error-ambiguous-context = More than one project matches this bibliography and citation style. Choose a project by name.
mcp-error-citation-format = This format exports records or Zotero markers. Choose a citation format.

app-failure-open-settings-file = The settings file could not be opened. Try again.

## Command-line requests, diagnostics and reports.

cli-keys-unreadable = the key list could not be read; give UTF-8 text with one key per line.
cli-keys-too-large = the key list is more than 1 MiB; send fewer keys at a time.
cli-keys-not-utf8 = the key list must be UTF-8 text, with one key per line.
cli-keys-empty = the key list is empty; give at least one citation key.
cli-key-file-unavailable = the key file is missing, unreadable or not a regular file; check its path, or use - for stdin.
cli-key-file-unopened = the key file could not be opened; check its path and permissions.
cli-key-file-not-regular = --keys-from needs a regular file, or - for stdin.
cli-key-file-timeout = the key file could not be read in time; try again.
cli-toggle-unprepared = could not prepare the toggle; check that the temporary folder can be written, then try again.
cli-app-libraries-unavailable = --app-libraries is not available here; name the libraries to read.
cli-records-with-libraries = --records cites only the records on stdin; leave out the library options.
cli-search-words-needed = give some words to search for: an author, title words, a year or a key.
cli-search-words-too-long = the search words are too long; use fewer.
cli-id-not-found = no open library holds the reference that --id names.
cli-attachment-not-found = no open library holds that attachment, or its reference changed; run ibis outline again for a new id.
cli-find-in-words = give the words to find, up to 200 characters.
cli-records-too-large = the records on stdin are more than 1 MiB; send fewer at a time.
cli-bibliography-needs-style = --bibliography needs a style: give --style, or a --format that is a style.
cli-project-too-many-keys = at most 1000 keys can be checked at once.
cli-styles-limit = --limit must be between 1 and 20000.
cli-request-help = stdin must hold one CitationRequest JSON object; see: ibis schema cite
cli-records-help = stdin must hold one JSON array of CSL-JSON records; see: ibis schema records
cli-id-help = --id must be a reference’s id, the JSON that ibis search --json prints.
cli-key-option-owner = --{ $flag } must follow the key it belongs to.
cli-key-option-repeated = --{ $flag } is given twice for '{ $key }'.
cli-locator-unreadable = --locator '{ $locator }' is not a locator Ibis reads; write it like "p. 12", "ch. 3" or "12-14".
cli-method-unknown = there is no method '{ $method }'; choose one of: { $names }
cli-style-unknown = there is no style '{ $style }'; list the styles with: ibis styles
cli-locale-unknown = there is no locale '{ $locale }'; list them with: ibis styles --locales
cli-params-help = --params must be the JSON parameters of { $method }; see: ibis schema { $method }
cli-source-findings = note: a library has warnings or could not refresh; see: ibis libraries{ $libraries }{ $json }
cli-keys-waiting = waiting for keys, one per line; end with Ctrl-D.
cli-search-keyless =
    { $plural ->
        [one] note: { $count } hit has no citation key; see ibis search { $query }{ $libraries } --json
       *[other] note: { $count } hits have no citation key; see ibis search { $query }{ $libraries } --json
    }
cli-search-more =
    { $plural ->
        [one] note: showing the first { $count } match; see more with --limit { $next }
       *[other] note: showing the first { $count } matches; see more with --limit { $next }
    }
cli-search-narrow = note: showing the first { $count } matches; use more distinctive search words
cli-file-name-invalid = cannot open { $path }: its name is not valid text; rename it.
-cli-name-library = name one with --file PATH, --zotero or --zotero-group ID
cli-app-settings-timeout = the Ibis app’s settings could not be read in time; try again, or { -cli-name-library }.
cli-chosen-library-unopened = cannot open the library chosen in { $app }: { $reason } Choose it again there, or { -cli-name-library }.
cli-app-settings-missing = no library is named and the Ibis app has no settings here; { -cli-name-library }.
cli-app-formats-unreadable = cannot read the formats chosen in { $app }: { $reason }. Open { $app } to repair its settings.
cli-app-libraries-unreadable = cannot read the libraries chosen in { $app }: { $reason }. { $fix }, or { -cli-name-library }.
cli-app-library-unchosen = no library is chosen in { $app } yet; choose one there, or { -cli-name-library }.
cli-app-library-relative = the library chosen in { $app } is not a full path; choose it again there, or { -cli-name-library }.
cli-app-settings-changed = its settings changed while saving; try again
cli-app-settings-permission = this program is not allowed to read its settings file; allow it to, or run it outside the sandbox
cli-app-settings-file-missing = its settings file is missing
cli-app-settings-unreadable = its settings file could not be read
cli-app-settings-invalid = its settings file is not valid
cli-app-settings-backup-unused = { $reason }, and Ibis does not use its backup here
cli-app-settings-update = Update this ibis command to match { $app }
cli-app-settings-repair = Open { $app } to repair its settings
cli-app-settings-zotero-invalid = its settings file is not valid (Zotero library entry { $position })
cli-app-settings-newer = its settings were written by a newer version of Ibis (format { $version })
cli-settings-no-folder = this computer has no folder for app settings (HOME or APPDATA is not set).
cli-settings-action-needed = choose export or import; see: ibis settings --help
cli-settings-backup-needed = name the backup file to restore.
cli-settings-folder-timeout = the settings folder did not respond in time; try again.
cli-settings-exported = Backed up { $app }’s settings to { $name }.
    Paste approvals and window places stay on this computer.
cli-settings-backup-made = a backup made on { $system } by Ibis { $version }
cli-settings-unchanged = { $app } already has the settings in { $backup }. Nothing changed.
cli-settings-would-restore = Restoring { $backup } would change { $app }’s settings:
cli-settings-restored = Restored { $backup } into { $app }:
cli-settings-library-not-found = Note: the backup’s bibliography file is not on this computer. It is kept, and Ibis finds it once the file is there.
cli-settings-shortcut-kept = Note: the backup has no opening shortcut for this system, so this computer’s is kept.
cli-settings-repairs-settings = Note: the settings file on this computer is damaged or missing. Restoring writes it again, and keeps any damaged file beside it.
cli-settings-dry-run = Nothing changed. Run it again without --dry-run to restore.
cli-settings-restored-next = The replaced settings are kept as settings.json.bak. Paste approvals and window places stay as they were.
    Ibis uses the restored settings the next time it opens; quit and reopen it if it is running.
cli-writing-style-unavailable = That citation style is unavailable. List styles with: ibis styles
cli-writing-output-unavailable = That output is unavailable. List outputs with: ibis outputs
cli-writing-bibliography-needed = Choose a BibTeX or BibLaTeX bibliography with a .bib file name.
cli-writing-folder-unavailable = The current folder is unavailable. Give the bibliography’s full path.
cli-writing-action-needed = Choose list, create, rename, update or remove.
cli-writings-empty = No projects yet. Create one with: ibis projects create NAME --format pandoc
cli-writing-format-needed = Choose --format or --style.
cli-writing-citation-unavailable = The project citation is unavailable.
cli-writing-saved =
    { $action ->
        [create] Created project { $name }.
        [rename] Renamed project { $name }.
        [remove] Removed project { $name }. Its bibliography file was kept.
       *[update] Updated project { $name }.
    }
cli-no-library = No library is open; add --file <library> or --zotero.
cli-search-no-match-terminal = No references matched. Try a shorter query.
cli-search-no-match = No references found.
cli-libraries-empty = No libraries loaded. Open one with --file <library>.
cli-styles-no-match = No style matches those words.
cli-share-written =
    { $count ->
        [one] Wrote 1 file, { $size }, to { $path }.
       *[other] Wrote { $count } files, { $size }, to { $path }.
    }
cli-share-preview =
    { $count ->
        [one] 1 file, { $size }. Add --to PATH to write the pack.
       *[other] { $count } files, { $size }. Add --to PATH to write the pack.
    }
cli-share-not-in-list = the reading list leaves out what its style cannot write: { $keys }
cli-share-not-in-bibtex = References.bib leaves out what has no usable citation key: { $keys }
cli-share-without-pdf = no PDF inside its library could be added for: { $keys }
cli-outline-none = This reference has no attachments.
cli-outline-pages = text, { $count } pages
cli-outline-printed-pages = text, { $count } pages with printed page numbers
cli-outline-text = text, pages not known
cli-outline-not-indexed = no text: Zotero has not indexed it
cli-outline-no-file = no text: a link, with no file
cli-outline-too-large = no text: larger than Ibis reads
cli-outline-not-yet = no text: Ibis cannot read this file yet
cli-outline-missing = no text: no file where the library says
cli-outline-not-local = no text: not a plain file on this computer, such as a link or an online-only file
cli-outline-not-pdf = no text: the file is not a PDF
cli-outline-damaged = no text: the PDF is damaged
cli-outline-timed-out = no text: reading the PDF took longer than ten seconds
cli-read-page = [file page { $page }]
cli-read-printed-page = [file page { $page }, printed page { $label }]
cli-read-more = More text: { $command }
cli-find-in-none = No passage holds those words.
cli-find-in-more = The first { $count } passages; add words to narrow it.
cli-entry-no-original = This reference has no original BibTeX entry here: only a .bib library keeps one, and Zotero, CSL-JSON, RIS and CSV libraries don’t. To write an entry from its fields, run ibis cite KEY --format bibtex, or export it from your reference manager in your project’s dialect, keeping its key.
cli-entry-changed = The library file changed or could not be read. Reload it and try again.
cli-entry-repeated = The library file repeats this key, so Ibis cannot tell which entry to give.
cli-entry-dependency = This entry relies on a crossref, xdata or @string that cannot come with it. Export it from your reference manager, keeping its key.
cli-library-refresh-failed = note: this library could not be refreshed; showing the last successful read
cli-library-not-watched = note: changes are not picked up automatically
cli-warning-unknown-field = an unrecognised field was skipped; Ibis kept the reference
cli-warning-unknown-type = the entry’s type is not supported; Ibis skips this entry
cli-warning-duplicate-key = more than one entry uses this key or id; Ibis skips them all
cli-warning-syntax-entry = this entry could not be read; Ibis skips it
cli-warning-entry-too-large = the entry is too large to read; Ibis skips it
cli-warning-library-incomplete = Ibis could not read the library in full; some references may be missing
cli-warning-attachment-skipped = an attachment link is not valid; Ibis leaves it out
cli-warning-syntax = an entry could not be read, so Ibis skips it
cli-warning-field = a field could not be fully converted; check this entry
cli-warning-work-limit = the library is too large to read in full
cli-warning-invalid-attachment = an attachment link is not valid
cli-warning-pdfs-not-kept = the folder has PDFs Ibis has not kept yet; run ibis sources keep --file FOLDER
cli-warning-cloud-directory = the library is in a cloud-synced folder, so changes can be late
cli-warning-keys-unavailable = Better BibTeX keys could not be read, so some items have none
cli-warning-unknown = something in the library needs checking
cli-output-disk-full = the output could not be written because the disk is full; free some space and try again.
cli-output-unwritten = the output could not be written; check where it is sent and try again.
cli-file-busy = it is being written or cannot be read right now; try again in a moment.
cli-file-too-large = it is larger than Ibis reads, or too many files are open at once; split the library or open fewer files.
cli-file-missing = there is no file at that path; check the spelling or give the full path.
cli-file-is-folder = it is a folder; give the library file inside it.
cli-file-permission = this program is not allowed to read it; check its permissions, or run it outside the sandbox.
cli-zotero-unavailable = no local Zotero library was found or it could not be read; check that Zotero is installed and has been opened once, or use --file.
cli-zotero-damaged = its data could not be understood; open Zotero once so it can repair its database.
cli-zotero-newer = a newer version of Zotero wrote it; update Ibis.
cli-zotero-too-large = it is larger than Ibis reads at once.
cli-key-empty = a citation key cannot be empty.
cli-search-partial = No reference matches every word. Closest matches:
    { $rows }
cli-styles-showing = Showing { $shown } of { $total } styles; narrow it with: ibis styles WORDS
cli-project-unreadable = Couldn’t read { $name }: { $reason }.
cli-project-damaged = Couldn’t read all of { $name }: check line { $lines }.
cli-warning-text-kept = { $subject } could not be fully converted; Ibis uses the text as written
cli-warning-date = { $subject } could not be fully converted; Ibis kept the reference; check the date before citing
cli-warning-link = { $subject } is private or is not a web address; Ibis leaves it out
cli-warning-names = { $subject } could not be fully read; Ibis uses the names it could read
cli-warning-fields-kept = { $subject } could not be fully converted; Ibis kept the reference with the fields it could read
cli-warning-syntax-field = { $subject } could not be read; Ibis skips this entry
cli-warning-duplicate-key-named = more than one entry uses '{ $key }' as its key or id, so Ibis skips them all
cli-search-stale-at = note: { $name } could not be refreshed; showing results from { $time }
cli-search-stale = note: { $name } could not be refreshed; showing the last successful read
cli-file-unopened = cannot open { $path }: { $reason }
cli-zotero-unreadable-profiles = cannot read the Zotero library: { $reason } Profiles: { $profiles }.
cli-zotero-unreadable = cannot read the Zotero library: { $reason }
cli-key-duplicate = the citation key '{ $key }' is used by more than one entry{ $at }, so Ibis skips them all; give each entry its own key.
cli-key-damaged = the entry '{ $key }'{ $at } could not be read, so Ibis skips it{ $reason }; fix it in your library.
cli-key-library-incomplete = cannot find '{ $key }': { $reason }
cli-key-ambiguous = several references use the citation key '{ $key }'; give each one its own key in your library.
cli-key-without-library = cannot look up '{ $key }': no library is open; add --file <library> or --zotero.
cli-key-not-found-search = no reference has the citation key '{ $key }'; find the right key with: ibis search <words>
cli-key-too-long = the citation key '{ $key }…' is too long.
cli-project-unavailable = the file is not there or is not on this computer yet
cli-project-not-file = it is not a file
cli-project-too-large = the file is larger than 64 MiB
cli-project-not-utf8 = the file is not UTF-8 text
cli-project-changing = the file is still changing
cli-project-not-bibtex = a bibliography must be a BibTeX or BibLaTeX file
cli-project-fields-differ = this entry and the library differ
cli-project-case-differs = a key differs only by letter case
cli-project-alias = it is another entry’s alias
cli-project-helper = it belongs to an @xdata entry, which cannot be cited
cli-project-commented = it is inside @comment, which some tools still read
cli-project-key-doubt = Key may name another reference (line { $line }): { $reason }
cli-destination-paste = note: Ibis pastes into { $name }.
cli-destination-unknown = note: Ibis does not know this app, so it asks before pasting.
cli-destination-unsupported = note: Ibis copies here, because it cannot paste on this system yet.
cli-destination-terminal = note: Ibis copies for { $name }, because a paste runs there as typed input.
cli-destination-remote = note: Ibis copies for { $name }, because keystrokes reach another system there.
cli-destination-shortcut = note: Ibis copies for { $name }, because its paste key does something else.
cli-destination-copy = note: Ibis copies for { $name }.
cli-doctor-all-good = All good.
cli-doctor-one-finding = 1 thing to look at.
cli-doctor-findings = { $count } things to look at.
cli-doctor-last-read = Last good read: { $age }.
cli-doctor-check-failed = Check failed.
cli-search-shorthand = Cited with: { $cited }; searched for: { $words }
cli-project-key-in = In { $name } (line { $line })
cli-project-key-missing = Missing from { $name }
cli-project-key-unchecked = Couldn’t check { $name }
cli-project-key-repeated = Key appears more than once (lines { $lines })
cli-citation-language = note: written in { $language }.
cli-library-stale-note = (last successful read; refresh failed)
cli-library-watch-note = (changes are not picked up automatically)
cli-greeting = Ibis { $version } finds references and cites them anywhere.

    Try it:
      ibis search lantern --file refs.bib       find a reference
      ibis cite vale2024lanterns --file refs.bib --style apa
                                                make a citation
      ibis doctor --file refs.bib               check a library

    With the Ibis app installed you can leave out --file.
    Add --json to any command for scripts and agents. All commands: ibis --help

# Settings and ibis agents share these app connections.
settings-ai-apps = AI Apps
ai-apps-group = Apps on this computer
ai-apps-command-line = Command line
ai-apps-description = Connect Ibis to the AI apps on this computer.
ai-apps-words = agents MCP Claude Codex Cursor VS Code Gemini Windsurf LM Studio ChatGPT AI apps
proposals-title = Proposed changes
proposals-description = Review and apply library changes proposed by AI apps.
proposals-words = proposals changes add reference
proposals-checking = Checking proposed changes…
proposals-empty = No proposed changes
proposals-from = { $app } · { $library }
proposals-apply = Apply
proposals-reject = Reject
proposals-app = { $app } · { $count } proposals
proposals-reject-app = Reject All
proposals-applied = Applied; backup kept as { $name }.
proposals-failed = Could not complete this action. Try again.
proposals-read-failed = Could not read proposed changes. Return to this page to try again.
app-failure-waiting-proposals = { proposals-read-failed }
app-failure-apply-proposal = { proposals-failed }
app-failure-reject-proposal = { proposals-failed }
app-failure-reject-app-proposals = { proposals-failed }
command-words = command line terminal shell PATH agents MCP
ai-apps-checking = Checking AI apps…
ai-apps-not-connected = Not connected
ai-apps-connected = Connected. Start a new session in the app to use Ibis.
ai-apps-stale = The ibis command has moved or is missing.
ai-apps-different = This app has a different Ibis connection. Review the change below.
ai-apps-plugin = Connected through the Ibis plugin.
ai-apps-connect = Connect
ai-apps-disconnect = Disconnect
ai-apps-repair = Repair
ai-apps-replace = Replace…
ai-apps-command-needed = The ibis command is missing. Install it in Command line above so this app can start Ibis.
ai-apps-failed = Could not change the AI app connection. Check the app’s settings and folder permissions, then try again.
ai-apps-kept = Other servers and settings are kept. A backup is kept beside each changed file, the same as ibis agents in a terminal.
app-failure-agent-apps = { ai-apps-failed }
app-failure-connect-agent-app = { ai-apps-failed }
app-failure-disconnect-agent-app = { ai-apps-failed }

# Native macOS picker and menu bar.
picker-native-app = Ibis
picker-native-panel = Ibis, citation picker
picker-native-open = Open Ibis
picker-native-quit = Quit Ibis
picker-native-allow-paste = Allow Paste…
picker-native-paste-copy-mode = Ibis copies citations. Press ⌘V to paste.
picker-native-open-library = Open Library
picker-native-shortcut-unavailable = The Ibis shortcut is unavailable.
picker-native-shortcut-not-saved = Ibis could not confirm that the shortcut was saved.
picker-native-format = Format
picker-native-format-value = Format, { $format }
picker-native-citation = Citation
picker-native-search-hint = Search by title, author, year or key.
picker-native-choose-title = Choose a library
picker-native-choose-hint = Choose a BibTeX, CSL JSON, RIS or CSV file to search.
picker-native-library-missing = { $name } can’t be found
picker-native-library-scope = Library: { $name }
picker-native-missing-hint = It may have moved or been renamed.
picker-native-find-library = Find { $name }…
picker-native-zotero = Zotero
picker-native-untitled = Untitled
picker-native-cited = In citation
picker-native-journal-article = Journal article
picker-native-other-reference = Reference
picker-native-author-others = { $author } { row-et-al }
picker-native-unmatched-segment = Choose a reference for “{ $query }”.
picker-native-token = Citation: { $citation }
picker-native-active-token = Editing citation: { $citation }
picker-native-citation-draft = Citation draft
picker-native-locator-type = Locator type
picker-native-edit-page = Edit Page or Locator
picker-native-return = Return
picker-native-open-source = Open Source
picker-native-text-larger = Larger Text
picker-native-text-smaller = Smaller Text
picker-native-density = Rows
picker-native-copied = Copied. Press ⌘V to paste.
picker-native-access-title = Switch on { $app }
picker-native-access-drag = If it’s not in the list, drag this icon in.
picker-native-access-wait = This closes by itself once it’s on. Until then, citations are copied for ⌘V.
picker-native-move-earlier = Move Earlier
picker-native-move-later = Move Later
picker-native-reference-count = { $count } References
picker-native-read-locator = { $term } { $value }
picker-native-read-prefix = “{ $text }” before
picker-native-read-more =
    { $count ->
        [one] then 1 more reference
       *[other] then { $count } more references
    }
picker-native-copy-failed = The clipboard changed or could not be written.
picker-native-engine-disconnected = The Ibis engine stopped. Try again.
picker-native-engine-invalid = The Ibis engine sent an invalid response.
picker-native-engine-peer = Ibis could not verify its local engine.
picker-native-engine-newer = A newer Ibis engine is running. Build the native app with the newer Ibis.
picker-native-engine-launch = Ibis could not start its local engine.
picker-native-engine-busy = Another Ibis delivery is in progress.
picker-native-engine-changed = The citation changed. Review it before trying again.
picker-native-request-failed = Ibis couldn’t finish this request. Try again.
picker-native-before-example = see
picker-native-after-example = emphasis added
picker-native-form-narrative = Name in the text
picker-native-copy-citation = Copy Citation
picker-native-paste-short = Paste
picker-native-no-matches-in = No matches in { $name }

picker-native-remember-format = Remember for { $app }
picker-native-format-project = Project: { $name }
picker-native-format-project-blocked = Open { $name } in the desktop picker to check its bibliography before citing.
picker-native-use-usual = Use { $format } again
picker-native-forget-format = Forget format for { $app }
