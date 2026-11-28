import AppKit
import ApplicationServices
import CIbisNative
import Combine
import SwiftUI

/// A nonactivating panel takes text without replacing the captured application.
/// It owns the window, its glass and the keyboard; SwiftUI draws the content.
@MainActor
final class PickerPanel: NSPanel, NSWindowDelegate {
    private static let preferred = CGSize(width: 720, height: 480)
    private static let minimum = CGSize(width: 480, height: 360)
    let model: PickerModel
    let controller: PickerController
    private let host: NSHostingView<PickerView>
    private let glass: PickerGlass
    private let chooser: (URL?) -> Void
    private let sizes: UserDefaults?
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

    init(model: PickerModel, sizes: UserDefaults? = nil, chooseLibrary: @escaping (URL?) -> Void) {
        self.model = model
        self.sizes = sizes
        chooser = chooseLibrary
        controller = PickerController(model: model)
        host = NSHostingView(rootView: PickerView(model: model, controller: controller))
        glass = PickerGlass(content: host)
        super.init(contentRect: NSRect(origin: .zero, size: Self.preferred),
                   styleMask: [.borderless, .nonactivatingPanel, .resizable],
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
        controller.layer = nil
        controller.closeActions(restoring: false)
        commandHold?.cancel()
        commandHold = nil
        controller.showNumbers(false)
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
        // A popover takes the keyboard for its fields; the picker stays open beneath it.
        if shown, !choosingLibrary, controller.layer == nil { dismiss() }
    }

    func windowDidEndLiveResize(_ notification: Notification) {
        guard let screen, let key = keyForDisplay(screen) else { return }
        let scale = model.scale
        sizes?.set([Double(frame.width / scale), Double(frame.height / scale)], forKey: key)
    }

    /// Opens a native menu under the control that owns it.
    func popUp(_ menu: NSMenu, at anchor: CGRect?) {
        let rect = anchor ?? CGRect(x: 16, y: host.bounds.height - 48, width: 0, height: 0)
        let y = host.isFlipped ? rect.maxY + 4 : host.bounds.height - rect.maxY - 4
        menu.autoenablesItems = false
        menu.popUp(positioning: nil, at: NSPoint(x: rect.minX, y: y), in: host)
    }

    private func keyForDisplay(_ screen: NSScreen) -> String? {
        let key = NSDeviceDescriptionKey("NSScreenNumber")
        guard let id = (screen.deviceDescription[key] as? NSNumber)?.uint32Value,
              let uuid = CGDisplayCreateUUIDFromDisplayID(id)?.takeRetainedValue() else { return nil }
        let name = CFUUIDCreateString(nil, uuid) as String
        return "IbisPickerSize-\(name)"
    }

    private func place(on screen: NSScreen) {
        let area = screen.visibleFrame
        let scale = model.scale
        let saved = keyForDisplay(screen).flatMap { sizes?.array(forKey: $0) as? [Double] }
        let base = saved.flatMap { $0.count == 2 ? NSSize(width: $0[0], height: $0[1]) : nil } ?? Self.preferred
        maxSize = NSSize(width: area.width * 0.8, height: area.height * 0.8)
        minSize = NSSize(width: min(Self.minimum.width * scale, maxSize.width),
                         height: min(Self.minimum.height * scale, maxSize.height))
        let width = min(maxSize.width, max(minSize.width, base.width * scale))
        let height = min(maxSize.height, max(minSize.height, base.height * scale))
        let top = min(area.maxY, max(area.minY + height, area.maxY - area.height / 4 + 26 * scale))
        setFrame(NSRect(x: area.midX - width / 2, y: top - height, width: width, height: height), display: true)
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

    func submit(whenReady: Bool = false) {
        guard !controller.actionsOpen else { return }
        if whenReady, controller.resolveLibraryIfNeeded() { return }
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
        if controller.actionsOpen { return controller.actionKey(event) }
        let modifiers = event.modifierFlags.intersection([.command, .option, .control, .shift])
        let key = event.specialKey
        let character = event.charactersIgnoringModifiers?.lowercased()
        let inSearch = controller.searchField?.hasFocus == true
        if modifiers.isEmpty || modifiers == [.shift], character == "\t" || character == "\u{19}" {
            if inSearch, modifiers.isEmpty, model.citationItem != nil { controller.focusLocator() }
            else { controller.focusSearch() }
            return true
        }
        if key == .carriageReturn || key == .enter {
            if modifiers == [.shift] {
                controller.runCommand(.membership, repeating: event.isARepeat)
                return true
            }
            if modifiers.isEmpty || modifiers == [.option] {
                if !event.isARepeat {
                    if modifiers.contains(.option) { controller.runCommand(.membership) } else { submit(whenReady: true) }
                }
                return true
            }
        }
        if modifiers.isEmpty, character == "\u{1b}" {
            if !event.isARepeat, !controller.escape() { dismiss() }
            return true
        }
        if inSearch, modifiers.isEmpty {
            if key == .upArrow || key == .downArrow {
                controller.move(key == .upArrow ? -1 : 1)
                return true
            }
            if key == .delete, model.shorthand, model.query.isEmpty {
                model.toggleShorthand()
                return true
            }
        }
        if modifiers.isEmpty || modifiers == [.shift],
           controller.tokenKey(key, shift: modifiers.contains(.shift)) { return true }
        if modifiers == [.command], character == "k" {
            if !event.isARepeat { controller.showActions() }
            return true
        }
        if modifiers == [.command], let character, let number = Int(character), (1...9).contains(number) {
            controller.selectNumber(number)
            return true
        }
        guard let command = PickerCommand.matching(character, modifiers: modifiers) else { return false }
        return controller.runCommand(command, repeating: event.isARepeat)
    }
}

/// The system's glass behind the picker: Liquid Glass on macOS 26, the popover
/// material before it, and an opaque window with Reduce Transparency.
@MainActor
private final class PickerGlass: NSView {
    private let content: NSView
    private var material: NSView?
    private var reduced: Bool?
    private let corner: CGFloat

    init(content: NSView) {
        self.content = content
        if #available(macOS 26.0, *) { corner = 26 } else { corner = 16 }
        super.init(frame: .zero)
        wantsLayer = true
        layer?.cornerRadius = corner
        layer?.cornerCurve = .continuous
        layer?.masksToBounds = true
        if let host = content as? NSHostingView<PickerView> {
            if #available(macOS 13.3, *) { host.safeAreaRegions = [] }
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
        material?.removeFromSuperview()
        material = nil
        if reduce {
            layer?.backgroundColor = NSColor.windowBackgroundColor.cgColor
            addSubview(content)
        } else {
            layer?.backgroundColor = NSColor.clear.cgColor
            material = makeMaterial()
        }
        needsLayout = true
    }

    private func makeMaterial() -> NSView {
        #if compiler(>=6.2)
        if #available(macOS 26.0, *) {
            let glass = NSGlassEffectView()
            glass.style = .regular
            glass.cornerRadius = corner
            glass.contentView = content
            addSubview(glass)
            return glass
        }
        #endif
        let effect = NSVisualEffectView()
        effect.material = .popover
        effect.blendingMode = .behindWindow
        effect.state = .active
        addSubview(effect)
        addSubview(content)
        return effect
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
        material?.frame = bounds
        content.frame = bounds
    }
}
