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
    let model: PickerModel
    let controller: PickerController
    private let host: NSHostingView<PickerView>
    private let glass: PickerGlass
    private let chooser: (URL?) -> Void
    private var display: NSScreen?
    private var shown = false
    private var commandHold: Task<Void, Never>?
    private var observers: [NSObjectProtocol] = []
    private var preferences: Set<AnyCancellable> = []
    var openSettings: (() -> Void)?

    var pickerIsOpen: Bool { shown }
    var searchField: NSTextField? { controller.searchField }
    var choosingLibrary: Bool {
        get { controller.choosingLibrary }
        set { controller.choosingLibrary = newValue }
    }

    init(model: PickerModel, chooseLibrary: @escaping (URL?) -> Void) {
        self.model = model
        chooser = chooseLibrary
        controller = PickerController(model: model)
        host = NSHostingView(rootView: PickerView(model: model, controller: controller))
        glass = PickerGlass(content: host)
        super.init(contentRect: NSRect(x: 0, y: 0, width: 640, height: 56),
                   styleMask: [.borderless, .nonactivatingPanel],
                   backing: .buffered, defer: false)
        controller.panel = self
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
        model.$appearance.removeDuplicates().sink { [weak self] choice in
            self?.appearance = switch choice {
            case .system: nil
            case .dark: NSAppearance(named: .darkAqua)
            case .light: NSAppearance(named: .aqua)
            }
        }.store(in: &preferences)
        model.$textPercent.removeDuplicates().dropFirst().sink { [weak self] _ in
            DispatchQueue.main.async {
                guard let self, let display = self.screen ?? self.display else { return }
                self.place(on: display)
            }
        }.store(in: &preferences)
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
        model.present(app: app?.bundleIdentifier, name: app?.localizedName, pid: app?.processIdentifier) {
            let site = ibis_native_capture(windowNumber, app?.processIdentifier ?? 0)
            guard let data = site.data else { return nil }
            return String(decoding: UnsafeBufferPointer(start: data, count: site.len), as: UTF8.self)
        }
        glass.refresh()
        host.layoutSubtreeIfNeeded()
        let reduceMotion = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        shown = true
        alphaValue = reduceMotion ? 1 : 0
        makeKeyAndOrderFront(nil)
        controller.focusSearch(caretAtEnd: true)
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
        controller.reset()
        commandHold?.cancel()
        commandHold = nil
        model.dismiss(forDelivery: forDelivery)
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
        controller.focusSearch(caretAtEnd: true)
    }

    func windowDidBecomeKey(_ notification: Notification) { controller.panelKeyChanged(true) }

    func windowDidResignKey(_ notification: Notification) {
        controller.panelKeyChanged(false)
        // Peek's popover takes the keyboard; the picker stays open beneath it.
        if shown, !choosingLibrary, controller.peek == nil { dismiss() }
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
        let grid = PickerGrid(s: model.scale)
        let width = min(grid.width, area.width * 0.9)
        let height = min(controller.height(grid), area.height * 0.8)
        let top = min(area.maxY - 8, area.maxY - area.height / 4 + grid.header)
        setFrame(NSRect(x: area.midX - width / 2, y: top - height, width: width, height: height), display: true)
    }

    /// Keeps the top edge still while the content grows or shrinks.
    func fitHeight() {
        guard shown, let area = (screen ?? display)?.visibleFrame else { return }
        let height = min(controller.height(PickerGrid(s: model.scale)), area.height * 0.8)
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
        let missing = locate ? model.catalog?.library?.file.map { URL(fileURLWithPath: $0) } : nil
        chooser(missing)
    }

    func openSource() {
        if let file = model.catalog?.library?.file {
            NSWorkspace.shared.open(URL(fileURLWithPath: file))
        } else if let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: "org.zotero.zotero") {
            NSWorkspace.shared.openApplication(at: url, configuration: NSWorkspace.OpenConfiguration())
        }
    }

    /// Return delivers what the footer shows. With references already added and a
    /// result highlighted for a new search, that result joins the citation first.
    func submit(whenReady: Bool = false) {
        if whenReady, controller.resolveLibraryIfNeeded() { return }
        if model.hasDraft, !model.query.isEmpty, let row = model.selection, !model.isSelectionStaged,
           model.canChangeSelectionMembership {
            model.toggleCitation(row)
            model.deliver(whenReady: true) { [weak self] in self?.dismiss(forDelivery: true) }
            return
        }
        model.deliver(whenReady: whenReady) { [weak self] in self?.dismiss(forDelivery: true) }
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
            commandHold = Task { [weak controller] in
                try? await Task.sleep(for: .seconds(1))
                if !Task.isCancelled { controller?.showNumbers(true) }
            }
        } else {
            commandHold?.cancel()
            commandHold = nil
            controller.showNumbers(false)
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
        if controller.editing != nil {
            // The fields in the row keep Tab and typing; Return and Escape close the row.
            if modifiers.isEmpty, isReturn || character == "\u{1b}" {
                if !event.isARepeat { controller.closeEditing() }
                return true
            }
            if modifiers.isEmpty || modifiers == [.shift] { return false }
        }
        if character == "\t" || character == "\u{19}", modifiers.isEmpty || modifiers == [.shift] {
            if modifiers.isEmpty, !event.isARepeat { controller.addSelection() }
            return true
        }
        if isReturn {
            if modifiers == [.shift] || modifiers == [.option] {
                controller.runCommand(.membership, repeating: event.isARepeat)
                return true
            }
            if modifiers.isEmpty {
                if !event.isARepeat { controller.returnKey() }
                return true
            }
        }
        if modifiers.isEmpty, character == "\u{1b}" {
            if !event.isARepeat, !controller.escape() { dismiss() }
            return true
        }
        let inSearch = controller.searchField?.hasFocus == true
        if inSearch, modifiers.isEmpty {
            if key == .upArrow || key == .downArrow {
                controller.move(key == .upArrow ? -1 : 1)
                return true
            }
            if key == .delete, model.query.isEmpty {
                if model.shorthand { model.toggleShorthand(); return true }
                if controller.deleteInEmptyField() { return true }
            }
        }
        if modifiers == [.shift], key == .upArrow || key == .downArrow,
           controller.reorderPinned(key == .upArrow ? -1 : 1) { return true }
        if modifiers == [.command], character == "k" {
            if !event.isARepeat { controller.openActionsMenu() }
            return true
        }
        if modifiers == [.command], let character, let number = Int(character), (1...9).contains(number) {
            if !event.isARepeat {
                controller.selectNumber(number)
                submit(whenReady: true)
            }
            return true
        }
        guard let command = PickerCommand.matching(character, modifiers: modifiers) else { return false }
        return controller.runCommand(command, repeating: event.isARepeat)
    }
}

