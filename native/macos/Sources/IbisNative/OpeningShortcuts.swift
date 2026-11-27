import AppKit
import Combine

@MainActor
final class OpeningShortcuts: ObservableObject {
    @Published private(set) var active: OpeningShortcut?
    @Published private(set) var hasCustomShortcut = false
    @Published private(set) var recording = false
    @Published private(set) var saving = false
    @Published private(set) var issue: PickerWord?

    private let hotKey: HotKey
    private let engine: EngineClient
    private let updateMenu: (OpeningShortcut?) -> Void
    private var savedBinding: String?
    private var didLoad = false
    private var layoutChangedDuringSave = false

    init(engine: EngineClient, action: @escaping () -> Void, updateMenu: @escaping (OpeningShortcut?) -> Void) {
        self.engine = engine
        self.hotKey = HotKey(action: action)
        self.updateMenu = updateMenu
    }

    func start() {
        registerSavedShortcut()
        Task {
            if let saved = try? await engine.settings(.readOnly) { restore(saved.shortcut) }
        }
    }

    func restore(_ binding: String?) {
        guard !saving, !didLoad || binding != savedBinding else { return }
        didLoad = true
        savedBinding = binding
        hasCustomShortcut = binding != nil
        registerSavedShortcut()
    }

    func layoutChanged() {
        if saving { layoutChangedDuringSave = true }
        else { registerSavedShortcut() }
    }

    private func registerSavedShortcut() {
        let wanted: OpeningShortcut
        do {
            wanted = try savedBinding.map(OpeningShortcut.parse) ?? .repoDefault
            issue = nil
        } catch {
            issue = .shortcutInvalid
            if hotKey.registered == nil, !hotKey.register(.repoDefault) { issue = .missingShortcut }
            publish()
            return
        }
        if !hotKey.register(wanted) {
            issue = .shortcutInUse
            if hotKey.registered == nil, !hotKey.register(.repoDefault) { issue = .missingShortcut }
        }
        publish()
    }

    func beginRecording() {
        guard !saving else { return }
        issue = nil
        recording = true
    }

    func cancelRecording() {
        if recording { recording = false }
    }

    func recordKey(_ event: NSEvent) -> Bool {
        guard recording, event.type == .keyDown else { return false }
        guard !event.isARepeat else { return true }
        let character = event.charactersIgnoringModifiers
        cancelRecording()
        if character == "\t" || character == "\u{19}" { return false }
        if character == "\u{1b}" { return true }
        do {
            let shortcut = try OpeningShortcut.record(event)
            let binding = shortcut.savedValue
            save(shortcut, binding: binding == OpeningShortcut.repoDefault.savedValue ? nil : binding)
        } catch ShortcutRefusal.needsModifier {
            issue = .shortcutNeedsModifier
        } catch ShortcutRefusal.reserved {
            issue = .shortcutReserved
        } catch {
            issue = .shortcutInvalid
        }
        return true
    }

    func useDefault() { save(.repoDefault, binding: nil) }

    private func save(_ shortcut: OpeningShortcut, binding: String?) {
        guard !saving else { return }
        guard hotKey.reserve(shortcut) else {
            issue = .shortcutInUse
            return
        }
        saving = true
        issue = nil
        Task {
            let request = SettingsRequest.patch(shortcut: ShortcutSetting(binding: binding))
            var saved = try? await engine.settings(request)
            // A dropped reply can follow a committed write. Confirm by reading.
            if saved == nil { saved = try? await engine.settings(.readOnly) }
            let confirmed = saved.map { $0.shortcut == binding } ?? false
            if confirmed {
                hotKey.commitReservation()
                didLoad = true
                savedBinding = binding
                hasCustomShortcut = binding != nil
            } else {
                hotKey.discardReservation()
            }
            saving = false
            if layoutChangedDuringSave {
                layoutChangedDuringSave = false
                registerSavedShortcut()
            } else {
                publish()
            }
            if !confirmed { issue = .shortcutNotSaved }
        }
    }

    private func publish() {
        active = hotKey.registered
        updateMenu(active)
    }
}
