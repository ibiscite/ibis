import AppKit
import CIbisNative
import SwiftUI

@MainActor
final class NativeSettingsWindow: NSWindowController, NSWindowDelegate {
    private let preferences: NativePreferences
    private let shortcuts: OpeningShortcuts

    init(engine: EngineClient, shortcuts: OpeningShortcuts, applied: @escaping (SettingsResult, Bool) -> Void) {
        self.shortcuts = shortcuts
        let preferences = NativePreferences(engine: engine) { [weak shortcuts] saved, resetText in
            shortcuts?.restore(saved.shortcut)
            applied(saved, resetText)
        }
        self.preferences = preferences
        let window = SettingsHostWindow(shortcuts: shortcuts)
        window.title = PickerWords.text(.settingsWindow)
        window.isReleasedWhenClosed = false
        window.contentView = NSHostingView(rootView: NativeSettingsForm(preferences: preferences, shortcuts: shortcuts))
        window.center()
        super.init(window: window)
        window.delegate = self
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
}

@MainActor
private final class SettingsHostWindow: NSWindow {
    private let shortcuts: OpeningShortcuts

    init(shortcuts: OpeningShortcuts) {
        self.shortcuts = shortcuts
        super.init(contentRect: NSRect(x: 0, y: 0, width: 480, height: 435),
                   styleMask: [.titled, .closable], backing: .buffered, defer: false)
    }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        shortcuts.recordKey(event) || super.performKeyEquivalent(with: event)
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
        send(SettingsRequest(appearance: appearance, density: density, largerText: largerText, paste: paste, shortcut: nil),
             changing: true)
    }

    private func send(_ request: SettingsRequest, changing: Bool) {
        guard !busy else { return }
        busy = true
        problem = nil
        Task {
            defer { busy = false }
            do {
                let reply = try await engine.settings(request)
                saved = reply
                applied(reply, request.largerText != nil)
            } catch {
                if changing {
                    // A dropped reply can follow a committed write. Recover the saved view.
                    if let reply = try? await engine.settings(.readOnly) {
                        saved = reply
                        applied(reply, request.largerText != nil)
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

private struct NativeSettingsForm: View {
    @ObservedObject var preferences: NativePreferences
    @ObservedObject var shortcuts: OpeningShortcuts

    var body: some View {
        Form {
            if let saved = preferences.saved {
                shortcutSection.disabled(preferences.busy || shortcuts.saving)
                pasteSection(saved).disabled(preferences.busy || shortcuts.saving)
                pickerSection(saved).disabled(preferences.busy || shortcuts.saving)
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

    private var shortcutSection: some View {
        Section(PickerWords.text(.shortcuts)) {
            LabeledContent {
                let label = shortcuts.active?.label ?? PickerWords.text(.shortcutNotAssigned)
                Button(shortcuts.recording ? PickerWords.text(.recordShortcut) : label) { shortcuts.beginRecording() }
                    .accessibilityLabel(Text(PickerWords.text(.open)))
                    .accessibilityValue(Text(shortcuts.recording ? PickerWords.text(.recordShortcut) : label))
                if shortcuts.hasCustomShortcut {
                    Button(PickerWords.text(.restoreShortcut)) { shortcuts.useDefault() }
                }
            } label: {
                Text(PickerWords.text(.open))
            }
            if let issue = shortcuts.issue {
                Text(PickerWords.text(issue)).foregroundStyle(.secondary)
            }
        }
    }

    private func pasteSection(_ saved: SettingsResult) -> some View {
        Section(PickerWords.text(.pasteSettings)) {
            Toggle(PickerWords.text(.automaticPaste), isOn: Binding(
                get: { preferences.saved?.paste ?? saved.paste }, set: { preferences.change(paste: $0) }
            ))
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
        Section(PickerWords.text(.pickerPreferences)) {
            Picker(PickerWords.text(.appearance), selection: Binding(
                get: { preferences.saved?.appearance ?? saved.appearance }, set: { preferences.change(appearance: $0) }
            )) {
                Text(PickerWords.text(.system)).tag(Appearance.system)
                Text(PickerWords.text(.light)).tag(Appearance.light)
                Text(PickerWords.text(.dark)).tag(Appearance.dark)
            }
            Picker(PickerWords.text(.density), selection: Binding(
                get: { preferences.saved?.density ?? saved.density }, set: { preferences.change(density: $0) }
            )) {
                Text(PickerWords.text(.compact)).tag(Density.compact)
                Text(PickerWords.text(.comfortable)).tag(Density.comfortable)
            }
            Toggle(PickerWords.text(.largerText), isOn: Binding(
                get: { preferences.saved?.largerText ?? saved.largerText }, set: { preferences.change(largerText: $0) }
            ))
        }
    }
}
