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
        // Every shot on the built-in display, at its 2x scale.
        let screen = NSScreen.screens.first { screen in
            (screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? CGDirectDisplayID)
                .map { CGDisplayIsBuiltin($0) != 0 } ?? false
        } ?? NSScreen.main
        switch self {
        case .settingsGeneral, .settingsAppearance, .settingsLibrary:
            NSApp.setActivationPolicy(.regular)
            NSApp.activate(ignoringOtherApps: true)
            settings.show(self == .settingsGeneral ? .general : self == .settingsAppearance ? .appearance : .library)
            if let window = settings.window, let area = screen?.visibleFrame {
                window.setFrameOrigin(NSPoint(x: area.midX - window.frame.width / 2,
                                              y: area.midY - window.frame.height / 2))
            }
            await pause(900)
            guard let window = settings.window else { return }
            report("\(window.windowNumber)")
            return
        default: break
        }
        let store = panel.store
        // An invented app to paste into, never the one in front.
        let textEdit = NSWorkspace.shared.urlForApplication(withBundleIdentifier: "com.apple.TextEdit")
        store.pinnedDestination = ("TextEdit", textEdit.map { NSWorkspace.shared.icon(forFile: $0.path) })
        panel.pinned = ("com.apple.TextEdit", "TextEdit", screen)
        panel.summon()
        backdrop(behind: panel)
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
        report(area(of: NSApp.windows.filter {
            $0.isVisible && $0.level >= .floating && $0 !== Self.backdropWindow && !$0.className.contains("StatusBar")
        }))
    }

    /// An invented shoreline behind the picker, so a shot shows the glass over
    /// the same picture every time and never what is on the person's screen.
    private func backdrop(behind panel: NSWindow) {
        guard let screen = panel.screen ?? NSScreen.main else { return }
        let window = NSWindow(contentRect: screen.frame, styleMask: [.borderless], backing: .buffered, defer: false)
        window.level = NSWindow.Level(rawValue: NSWindow.Level.floating.rawValue - 1)
        window.ignoresMouseEvents = true
        window.isOpaque = true
        window.collectionBehavior = [.canJoinAllSpaces, .ignoresCycle]
        let view = NSView()
        view.wantsLayer = true
        let gradient = CAGradientLayer()
        gradient.colors = [NSColor(red: 0.18, green: 0.50, blue: 0.69, alpha: 1).cgColor,
                           NSColor(red: 0.50, green: 0.72, blue: 0.79, alpha: 1).cgColor,
                           NSColor(red: 0.91, green: 0.83, blue: 0.68, alpha: 1).cgColor]
        gradient.startPoint = CGPoint(x: 0.1, y: 1)
        gradient.endPoint = CGPoint(x: 0.9, y: 0)
        view.layer = gradient
        window.contentView = view
        window.orderFront(nil)
        Self.backdropWindow = window
    }

    private static var backdropWindow: NSWindow?

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

    /// Waits up to 15 s for the engine to catch up, and says so when it never does.
    private func settle(_ store: PickerStore) async {
        await pause(150)
        for _ in 0..<300 {
            if store.settled && !store.isChangingFormat { return }
            await pause(50)
        }
        FileHandle.standardError.write(Data("settle timeout \(rawValue)\n".utf8))
    }

    private func pause(_ milliseconds: Int) async { try? await Task.sleep(for: .milliseconds(milliseconds)) }
}
