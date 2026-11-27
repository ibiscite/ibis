import AppKit
import CIbisNative
import SwiftUI

@MainActor
final class NativeSettingsWindow: NSWindowController, NSWindowDelegate {
    private let preferences: NativePreferences

    init(engine: EngineClient, applied: @escaping (SettingsResult, Bool) -> Void) {
        let preferences = NativePreferences(engine: engine, applied: applied)
        self.preferences = preferences
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 480, height: 360),
                              styleMask: [.titled, .closable], backing: .buffered, defer: false)
        window.title = PickerWords.text(.settingsWindow)
        window.isReleasedWhenClosed = false
        window.contentView = NSHostingView(rootView: NativeSettingsForm(preferences: preferences))
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
}

@MainActor
private final class NativePreferences: ObservableObject {
    @Published private(set) var saved: SettingsResult?
    @Published private(set) var busy = false
    @Published private(set) var problem: String?
    @Published private(set) var pasteGranted = false
    private let engine: EngineClient
    private let applied: (SettingsResult, Bool) -> Void
    private let empty = SettingsRequest(appearance: nil, density: nil, largerText: nil, paste: nil)

    init(engine: EngineClient, applied: @escaping (SettingsResult, Bool) -> Void) {
        self.engine = engine
        self.applied = applied
    }

    func read() { send(empty, changing: false) }

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
        send(SettingsRequest(appearance: appearance, density: density, largerText: largerText, paste: paste), changing: true)
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
                    if let reply = try? await engine.settings(empty) {
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

    var body: some View {
        Form {
            if let saved = preferences.saved {
                pasteSection(saved).disabled(preferences.busy)
                pickerSection(saved).disabled(preferences.busy)
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
