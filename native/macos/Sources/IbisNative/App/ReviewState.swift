import AppKit

/// Puts the app into one named state for a screenshot, then prints where to
/// capture it, so `scripts/native-macos.sh shots` can: `ready WINDOW` for one
/// window, or `ready rect X,Y,W,H` for the picker with its Peek or popover.
@MainActor
enum ReviewState: String, CaseIterable {
    case results, search, draft, details, peek
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
        switch self {
        case .settingsGeneral, .settingsAppearance, .settingsLibrary:
            NSApp.setActivationPolicy(.regular)
            NSApp.activate(ignoringOtherApps: true)
            settings.show(self == .settingsGeneral ? .general : self == .settingsAppearance ? .appearance : .library)
            await pause(900)
            guard let window = settings.window else { return }
            report("\(window.windowNumber)")
            return
        default: break
        }
        let store = panel.store
        panel.summon()
        await settle(store)
        // The same format in every shot, whichever app is in front.
        if self != .choose { store.useFormatOnce("apa") }
        switch self {
        case .results: store.type("invented")
        case .search: store.type("shore")
        case .noMatches: store.type("zzqxv")
        case .larger: store.changeSize(50); store.type("shore")
        case .draft, .details:
            store.type("shore")
            await settle(store)
            store.add()
            await settle(store)
            if let first = store.added.first {
                store.changeDetails(first.row.id, to: first.details.with(locator: Locator(term: .page, value: "417")))
            }
            await settle(store)
            store.type(self == .draft ? "tide" : "")
            await settle(store)
            if self == .details, let first = store.added.first { store.edit(first.row.id) }
        case .peek:
            store.type("shore")
            await settle(store)
            store.togglePeek()
        default: break
        }
        await settle(store)
        await pause(500)
        report(area(of: NSApp.windows.filter { $0.isVisible && $0.level >= .floating && !$0.className.contains("StatusBar") }))
    }

    /// The windows' union in screen points from the top left, as `screencapture -R` takes it.
    private func area(of windows: [NSWindow]) -> String {
        let union = windows.map(\.frame).reduce(CGRect.null) { $0.union($1) }.insetBy(dx: -24, dy: -24)
        let top = NSScreen.screens.first?.frame.maxY ?? union.maxY
        return "rect \(Int(union.minX)),\(Int(top - union.maxY)),\(Int(union.width)),\(Int(union.height))"
    }

    private func report(_ place: String) {
        print("ready \(place)")
        fflush(stdout)
    }

    private func settle(_ store: PickerStore) async {
        await pause(150)
        for _ in 0..<100 where !store.settled || store.isChangingFormat {
            await pause(50)
        }
    }

    private func pause(_ milliseconds: Int) async { try? await Task.sleep(for: .milliseconds(milliseconds)) }
}
