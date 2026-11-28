import AppKit
import ApplicationServices
import CIbisNative
import Combine
import SwiftUI

/// A nonactivating panel takes text without replacing the captured application.
/// It owns the window, its glass and the keyboard; SwiftUI draws the content.
/// Like Spotlight it is not resizable: its width follows the text size and its
/// height follows the content.
@MainActor
final class PickerPanel: NSPanel, NSWindowDelegate {
    let store: PickerStore
    private let host: NSHostingView<PickerView>
    private let peek: PeekPanel
    private let glass: SystemGlass
    private let chooser: (URL?) -> Void
    private var display: NSScreen?
    private var shown = false
    private var commandHold: Task<Void, Never>?
    private var observers: [NSObjectProtocol] = []
    private var preferences: Set<AnyCancellable> = []
    var openSettings: (() -> Void)?

    var pickerIsOpen: Bool { shown }
    var searchField: NSTextField? { store.searchField }
    var choosingLibrary: Bool {
        get { store.choosingLibrary }
        set { store.choosingLibrary = newValue }
    }

    init(store: PickerStore, chooseLibrary: @escaping (URL?) -> Void) {
        self.store = store
        chooser = chooseLibrary
        host = NSHostingView(rootView: PickerView(store: store))
        peek = PeekPanel(store: store)
        host.safeAreaRegions = []
        host.sizingOptions = []
        glass = SystemGlass(content: host, corner: 26)
        super.init(contentRect: NSRect(x: 0, y: 0, width: 640, height: 56),
                   styleMask: [.borderless, .nonactivatingPanel],
                   backing: .buffered, defer: false)
        store.panel = self
        title = PickerWords.text(.panel)
        setAccessibilityLabel(title)
        isFloatingPanel = true
        level = .floating
        isOpaque = false
        backgroundColor = .clear
        hasShadow = true
        hidesOnDeactivate = false
        becomesKeyOnlyIfNeeded = false
        isMovableByWindowBackground = false
        acceptsMouseMovedEvents = true
        isExcludedFromWindowsMenu = true
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .ignoresCycle]
        contentView = glass
        delegate = self
        observers.append(NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.accessibilityDisplayOptionsDidChangeNotification, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.glass.refresh() }
        })
        store.$appearance.removeDuplicates().sink { [weak self] choice in
            self?.appearance = switch choice {
            case .system: nil
            case .dark: NSAppearance(named: .darkAqua)
            case .light: NSAppearance(named: .aqua)
            }
        }.store(in: &preferences)
        store.$textPercent.removeDuplicates().dropFirst().sink { [weak self] _ in
            DispatchQueue.main.async {
                guard let self, let display = self.screen ?? self.display else { return }
                self.place(on: display)
            }
        }.store(in: &preferences)
        store.objectWillChange.sink { [weak self] in
            DispatchQueue.main.async { self?.followStore() }
        }.store(in: &preferences)
    }

    /// The window follows the content's height, and Peek follows its reference.
    private func followStore() {
        guard shown else { return }
        fitHeight()
        peek.appearance = appearance
        peek.show(store.peek.flatMap { item in store.rows.first { $0.id == item } }, beside: frame)
    }

    deinit {
        observers.forEach(NSWorkspace.shared.notificationCenter.removeObserver)
    }

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    func summon() {
        let target = NSWorkspace.shared.frontmostApplication
        display = focusedDisplay(for: target)
            ?? NSScreen.screens.first { $0.frame.contains(NSEvent.mouseLocation) }
            ?? NSScreen.main
        if let display { place(on: display) }
        let app = target.flatMap { app in
            app.processIdentifier == ProcessInfo.processInfo.processIdentifier || app.bundleIdentifier == "com.apple.loginwindow"
                ? nil : app
        }
        store.present(app: app?.bundleIdentifier, name: app?.localizedName, pid: app?.processIdentifier) {
            let site = ibis_native_capture(windowNumber, app?.processIdentifier ?? 0)
            var bytes = [UInt8](repeating: 0, count: 1024)
            let length = ibis_native_take_selection(&bytes, bytes.count)
            let selection = length > 0 ? String(decoding: bytes.prefix(length), as: UTF8.self) : nil
            return (site.data.map { String(decoding: UnsafeBufferPointer(start: $0, count: site.len), as: UTF8.self) },
                    selection)
        }
        glass.refresh()
        host.layoutSubtreeIfNeeded()
        // Draw once before showing, so the glass never appears empty.
        host.displayIfNeeded()
        let reduceMotion = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        shown = true
        alphaValue = reduceMotion ? 1 : 0
        makeKeyAndOrderFront(nil)
        store.focusSearch()
        if !reduceMotion {
            NSAnimationContext.runAnimationGroup { context in
                context.duration = 0.12
                animator().alphaValue = 1
            }
        }
    }

    func dismiss(forDelivery: Bool = false) {
        guard shown else { return }
        shown = false
        commandHold?.cancel()
        commandHold = nil
        store.dismiss(forDelivery: forDelivery)
        peek.show(nil, beside: frame)
        if forDelivery || NSWorkspace.shared.accessibilityDisplayShouldReduceMotion {
            orderOut(nil)
        } else {
            NSAnimationContext.runAnimationGroup { context in
                context.duration = 0.08
                animator().alphaValue = 0
            } completionHandler: { [weak self] in
                MainActor.assumeIsolated {
                    guard let self, !self.shown else { return }
                    self.orderOut(nil)
                }
            }
        }
    }

    func focusSearch() {
        makeKeyAndOrderFront(nil)
        store.focusSearch()
    }

    func windowDidBecomeKey(_ notification: Notification) { store.panelKeyChanged(true) }

    func windowDidResignKey(_ notification: Notification) {
        store.panelKeyChanged(false)
        // The details popover takes the keyboard; the picker stays open beneath it.
        if shown, !choosingLibrary, store.editing == nil { dismiss() }
    }

    /// Opens a native menu at the control that owns it. Footer menus open upward.
    func popUp(_ menu: NSMenu, at anchor: CGRect?) {
        let rect = anchor ?? CGRect(x: 16, y: host.bounds.height - 40, width: 0, height: 0)
        let top = host.isFlipped ? rect.minY : host.bounds.height - rect.minY
        menu.autoenablesItems = false
        menu.popUp(positioning: menu.items.last, at: NSPoint(x: rect.minX, y: top - 4), in: host)
    }

    /// Spotlight's place: centred, a quarter down the screen, growing downward.
    private func place(on screen: NSScreen) {
        let area = screen.visibleFrame
        let grid = PickerGrid(s: store.scale)
        let width = min(grid.width, area.width * 0.9)
        let height = min(store.height(grid), area.height * 0.8)
        let top = min(area.maxY - 8, area.maxY - area.height / 4 + grid.header)
        setFrame(NSRect(x: area.midX - width / 2, y: top - height, width: width, height: height), display: true)
    }

    /// Keeps the top edge still while the content grows or shrinks.
    func fitHeight() {
        guard shown, let area = (screen ?? display)?.visibleFrame else { return }
        let height = min(store.height(PickerGrid(s: store.scale)), area.height * 0.8)
        guard abs(height - frame.height) > 0.5 else { return }
        setFrame(NSRect(x: frame.minX, y: frame.maxY - height, width: frame.width, height: height), display: true)
    }

    private func focusedDisplay(for app: NSRunningApplication?) -> NSScreen? {
        guard let app, let primary = NSScreen.screens.first else { return nil }
        let topWindow = focusedWindow(of: app) ?? visibleWindow(of: app)
        guard let topWindow else { return nil }
        let rect = NSRect(x: topWindow.minX, y: primary.frame.maxY - topWindow.maxY,
                          width: topWindow.width, height: topWindow.height)
        return NSScreen.screens.filter { $0.frame.intersects(rect) }.max {
            let a = $0.frame.intersection(rect)
            let b = $1.frame.intersection(rect)
            return a.width * a.height < b.width * b.height
        }
    }

    private func focusedWindow(of app: NSRunningApplication) -> CGRect? {
        guard AXIsProcessTrusted() else { return nil }
        let element = AXUIElementCreateApplication(app.processIdentifier)
        AXUIElementSetMessagingTimeout(element, 0.06)
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, kAXFocusedWindowAttribute as CFString, &value) == .success,
              let value, CFGetTypeID(value) == AXUIElementGetTypeID() else { return nil }
        let window = value as! AXUIElement
        var position: CFTypeRef?
        var size: CFTypeRef?
        var point = CGPoint.zero
        var extent = CGSize.zero
        guard AXUIElementCopyAttributeValue(window, kAXPositionAttribute as CFString, &position) == .success,
              AXUIElementCopyAttributeValue(window, kAXSizeAttribute as CFString, &size) == .success,
              let position, let size, CFGetTypeID(position) == AXValueGetTypeID(), CFGetTypeID(size) == AXValueGetTypeID(),
              AXValueGetValue(position as! AXValue, .cgPoint, &point),
              AXValueGetValue(size as! AXValue, .cgSize, &extent) else { return nil }
        return CGRect(origin: point, size: extent)
    }

    private func visibleWindow(of app: NSRunningApplication) -> CGRect? {
        let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
            as? [[String: Any]]
        guard let window = windows?.first(where: {
            $0[kCGWindowOwnerPID as String] as? pid_t == app.processIdentifier
                && $0[kCGWindowLayer as String] as? Int == 0
        }), let bounds = window[kCGWindowBounds as String] as? [String: CGFloat],
            let x = bounds["X"], let y = bounds["Y"], let width = bounds["Width"], let height = bounds["Height"] else {
            return nil
        }
        return CGRect(x: x, y: y, width: width, height: height)
    }

    func chooseLibrary(locate: Bool = false) {
        let missing = locate ? store.catalog?.library?.file.map { URL(fileURLWithPath: $0) } : nil
        chooser(missing)
    }

    func openSource() {
        if let file = store.catalog?.library?.file {
            NSWorkspace.shared.open(URL(fileURLWithPath: file))
        } else if let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: "org.zotero.zotero") {
            NSWorkspace.shared.openApplication(at: url, configuration: NSWorkspace.OpenConfiguration())
        }
    }

    /// Return delivers what the footer shows; with no library yet, it finds one.
    func submit() {
        if store.resolveLibraryIfNeeded() { return }
        store.deliver()
    }

    override func sendEvent(_ event: NSEvent) {
        if event.type == .flagsChanged { flags(event.modifierFlags) }
        if event.type == .keyDown, routeKey(event) { return }
        super.sendEvent(event)
    }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        // Route Command keys before AppKit asks the focused field to handle them.
        if isKeyWindow, routeKey(event) { return true }
        return super.performKeyEquivalent(with: event)
    }

    private func flags(_ flags: NSEvent.ModifierFlags) {
        if flags.contains(.command) {
            guard commandHold == nil else { return }
            commandHold = Task { [weak store] in
                try? await Task.sleep(for: .seconds(1))
                if !Task.isCancelled { store?.showNumbers(true) }
            }
        } else {
            commandHold?.cancel()
            commandHold = nil
            store.showNumbers(false)
        }
    }

    /// One router for the picker's keys, so every command works from every control.
    func routeKey(_ event: NSEvent) -> Bool {
        let responder = (event.window ?? self).firstResponder
        guard (responder as? NSTextInputClient)?.hasMarkedText() != true else { return false }
        let modifiers = event.modifierFlags.intersection([.command, .option, .control, .shift])
        let key = event.specialKey
        let character = event.charactersIgnoringModifiers?.lowercased()
        let isReturn = key == .carriageReturn || key == .enter
        if character == "\t" || character == "\u{19}", modifiers.isEmpty || modifiers == [.shift] {
            if modifiers.isEmpty, !event.isARepeat { store.add() }
            return true
        }
        if isReturn {
            if modifiers.isEmpty {
                if !event.isARepeat { submit() }
                return true
            }
            if modifiers == [.option] {
                // Copy instead of pasting: the citation waits on the clipboard.
                if !event.isARepeat { store.deliver(copyOnly: true) }
                return true
            }
        }
        if modifiers.isEmpty, character == "\u{1b}" {
            if !event.isARepeat, !store.escape() { dismiss() }
            return true
        }
        if store.searchField?.hasFocus == true, modifiers.isEmpty {
            if key == .upArrow || key == .downArrow {
                store.move(key == .upArrow ? -1 : 1)
                return true
            }
            if key == .delete, store.deleteInEmptyField() { return true }
        }
        if modifiers == [.command], character == "k" {
            if !event.isARepeat { store.openActionsMenu() }
            return true
        }
        if modifiers == [.command], let character, let number = Int(character), (1...9).contains(number) {
            if !event.isARepeat, store.selectNumber(number) { submit() }
            return true
        }
        guard let command = PickerCommand.matching(character, modifiers: modifiers) else { return false }
        return store.run(command, repeating: event.isARepeat)
    }
}
