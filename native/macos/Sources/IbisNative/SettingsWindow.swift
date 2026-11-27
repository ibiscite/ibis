import AppKit
import SwiftUI

@MainActor
final class NativeSettingsWindow: NSWindowController {
    private let preferences: NativePreferences

    init(engine: EngineClient, applied: @escaping (SettingsResult, Bool) -> Void) {
        let preferences = NativePreferences(engine: engine, applied: applied)
        self.preferences = preferences
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 430, height: 188),
                              styleMask: [.titled, .closable], backing: .buffered, defer: false)
        window.title = PickerWords.text(.settingsWindow)
        window.isReleasedWhenClosed = false
        window.contentView = NSHostingView(rootView: NativeSettingsForm(preferences: preferences))
        window.center()
        super.init(window: window)
    }

    required init?(coder: NSCoder) { nil }

    override func showWindow(_ sender: Any?) {
        super.showWindow(sender)
        window?.makeKeyAndOrderFront(sender)
        preferences.read()
    }
}

@MainActor
private final class NativePreferences: ObservableObject {
    @Published private(set) var saved: SettingsResult?
    @Published private(set) var busy = false
    @Published private(set) var problem: String?
    private let engine: EngineClient
    private let applied: (SettingsResult, Bool) -> Void
    private let empty = SettingsRequest(appearance: nil, density: nil, largerText: nil)

    init(engine: EngineClient, applied: @escaping (SettingsResult, Bool) -> Void) {
        self.engine = engine
        self.applied = applied
    }

    func read() { send(empty, changing: false) }

    func change(appearance: Appearance? = nil, density: Density? = nil, largerText: Bool? = nil) {
        guard saved != nil else { return }
        send(SettingsRequest(appearance: appearance, density: density, largerText: largerText), changing: true)
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
            Section(PickerWords.text(.pickerPreferences)) {
                Picker(PickerWords.text(.appearance), selection: Binding(
                    get: { preferences.saved?.appearance ?? .system }, set: { preferences.change(appearance: $0) }
                )) {
                    Text(PickerWords.text(.system)).tag(Appearance.system)
                    Text(PickerWords.text(.light)).tag(Appearance.light)
                    Text(PickerWords.text(.dark)).tag(Appearance.dark)
                }
                Picker(PickerWords.text(.density), selection: Binding(
                    get: { preferences.saved?.density ?? .compact }, set: { preferences.change(density: $0) }
                )) {
                    Text(PickerWords.text(.compact)).tag(Density.compact)
                    Text(PickerWords.text(.comfortable)).tag(Density.comfortable)
                }
                Toggle(PickerWords.text(.largerText), isOn: Binding(
                    get: { preferences.saved?.largerText ?? false }, set: { preferences.change(largerText: $0) }
                ))
            }
            .disabled(preferences.busy || preferences.saved == nil)

            if let problem = preferences.problem {
                Text(problem).foregroundStyle(.secondary)
            } else if preferences.busy && preferences.saved == nil {
                ProgressView().controlSize(.small)
            }
        }
        .formStyle(.grouped)
    }
}
