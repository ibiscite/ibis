import AppKit

/// Puts the app into one named state for a screenshot, then prints
/// `ready WINDOW` so `scripts/native-macos.sh shots` can capture that window.
@MainActor
enum ReviewState: String, CaseIterable {
    case results, search, draft, details, actions, peek
    case noMatches = "no-matches"
    case larger, choose
    case settingsGeneral = "settings-general"
    case settingsAppearance = "settings-appearance"
    case settingsLibrary = "settings-library"

    static var requested: ReviewState? {
        let arguments = CommandLine.arguments
        return arguments.firstIndex(of: "--state").flatMap { index in
            arguments.indices.contains(index + 1) ? ReviewState(rawValue: arguments[index + 1]) : nil
        }
    }

    func run(panel: PickerPanel, settings: NativeSettingsWindow) async {
        let window: NSWindow
        switch self {
        case .settingsGeneral, .settingsAppearance, .settingsLibrary:
            NSApp.setActivationPolicy(.regular)
            NSApp.activate(ignoringOtherApps: true)
            settings.show(self == .settingsGeneral ? .general : self == .settingsAppearance ? .appearance : .library)
            await pause(900)
            guard let shown = settings.window else { return }
            window = shown
        default:
            let model = panel.model
            let controller = panel.controller
            panel.summon()
            await settle(model)
            switch self {
            case .search: model.query = "shore"
            case .noMatches: model.query = "zzqxv"
            case .larger: model.changeSize(50)
            case .draft, .details:
                model.query = "shore"
                await settle(model)
                for row in model.rows.prefix(2) { model.toggleCitation(row) }
                await settle(model)
                if let item = model.citationItem { model.changeDetails(for: item) { $0.locator = "417" } }
                await settle(model)
                if self == .details { controller.showDetails() }
            case .actions: controller.showActions()
            case .peek: controller.togglePeek()
            default: break
            }
            await settle(model)
            window = panel
        }
        await pause(500)
        print("ready \(window.windowNumber)")
        fflush(stdout)
    }

    private func settle(_ model: PickerModel) async {
        await pause(150)
        for _ in 0..<100 where model.isOpening || model.isFormatting || (model.libraryReady && !model.canDeliver
            && !model.rows.isEmpty) {
            await pause(50)
        }
    }

    private func pause(_ milliseconds: Int) async { try? await Task.sleep(for: .milliseconds(milliseconds)) }
}
