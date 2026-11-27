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

        init() {
            let arguments = CommandLine.arguments
            file = arguments.firstIndex(of: "--file").flatMap { index in
                guard arguments.indices.contains(index + 1) else { return nil }
                return URL(fileURLWithPath: arguments[index + 1]).standardizedFileURL
            }
            show = arguments.contains("--show")
        }
    }

    private let launch = Launch()
    private let engine = EngineClient(binary: Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS/ibis"))
    private lazy var model = PickerModel(engine: engine, file: launch.file)
    private lazy var panel: PickerPanel = {
        let panel = PickerPanel(model: model, sizes: layoutDefaults) { [weak self] missing in
            self?.chooseLibrary(missing: missing)
        }
        panel.openSettings = { [weak self] in self?.showSettings() }
        return panel
    }()
    private lazy var settings = NativeSettingsWindow(engine: engine, shortcuts: shortcuts, chooseLibrary: {
        [weak self] window, missing in self?.chooseLibrary(missing: missing, in: window)
    }, applied: { [weak self] saved, resetText in
        self?.model.applySettings(saved, resetTextSize: resetText)
    })
    private var previousApp: NSRunningApplication?
    private var layoutDefaults: UserDefaults? {
        ProcessInfo.processInfo.environment["IBIS_CONFIG_DIR"] == nil ? .standard : nil
    }
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
        if launch.show { panel.summon() }
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
        settings.showGeneral()
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
        guard !panel.choosingLibrary, !model.isDelivering else { return }
        let chooser = NSOpenPanel()
        chooser.prompt = PickerWords.text(.openLibrary)
        chooser.canChooseDirectories = false
        chooser.allowsMultipleSelection = false
        chooser.directoryURL = missing?.deletingLastPathComponent()
        let extensions = window == nil ? model.catalog?.libraryExtensions ?? [] : settings.libraryExtensions
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
            if fromSettings { self.settings.setLibraryBusy(true) }
            Task {
                defer {
                    self.panel.choosingLibrary = false
                    if fromSettings { self.settings.setLibraryBusy(false) }
                }
                do {
                    let saved = try await self.engine.chooseLibrary(file)
                    self.model.useSavedLibrary()
                    if fromSettings {
                        self.settings.didSelectLibrary(saved)
                    } else if self.panel.pickerIsOpen {
                        self.panel.makeKeyAndOrderFront(nil)
                        self.panel.makeFirstResponder(self.panel.searchField)
                    } else {
                        self.panel.summon()
                    }
                } catch {
                    if fromSettings {
                        self.settings.didRejectLibrary(error)
                    } else {
                        if !self.panel.pickerIsOpen { self.panel.summon() }
                        self.model.reportLibraryChoice(error)
                    }
                }
            }
        }
        if let window { chooser.beginSheetModal(for: window, completionHandler: finished) }
        else { chooser.begin(completionHandler: finished) }
    }

    @objc private func quit() { NSApp.terminate(nil) }
}
