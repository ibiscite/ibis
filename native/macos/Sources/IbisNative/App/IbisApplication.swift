import AppKit
import Carbon
import CIbisNative
import UniformTypeIdentifiers

@main
struct IbisApplication {
    @MainActor
    static func main() {
        let application = NSApplication.shared
        let delegate = PickerApp()
        application.delegate = delegate
        application.setActivationPolicy(.accessory)
        withExtendedLifetime(delegate) { application.run() }
    }
}

@MainActor
private final class PickerApp: NSObject, NSApplicationDelegate {
    private struct Launch {
        let file: URL?
        let show: Bool
        let appearance: NSAppearance.Name?

        init() {
            let arguments = CommandLine.arguments
            file = arguments.firstIndex(of: "--file").flatMap { index in
                guard arguments.indices.contains(index + 1) else { return nil }
                return URL(fileURLWithPath: arguments[index + 1]).standardizedFileURL
            }
            show = arguments.contains("--show")
            appearance = arguments.firstIndex(of: "--appearance").flatMap { index in
                guard arguments.indices.contains(index + 1) else { return nil }
                return arguments[index + 1] == "dark" ? .darkAqua : .aqua
            }
        }
    }

    private let launch = Launch()
    private let engine = EngineClient(binary: Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS/ibis"))
    private lazy var store = PickerStore(engine: engine, file: launch.file)
    private lazy var panel: PickerPanel = {
        let panel = PickerPanel(store: store, chooseLibrary: { [weak self] missing in
            self?.chooseLibrary(missing: missing)
        }, useLibrary: { [weak self] file in
            guard let self, !self.panel.choosingLibrary, !self.store.isDelivering else { return }
            self.useLibrary(file, fromSettings: false)
        })
        panel.openSettings = { [weak self] in self?.showSettings() }
        store.needsPasteAccess = { PermissionHelper.shared.showAtPaste() }
        store.didDeliver = { [weak self] in self?.nod() }
        store.didCopy = { NoticeHUD.shared.show($0) }
        return panel
    }()
    private lazy var settings = NativeSettingsWindow(engine: engine, shortcuts: shortcuts, chooseLibrary: {
        [weak self] window, missing in self?.chooseLibrary(missing: missing, in: window)
    }, libraryInUse: { [weak self] in
        self?.store.catalog?.library?.file.map(URL.init(fileURLWithPath:)) ?? self?.store.file
    }, applied: { [weak self] saved, resetText in
        self?.store.applySettings(saved, resetTextSize: resetText)
    })
    private var previousApp: NSRunningApplication?
    private let status = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
    private let openItem = NSMenuItem(title: PickerWords.text(.open), action: #selector(toggle), keyEquivalent: "")
    private lazy var shortcuts: OpeningShortcuts = OpeningShortcuts(engine: engine, action: { [weak self] in
        guard let self else { return }
        if shortcuts.recording { shortcuts.cancelRecording() }
        else { toggle() }
    }, updateMenu: { [weak self] shortcut in
        guard let self else { return }
        let title = PickerWords.text(.open)
        let key = shortcut?.menuKey ?? ""
        if let shortcut, key.isEmpty { openItem.title = "\(title) (\(shortcut.label))" }
        else { openItem.title = title }
        openItem.keyEquivalent = key
        openItem.keyEquivalentModifierMask = shortcut?.modifiers ?? []
        openItem.toolTip = shortcut == nil ? PickerWords.text(.missingShortcut) : nil
    })
    private var layoutObserver: NSObjectProtocol?
    private var windowObserver: NSObjectProtocol?

    func applicationDidFinishLaunching(_ notification: Notification) {
        if let path = Bundle.main.path(forResource: "ibis-menu-bar", ofType: "png"),
           let image = NSImage(contentsOfFile: path) {
            image.isTemplate = true
            image.size = NSSize(width: 18, height: 18)
            status.button?.image = image
        } else {
            status.button?.title = PickerWords.text(.app)
        }
        status.button?.setAccessibilityLabel(PickerWords.text(.app))
        let menu = NSMenu()
        openItem.target = self
        menu.addItem(openItem)
        menu.addItem(item(PickerWords.text(.choose), action: #selector(chooseLibraryItem)))
        menu.addItem(item(PickerWords.text(.allowPaste), action: #selector(showPasteSettings)))
        menu.addItem(.separator())
        menu.addItem(item(PickerWords.text(.settings), action: #selector(showSettings), key: ","))
        menu.addItem(item(PickerWords.text(.about), action: #selector(showAbout)))
        menu.addItem(.separator())
        menu.addItem(item(PickerWords.text(.quit), action: #selector(quit), key: "q"))
        status.menu = menu
        installAppMenu()
        windowObserver = NotificationCenter.default.addObserver(forName: NSWindow.willCloseNotification,
                                                                 object: nil, queue: .main) { [weak self] _ in
            Task { @MainActor in self?.restoreAccessoryWhenPossible() }
        }
        shortcuts.start()
        layoutObserver = DistributedNotificationCenter.default().addObserver(
            forName: Notification.Name(kTISNotifySelectedKeyboardInputSourceChanged as String),
            object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.shortcuts.layoutChanged() }
        }
        if let appearance = launch.appearance { NSApp.appearance = NSAppearance(named: appearance) }
        if let state = ReviewState.requested {
            Task { await state.run(panel: panel, settings: settings) }
        } else if launch.show {
            panel.summon()
        }
    }

    func applicationWillTerminate(_ notification: Notification) {
        if let layoutObserver { DistributedNotificationCenter.default().removeObserver(layoutObserver) }
        if let windowObserver { NotificationCenter.default.removeObserver(windowObserver) }
    }

    private func installAppMenu() {
        let menu = NSMenu()
        menu.addItem(item(PickerWords.text(.about), action: #selector(showAbout)))
        menu.addItem(.separator())
        menu.addItem(item(PickerWords.text(.settings), action: #selector(showSettings), key: ","))
        menu.addItem(.separator())
        menu.addItem(item(PickerWords.text(.quit), action: #selector(quit), key: "q"))
        let root = NSMenu()
        let application = NSMenuItem()
        application.submenu = menu
        root.addItem(application)
        let windows = NSMenu(title: PickerWords.text(.windowMenu))
        windows.addItem(NSMenuItem(title: PickerWords.text(.closeWindow), action: #selector(NSWindow.performClose(_:)), keyEquivalent: "w"))
        let windowItem = NSMenuItem()
        windowItem.submenu = windows
        root.addItem(windowItem)
        NSApp.mainMenu = root
    }

    private func activateAppWindow() {
        panel.dismiss()
        activateTitledWindow()
    }

    private func activateTitledWindow() {
        if NSApp.activationPolicy() != .regular {
            previousApp = NSWorkspace.shared.frontmostApplication
            NSApp.setActivationPolicy(.regular)
        }
        NSApp.activate(ignoringOtherApps: true)
    }

    private func restoreAccessoryWhenPossible() {
        guard NSApp.activationPolicy() == .regular,
              !NSApp.windows.contains(where: { $0.isVisible && $0.styleMask.contains(.titled) }) else { return }
        let front = NSApp.isActive
        NSApp.setActivationPolicy(.accessory)
        if front { previousApp?.activate(options: []) }
        previousApp = nil
    }

    @objc private func showSettings() {
        activateAppWindow()
        settings.showWindow(nil)
    }

    @objc private func showPasteSettings() {
        settings.showPaste()
        showSettings()
    }

    @objc private func showAbout() {
        Task {
            let saved = try? await engine.settings(.readOnly)
            var options: [NSApplication.AboutPanelOptionKey: Any] = [.applicationName: PickerWords.text(.app)]
            if let saved { options[.applicationVersion] = saved.version }
            activateAppWindow()
            NSApp.orderFrontStandardAboutPanel(options: options)
        }
    }

    private func item(_ title: String, action: Selector, key: String = "") -> NSMenuItem {
        let item = NSMenuItem(title: title, action: action, keyEquivalent: key)
        item.target = self
        return item
    }

    @objc private func toggle() {
        if panel.pickerIsOpen { panel.dismiss() } else { panel.summon() }
    }

    @objc private func chooseLibraryItem() { chooseLibrary(missing: nil) }

    private func chooseLibrary(missing: URL?, in window: NSWindow? = nil) {
        guard !panel.choosingLibrary, !store.isDelivering else { return }
        let chooser = NSOpenPanel()
        chooser.prompt = PickerWords.text(.openLibrary)
        chooser.canChooseDirectories = false
        chooser.allowsMultipleSelection = false
        chooser.directoryURL = missing?.deletingLastPathComponent()
        let extensions = window == nil ? store.catalog?.libraryExtensions ?? [] : settings.libraryExtensions
        chooser.allowedContentTypes = extensions.compactMap { UTType(filenameExtension: $0) }
        panel.choosingLibrary = true
        if window == nil { activateTitledWindow() }
        let fromSettings = window != nil
        let finished: (NSApplication.ModalResponse) -> Void = { [weak self] response in
            guard let self else { return }
            guard response == .OK, let file = chooser.url?.standardizedFileURL else {
                self.panel.choosingLibrary = false
                return
            }
            self.useLibrary(file, fromSettings: fromSettings)
        }
        if let window { chooser.beginSheetModal(for: window, completionHandler: finished) }
        else { chooser.begin(completionHandler: finished) }
    }

    /// A library file chosen in the chooser or dropped on the picker becomes the saved library.
    private func useLibrary(_ file: URL, fromSettings: Bool) {
        panel.choosingLibrary = true
        if fromSettings { settings.setLibraryBusy(true) }
        Task {
            defer {
                panel.choosingLibrary = false
                if fromSettings { settings.setLibraryBusy(false) }
            }
            do {
                let saved = try await engine.chooseLibrary(file)
                store.useSavedLibrary()
                if fromSettings {
                    settings.didSelectLibrary(saved)
                } else if panel.pickerIsOpen {
                    panel.focusSearch()
                } else {
                    panel.summon()
                }
            } catch {
                if fromSettings {
                    settings.didRejectLibrary(error)
                } else {
                    if !panel.pickerIsOpen { panel.summon() }
                    store.reportLibraryChoice(error)
                }
            }
        }
    }

    @objc private func quit() { NSApp.terminate(nil) }

    /// After a delivery the bird in the menu bar nods once; never with Reduce Motion.
    private func nod() {
        guard !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion, let button = status.button else { return }
        button.wantsLayer = true
        guard let layer = button.layer else { return }
        let nod = CAKeyframeAnimation(keyPath: "transform.rotation.z")
        nod.values = [0, -0.18, 0.08, 0]
        nod.keyTimes = [0, 0.35, 0.7, 1]
        nod.duration = 0.45
        nod.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
        layer.anchorPoint = CGPoint(x: 0.5, y: 0.5)
        layer.position = CGPoint(x: button.bounds.midX, y: button.bounds.midY)
        layer.add(nod, forKey: "nod")
    }
}
