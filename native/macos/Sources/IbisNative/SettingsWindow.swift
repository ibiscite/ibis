import AppKit
import CIbisNative
import SwiftUI

@MainActor
final class NativeSettingsWindow: NSWindowController, NSWindowDelegate, NSSearchFieldDelegate {
    private let preferences: NativePreferences
    private let shortcuts: OpeningShortcuts
    let searchField = NSSearchField()
    private weak var searchOpener: NSResponder?
    private var searchAnnouncement: Task<Void, Never>?

    init(engine: EngineClient, shortcuts: OpeningShortcuts, chooseLibrary: @escaping (NSWindow, URL?) -> Void = { _, _ in },
         applied: @escaping (SettingsResult, Bool) -> Void) {
        self.shortcuts = shortcuts
        let preferences = NativePreferences(engine: engine) { [weak shortcuts] saved, resetText in
            shortcuts?.restore(saved.shortcut)
            applied(saved, resetText)
        }
        self.preferences = preferences
        let window = SettingsHostWindow(shortcuts: shortcuts)
        window.isReleasedWhenClosed = false
        super.init(window: window)
        window.contentView = NSHostingView(rootView: NativeSettingsForm(
            preferences: preferences, shortcuts: shortcuts, searchField: searchField,
            chooseLibrary: { [weak window] file in
                if let window { chooseLibrary(window, file) }
            }, select: { [weak self] pane in self?.select(pane) },
            reveal: { [weak self] row in self?.reveal(row) }
        ))
        window.center()
        window.delegate = self
        let prompt = PickerWords.text(.settingsSearch)
        searchField.placeholderString = prompt
        searchField.setAccessibilityLabel(prompt)
        searchField.sendsSearchStringImmediately = true
        searchField.delegate = self
        searchField.target = self
        searchField.action = #selector(searchChanged)
        window.find = { [weak self] in self?.beginSearch() }
        select(.general)
    }

    required init?(coder: NSCoder) { nil }

    override func showWindow(_ sender: Any?) {
        super.showWindow(sender)
        window?.makeKeyAndOrderFront(sender)
        preferences.refreshPasteAccess()
        preferences.read()
    }

    func windowDidBecomeKey(_ notification: Notification) { preferences.refreshPasteAccess() }
    func windowDidResignKey(_ notification: Notification) { shortcuts.cancelRecording() }
    func windowWillClose(_ notification: Notification) {
        clearSearch(endInteraction: true)
        preferences.revealed = nil
    }

    func showPaste() { reveal(.paste) }
    var libraryExtensions: [String] { preferences.saved?.libraryExtensions ?? [] }
    func setLibraryBusy(_ busy: Bool) { preferences.libraryBusy = busy }
    func didSelectLibrary(_ saved: SettingsResult) { preferences.useSaved(saved, resetText: false) }
    func didRejectLibrary(_ error: Error) { preferences.libraryProblem = pickerError(error) }

    private func select(_ pane: SettingsPane, revealing row: SettingsSearchRow? = nil) {
        shortcuts.cancelRecording()
        clearSearch(endInteraction: true)
        preferences.pane = pane
        preferences.revealed = row
        window?.title = pane.title
    }

    private func beginSearch() {
        if window?.firstResponder !== searchField.currentEditor() {
            searchOpener = window?.firstResponder
        }
        window?.makeFirstResponder(searchField)
        searchField.selectText(nil)
    }

    private func clearSearch(endInteraction: Bool) {
        searchAnnouncement?.cancel()
        searchField.stringValue = ""
        searchField.currentEditor()?.string = ""
        if !preferences.search.query.isEmpty { preferences.search.update("") }
        if endInteraction {
            if searchField.currentEditor() != nil { window?.makeFirstResponder(nil) }
            searchOpener = nil
        }
    }

    @objc private func searchChanged(_ sender: NSSearchField) {
        guard sender.stringValue != preferences.search.query else { return }
        if !sender.stringValue.isEmpty, preferences.revealed != nil { preferences.revealed = nil }
        preferences.search.update(sender.stringValue)
        searchAnnouncement?.cancel()
        if preferences.search.isSearching, NSWorkspace.shared.isVoiceOverEnabled {
            searchAnnouncement = Task { [weak self] in
                try? await Task.sleep(for: .milliseconds(350))
                if !Task.isCancelled { self?.announceSearchHighlight() }
            }
        }
    }

    func controlTextDidChange(_ notification: Notification) {
        if let sender = notification.object as? NSSearchField { searchChanged(sender) }
    }

