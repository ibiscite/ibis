# Interface messages only. Citation language and text direction have separate owners.
# General Settings is the first slice; the other panels still use messages.ts.
settings-pages = Settings pages
settings-general = General
settings-shortcuts = Shortcuts
settings-about = About
settings-save-failed = Ibis could not save that. Try again.

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

general-picker-title = Picker
general-rows = Rows
general-density-compact = Compact
general-density-comfortable = Comfortable
general-density-spacious = Spacious
general-larger-text = Larger text
general-preview = Show what will be pasted
general-preview-description = The line above the footer
general-search-mark = Ibis mark in search
general-search-mark-description = In place of the magnifier