/// The system's Liquid Glass behind the picker, and an opaque window with
/// Reduce Transparency, as the system's own panels do.
@MainActor
private final class PickerGlass: NSView {
    private static let corner: CGFloat = 26
    private let content: NSView
    private var glass: NSGlassEffectView?
    private var reduced: Bool?

    init(content: NSView) {
        self.content = content
        super.init(frame: .zero)
        wantsLayer = true
        layer?.cornerRadius = Self.corner
        layer?.cornerCurve = .continuous
        layer?.masksToBounds = true
        if let host = content as? NSHostingView<PickerView> {
            host.safeAreaRegions = []
            host.sizingOptions = []
        }
        refresh()
    }

    required init?(coder: NSCoder) { nil }

    func refresh() {
        let reduce = NSWorkspace.shared.accessibilityDisplayShouldReduceTransparency
        layer?.borderWidth = NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast ? 1 : 0
        layer?.borderColor = NSColor.separatorColor.cgColor
        guard reduced != reduce else { return }
        reduced = reduce
        content.removeFromSuperview()
        glass?.removeFromSuperview()
        glass = nil
        if reduce {
            layer?.backgroundColor = NSColor.windowBackgroundColor.cgColor
            addSubview(content)
        } else {
            layer?.backgroundColor = NSColor.clear.cgColor
            let view = NSGlassEffectView()
            view.style = .regular
            view.cornerRadius = Self.corner
            view.contentView = content
            addSubview(view)
            glass = view
        }
        needsLayout = true
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer?.borderColor = NSColor.separatorColor.cgColor
            if reduced == true { layer?.backgroundColor = NSColor.windowBackgroundColor.cgColor }
        }
    }

    override func layout() {
        super.layout()
        glass?.frame = bounds
        content.frame = bounds
    }
}