    func control(_ control: NSControl, textView: NSTextView, doCommandBy commandSelector: Selector) -> Bool {
        guard control === searchField, !textView.hasMarkedText() else { return false }
        switch commandSelector {
        case #selector(NSResponder.moveDown(_:)), #selector(NSResponder.moveUp(_:)):
            guard preferences.search.isSearching else { return false }
            let previous = preferences.search.highlighted
            preferences.search.move(commandSelector == #selector(NSResponder.moveDown(_:)) ? 1 : -1)
            if preferences.search.highlighted != previous {
                searchAnnouncement?.cancel()
                announceSearchHighlight()
            }
        case #selector(NSResponder.insertNewline(_:)):
            guard preferences.search.isSearching else { return false }
            if NSApp.currentEvent?.isARepeat != true, let row = preferences.search.highlighted { reveal(row) }
        case #selector(NSResponder.cancelOperation(_:)):
            if preferences.search.query.isEmpty {
                let opener = searchOpener
                clearSearch(endInteraction: true)
                if let opener { window?.makeFirstResponder(opener) }
            } else {
                clearSearch(endInteraction: false)
            }
        default: return false
        }
        return true
    }

    private func announceSearchHighlight() {
        guard window?.isKeyWindow == true, window?.firstResponder === searchField.currentEditor() else { return }
        let message = preferences.search.highlighted.map { "\($0.title), \($0.pane.title)" }
            ?? PickerWords.text(.settingsNoResults, preferences.search.query)
        NSAccessibility.post(element: searchField, notification: .announcementRequested, userInfo: [
            .announcement: message, .priority: NSAccessibilityPriorityLevel.medium.rawValue
        ])
    }

    private func reveal(_ row: SettingsSearchRow) { select(row.pane, revealing: row) }
}

@MainActor
private final class SettingsHostWindow: NSWindow {
    private let shortcuts: OpeningShortcuts
    var find: (() -> Void)?

    init(shortcuts: OpeningShortcuts) {
        self.shortcuts = shortcuts
        super.init(contentRect: NSRect(x: 0, y: 0, width: 744, height: 500),
                   styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false)
        contentMinSize = NSSize(width: 620, height: 390)
        titleVisibility = .hidden
    }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        if shortcuts.recordKey(event) { return true }
        if event.modifierFlags.intersection([.command, .option, .control, .shift]) == [.command],
           event.charactersIgnoringModifiers?.lowercased() == "f",
           (firstResponder as? NSTextInputClient)?.hasMarkedText() != true {
            find?()
            return true
        }
        return super.performKeyEquivalent(with: event)
    }

    override func sendEvent(_ event: NSEvent) {
        if event.type == .leftMouseDown || event.type == .rightMouseDown { shortcuts.cancelRecording() }
        if !shortcuts.recordKey(event) { super.sendEvent(event) }
    }
}

@MainActor
private final class NativePreferences: ObservableObject {
    @Published private(set) var saved: SettingsResult?
    @Published private(set) var busy = false
    @Published private(set) var problem: String?
    @Published private(set) var pasteGranted = false
    @Published var pane: SettingsPane = .general
    @Published var search = SettingsSearch()
    @Published var revealed: SettingsSearchRow?
    @Published var libraryBusy = false {
        didSet { if libraryBusy { libraryProblem = nil } }
    }
    @Published var libraryProblem: String?
    private let engine: EngineClient
    private let applied: (SettingsResult, Bool) -> Void

    init(engine: EngineClient, applied: @escaping (SettingsResult, Bool) -> Void) {
        self.engine = engine
        self.applied = applied
    }

    func read() { send(.readOnly, changing: false) }

    func refreshPasteAccess() { pasteGranted = ibis_native_paste_granted() }

    func openPasteAccess() {
        refreshPasteAccess()
        guard saved?.paste == true, !pasteGranted else { return }
        ibis_native_request_paste_access()
        if let pane = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility") {
            NSWorkspace.shared.open(pane)
        }
    }

    func change(appearance: Appearance? = nil, density: Density? = nil, largerText: Bool? = nil, paste: Bool? = nil) {
        guard saved != nil else { return }
        send(.patch(appearance: appearance, density: density, largerText: largerText, paste: paste), changing: true)
    }

    func useSaved(_ reply: SettingsResult, resetText: Bool) {
        saved = reply
        applied(reply, resetText)
    }

    private func send(_ request: SettingsRequest, changing: Bool) {
        guard !busy else { return }
        busy = true
        problem = nil
        Task {
            defer { busy = false }
            do {
                let reply = try await engine.settings(request)
                useSaved(reply, resetText: request.largerText != nil)
            } catch {
                if changing {
                    // A dropped reply can follow a committed write. Recover the saved view.
                    if let reply = try? await engine.settings(.readOnly) {
                        useSaved(reply, resetText: request.largerText != nil)
                    } else {
                        saved = nil
                    }
                } else {
                    saved = nil
                }
                problem = PickerWords.text(changing && saved != nil ? .settingsSaveFailed : .settingsReadFailed)
            }
        }
    }
}

