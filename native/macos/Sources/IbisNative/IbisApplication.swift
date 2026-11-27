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
    private lazy var model = PickerModel(
        engine: EngineClient(binary: Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS/ibis")),
        file: launch.file
    )
    private lazy var panel = PickerPanel(model: model) { [weak self] in self?.chooseLibrary() }
    private let status = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
    private let openItem = NSMenuItem(title: "Open Ibis", action: #selector(toggle), keyEquivalent: "")
    private lazy var hotKey = HotKey { [weak self] in self?.toggle() }
    private var layoutObserver: NSObjectProtocol?

    func applicationDidFinishLaunching(_ notification: Notification) {
        if let path = Bundle.main.path(forResource: "ibis-menu-bar", ofType: "png"),
           let image = NSImage(contentsOfFile: path) {
            image.isTemplate = true
            image.size = NSSize(width: 18, height: 18)
            status.button?.image = image
        } else {
            status.button?.title = "Ibis"
        }
        status.button?.setAccessibilityLabel("Ibis Native")
        let menu = NSMenu()
        openItem.target = self
        menu.addItem(openItem)
        menu.addItem(item("Choose Library…", action: #selector(chooseLibrary)))
        menu.addItem(item("Allow Paste…", action: #selector(allowPaste)))
        menu.addItem(.separator())
        menu.addItem(item("Quit Ibis Native", action: #selector(quit), key: "q"))
        status.menu = menu
        registerShortcut()
        layoutObserver = DistributedNotificationCenter.default().addObserver(
            forName: Notification.Name(kTISNotifySelectedKeyboardInputSourceChanged as String),
            object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.registerShortcut() }
        }
        if launch.show { panel.summon() }
    }

    func applicationWillTerminate(_ notification: Notification) {
        if let layoutObserver { DistributedNotificationCenter.default().removeObserver(layoutObserver) }
    }

    private func item(_ title: String, action: Selector, key: String = "") -> NSMenuItem {
        let item = NSMenuItem(title: title, action: action, keyEquivalent: key)
        item.target = self
        return item
    }

    private func registerShortcut() {
        let registered = hotKey.register()
        openItem.keyEquivalent = registered ? "y" : ""
        openItem.keyEquivalentModifierMask = [.command, .shift, .option]
        openItem.toolTip = registered ? nil : "The Ibis Native shortcut is unavailable."
    }

    @objc private func toggle() {
        if panel.isVisible { panel.dismiss() } else { panel.summon() }
    }

    @objc private func chooseLibrary() {
        let chooser = NSOpenPanel()
        chooser.prompt = "Open Library"
        chooser.canChooseDirectories = false
        chooser.allowsMultipleSelection = false
        chooser.allowedContentTypes = model.catalog?.libraryExtensions.compactMap { UTType(filenameExtension: $0) } ?? []
        chooser.begin { [weak self] response in
            guard let self, response == .OK, let file = chooser.url else { return }
            self.model.file = file
            self.panel.summon()
        }
    }

    @objc private func allowPaste() {
        ibis_native_request_paste_access()
        if let pane = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility") {
            NSWorkspace.shared.open(pane)
        }
    }
    @objc private func quit() { NSApp.terminate(nil) }
}