private struct SettingsSearchHost: NSViewRepresentable {
    let field: NSSearchField

    func makeNSView(context: Context) -> NSSearchField { field }
    func updateNSView(_ field: NSSearchField, context: Context) {}
}

private struct NativeSettingsForm: View {
    @ObservedObject var preferences: NativePreferences
    @ObservedObject var shortcuts: OpeningShortcuts
    @FocusState private var focused: SettingsSearchRow?
    @AccessibilityFocusState(for: .voiceOver) private var spoken: SettingsSearchRow?
    @State private var flashed: SettingsSearchRow?
    let searchField: NSSearchField
    let chooseLibrary: (URL?) -> Void
    let select: (SettingsPane) -> Void
    let reveal: (SettingsSearchRow) -> Void

    var body: some View {
        HSplitView {
            sidebar.frame(minWidth: 172, idealWidth: 192, maxWidth: 226)
            detail.frame(minWidth: 410)
        }
    }

    private var sidebar: some View {
        VStack(spacing: 8) {
            SettingsSearchHost(field: searchField)
                .frame(height: 26)
                .padding(.horizontal, 12)
                .padding(.top, 12)

            if preferences.search.isSearching {
                searchResults.onAppear { flashed = nil }
            } else {
                List(selection: Binding<SettingsPane?>(
                    get: { preferences.pane },
                    set: { pane in if let pane { select(pane) } }
                )) {
                    ForEach(SettingsPane.allCases, id: \.self) { pane in
                        Text(pane.title).tag(pane)
                    }
                }
                .listStyle(.sidebar)
                .scrollContentBackground(.hidden)
            }
        }
        .background(Color(nsColor: .controlBackgroundColor))
    }

    private var detail: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(preferences.pane.title)
                .font(.title2.weight(.semibold))
                .accessibilityAddTraits(.isHeader)
                .padding(.horizontal, 24)
                .padding(.vertical, 17)
                .frame(maxWidth: .infinity, alignment: .leading)
            Divider()
            ScrollViewReader { scroll in
                settingsForm
                    .id(preferences.pane)
                    .task(id: revealWhenAvailable) {
                        guard let target = revealWhenAvailable else { return }
                        await Task.yield()
                        guard !Task.isCancelled else { return }
                        scroll.scrollTo(target, anchor: .center)
                        flashed = target
                        focused = target
                        spoken = target
                        preferences.revealed = nil
                    }
                    .task(id: flashed) {
                        guard flashed != nil else { return }
                        try? await Task.sleep(for: .seconds(2))
                        if !Task.isCancelled { flashed = nil }
                    }
            }
        }
        .background(Color(nsColor: .windowBackgroundColor))
    }

    private var revealWhenAvailable: SettingsSearchRow? {
        guard preferences.saved != nil, !preferences.busy, !shortcuts.saving, !preferences.libraryBusy else { return nil }
        return preferences.revealed
    }

    private var searchResults: some View {
        List {
            if preferences.search.matches.isEmpty {
                Text(PickerWords.text(.settingsNoResults, preferences.search.query))
                    .foregroundStyle(.secondary)
            }
            ForEach(preferences.search.matches) { row in
                Button { reveal(row) } label: {
                    VStack(alignment: .leading, spacing: 3) {
                        Text(row.title)
                        if row.title != row.pane.title {
                            Text(row.pane.title).font(.caption).foregroundStyle(.secondary)
                        }
                    }
                    .padding(.vertical, 3)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(Text(row.title))
                .accessibilityValue(Text(row.pane.title))
                .accessibilityAddTraits(row == preferences.search.highlighted ? .isSelected : [])
                .listRowBackground(row == preferences.search.highlighted ? Color.accentColor.opacity(0.14) : nil)
            }
        }
        .listStyle(.sidebar)
        .scrollContentBackground(.hidden)
        .accessibilityLabel(Text(PickerWords.text(.settingsSearchResults)))
    }

    private var settingsForm: some View {
        Form {
            if let saved = preferences.saved {
                Group {
                    switch preferences.pane {
                    case .general:
                        shortcutSection
                        pasteSection(saved)
                    case .appearance:
                        pickerSection(saved)
                        Section {
                            AppearancePreview(appearance: saved.appearance, density: saved.density, largerText: saved.largerText)
                                .padding(.vertical, 6)
                        }
                    case .library:
                        librarySection(saved)
                    }
                }
                .disabled(preferences.busy || shortcuts.saving || preferences.libraryBusy)
            }

            if let problem = preferences.problem {
                if preferences.saved == nil {
                    LabeledContent {
                        Button(PickerWords.text(.tryAgain)) { preferences.read() }
                    } label: {
                        Text(problem).foregroundStyle(.secondary)
                    }
                } else {
                    Text(problem).foregroundStyle(.secondary)
                }
            } else if preferences.busy && preferences.saved == nil {
                ProgressView().controlSize(.small)
            }
        }
        .formStyle(.grouped)
    }

    private func control(_ row: SettingsSearchRow, @ViewBuilder content: () -> some View) -> some View {
        content().focused($focused, equals: row)
            .accessibilityFocused($spoken, equals: row)
            .accessibilityIdentifier("ibis.settings.\(row.rawValue)")
    }

    private func destination(_ row: SettingsSearchRow, @ViewBuilder content: () -> some View) -> some View {
        content().id(row)
            .listRowBackground(row == flashed ? Color.accentColor.opacity(0.14) : nil)
    }

    private func librarySection(_ saved: SettingsResult) -> some View {
        Section {
            destination(.library) {
                LabeledContent {
                    control(.library) {
                        Button(PickerWords.text(saved.library?.missing == true ? .findSource : .choose)) {
                            let missing = saved.library?.missing == true ? saved.library?.file.map(URL.init(fileURLWithPath:)) : nil
                            chooseLibrary(missing)
                        }
                    }
                } label: {
                    Text(saved.library?.name ?? PickerWords.text(.noLibrarySelected))
                }
            }
            if let problem = preferences.libraryProblem {
                Text(problem).foregroundStyle(.secondary)
            } else if let library = saved.library, library.missing {
                Text(PickerWords.text(.libraryMissing, library.name)).foregroundStyle(.secondary)
            } else if saved.library == nil {
                Text(PickerWords.text(.chooseHint)).foregroundStyle(.secondary)
            }
            if preferences.libraryBusy { ProgressView().controlSize(.small) }
        }
    }

    private var shortcutSection: some View {
        Section(PickerWords.text(.shortcuts)) {
            destination(.shortcut) {
                LabeledContent {
                    let label = shortcuts.active?.label ?? PickerWords.text(.shortcutNotAssigned)
                    control(.shortcut) {
                        Button(shortcuts.recording ? PickerWords.text(.recordShortcut) : label) { shortcuts.beginRecording() }
                            .accessibilityLabel(Text(SettingsSearchRow.shortcut.title))
                            .accessibilityValue(Text(shortcuts.recording ? PickerWords.text(.recordShortcut) : label))
                    }
                    if shortcuts.hasCustomShortcut {
                        Button(PickerWords.text(.restoreShortcut)) { shortcuts.useDefault() }
                    }
                } label: {
                    Text(SettingsSearchRow.shortcut.title)
                }
            }
            if let issue = shortcuts.issue {
                Text(PickerWords.text(issue)).foregroundStyle(.secondary)
            }
        }
    }

    private func pasteSection(_ saved: SettingsResult) -> some View {
        Section(PickerWords.text(.pasteSettings)) {
            destination(.paste) {
                control(.paste) {
                    Toggle(SettingsSearchRow.paste.title, isOn: Binding(
                        get: { preferences.saved?.paste ?? saved.paste }, set: { preferences.change(paste: $0) }
                    ))
                }
            }
            if !saved.paste || preferences.pasteGranted {
                Text(PickerWords.text(saved.paste ? .pasteReady : .copyMode))
                    .foregroundStyle(.secondary)
            } else {
                VStack(alignment: .leading, spacing: 8) {
                    Text(PickerWords.text(.pasteNeedsAccess)).foregroundStyle(.secondary)
                    Button(PickerWords.text(.openSystemSettings)) { preferences.openPasteAccess() }
                    Text(PickerWords.text(.pasteTryAgain)).foregroundStyle(.secondary)
                }
            }
        }
    }

    private func pickerSection(_ saved: SettingsResult) -> some View {
        Section {
            destination(.appearance) {
                control(.appearance) {
                    Picker(SettingsSearchRow.appearance.title, selection: Binding(
                        get: { preferences.saved?.appearance ?? saved.appearance }, set: { preferences.change(appearance: $0) }
                    )) {
                        Text(PickerWords.text(.system)).tag(Appearance.system)
                        Text(PickerWords.text(.light)).tag(Appearance.light)
                        Text(PickerWords.text(.dark)).tag(Appearance.dark)
                    }
                }
            }
            destination(.density) {
                control(.density) {
                    Picker(SettingsSearchRow.density.title, selection: Binding(
                        get: { preferences.saved?.density ?? saved.density }, set: { preferences.change(density: $0) }
                    )) {
                        Text(PickerWords.text(.compact)).tag(Density.compact)
                        Text(PickerWords.text(.comfortable)).tag(Density.comfortable)
                    }
                }
            }
            destination(.largerText) {
                control(.largerText) {
                    Toggle(SettingsSearchRow.largerText.title, isOn: Binding(
                        get: { preferences.saved?.largerText ?? saved.largerText }, set: { preferences.change(largerText: $0) }
                    ))
                }
            }
        }
    }
}
