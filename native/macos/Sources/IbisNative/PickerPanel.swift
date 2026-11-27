import AppKit
import ApplicationServices
import CIbisNative
import SwiftUI

/// A nonactivating panel takes text without replacing the captured application.
@MainActor
final class PickerPanel: NSPanel, NSWindowDelegate {
    private static let preferred = CGSize(width: 720, height: 480)
    private static let minimum = CGSize(width: 480, height: 360)
    private let model: PickerModel
    private let surface: PickerSurface
    private let chooser: (URL?) -> Void
    private let sizes: UserDefaults?
    private weak var canvas: PickerCanvas?
    private var display: NSScreen?
    private var shown = false
    private var lastScale: CGFloat = 1
    private var lastAppearance: Appearance = .system
    private var commandHold: Task<Void, Never>?
    private var displayOptions: NSObjectProtocol?
    var choosingLibrary = false
    var openSettings: (() -> Void)?

    var searchField: NSTextField? { canvas?.search }
    var pickerIsOpen: Bool { shown }

    init(model: PickerModel, sizes: UserDefaults? = nil, chooseLibrary: @escaping (URL?) -> Void) {
        self.model = model
        self.sizes = sizes
        chooser = chooseLibrary
        surface = PickerSurface(view: NSHostingView(rootView: PickerView(model: model)))
        super.init(contentRect: NSRect(origin: .zero, size: Self.preferred),
                   styleMask: [.borderless, .nonactivatingPanel, .resizable], backing: .buffered, defer: false)
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
        autorecalculatesKeyViewLoop = false
        contentView = surface
        delegate = self
        displayOptions = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.accessibilityDisplayOptionsDidChangeNotification, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.refreshSurface() }
        }
    }

    deinit {
        if let displayOptions { NSWorkspace.shared.notificationCenter.removeObserver(displayOptions) }
    }

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    func attach(_ canvas: PickerCanvas) {
        self.canvas = canvas
        initialFirstResponder = canvas.search
        if isKeyWindow { makeFirstResponder(canvas.search) }
    }

    func summon() {
        let target = NSWorkspace.shared.frontmostApplication
        display = focusedDisplay(for: target)
            ?? NSScreen.screens.first { $0.frame.contains(NSEvent.mouseLocation) }
            ?? NSScreen.main
        if let display { place(on: display) }
        model.present(app: target?.bundleIdentifier, name: target?.localizedName, pid: target?.processIdentifier)
        ibis_native_capture(windowNumber)
        contentView?.layoutSubtreeIfNeeded()
        invalidateShadow()
        let reduceMotion = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        shown = true
        alphaValue = reduceMotion ? 1 : 0
        makeFirstResponder(searchField)
        makeKeyAndOrderFront(nil)
        canvas?.focusSearch()
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
        canvas?.closePopover()
        commandHold?.cancel()
        commandHold = nil
        canvas?.showNumbers(false)
        canvas?.showOption(false)
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

    func windowDidResignKey(_ notification: Notification) {
        if shown, !choosingLibrary, canvas?.hasOpenPopover != true { dismiss() }
    }

    func windowDidEndLiveResize(_ notification: Notification) {
        guard let screen, let key = keyForDisplay(screen) else { return }
        let scale = model.scale
        sizes?.set([Double(frame.width / scale), Double(frame.height / scale)], forKey: key)
    }

    func viewPreferencesChanged() {
        if model.appearance != lastAppearance {
            lastAppearance = model.appearance
            appearance = switch lastAppearance {
            case .system: nil
            case .dark: NSAppearance(named: .darkAqua)
            case .light: NSAppearance(named: .aqua)
            }
        }
        if model.scale != lastScale {
            lastScale = model.scale
            if let display = screen ?? display { place(on: display) }
            refreshSurface()
        }
    }

    private func refreshSurface() {
        surface.configure(scale: model.scale)
        canvas?.refresh()
        invalidateShadow()
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
        if let file = model.catalog?.library?.file { NSWorkspace.shared.open(URL(fileURLWithPath: file)) }
        else { openZotero() }
    }

    func openZotero() {
        if let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: "org.zotero.zotero") {
            NSWorkspace.shared.openApplication(at: url, configuration: NSWorkspace.OpenConfiguration())
        }
    }

    func submit(whenReady: Bool = false) {
        model.deliver(whenReady: whenReady) { [weak self] in self?.dismiss(forDelivery: true) }
    }

    override func sendEvent(_ event: NSEvent) {
        if let canvas {
            if event.type == .flagsChanged { flags(event.modifierFlags, view: canvas) }
            if event.type == .keyDown, routeKey(event, view: canvas) { return }
        }
        super.sendEvent(event)
    }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        // Route Command keys before AppKit asks the search field to handle them.
        if isKeyWindow, let canvas, routeKey(event, view: canvas) { return true }
        return super.performKeyEquivalent(with: event)
    }

    private func flags(_ flags: NSEvent.ModifierFlags, view: PickerCanvas) {
        view.showOption(flags.contains(.option))
        if flags.contains(.command) {
            guard commandHold == nil else { return }
            commandHold = Task { [weak view] in
                try? await Task.sleep(for: .seconds(1))
                if !Task.isCancelled { view?.showNumbers(true) }
            }
        } else {
            commandHold?.cancel()
            commandHold = nil
            view.showNumbers(false)
        }
    }

    func routeKey(_ event: NSEvent, view: PickerCanvas) -> Bool {
        let responder = (event.window ?? self).firstResponder
        guard (responder as? NSTextInputClient)?.hasMarkedText() != true else { return false }
        let modifiers = event.modifierFlags.intersection([.command, .option, .control, .shift])
        let key = event.specialKey
        let character = event.charactersIgnoringModifiers?.lowercased()
        let inSearch = searchField?.currentEditor() === responder || searchField === responder
        if modifiers.isEmpty, character == "\t", view.locatorHasFocus {
            view.focusSearch()
            return true
        }
        if key == .carriageReturn || key == .enter {
            if modifiers == [.shift] {
                if !event.isARepeat { view.addSelection() }
                return true
            }
            if modifiers.isEmpty || modifiers == [.option] {
                if !event.isARepeat {
                    if modifiers.contains(.option) { view.addSelection() } else { submit(whenReady: true) }
                }
                return true
            }
        }
        if modifiers.isEmpty, character == "\u{1b}" {
            if !event.isARepeat, !view.escape() { dismiss() }
            return true
        }
        if inSearch {
            if modifiers.isEmpty, key == .upArrow || key == .downArrow {
                view.move(key == .upArrow ? -1 : 1)
                return true
            }
            if modifiers.isEmpty, key == .delete, model.shorthand, model.query.isEmpty {
                model.toggleShorthand()
                return true
            }
        }
        if modifiers.isEmpty || modifiers == [.shift], view.tokenKey(key, shift: modifiers.contains(.shift)) { return true }
        if modifiers == [.command, .shift] {
            switch character {
            case "f": view.openFormat(); return true
            case "o": chooseLibrary(); return true
            case "+", "=": model.changeSize(10); return true
            default: break
            }
        }
        guard modifiers == [.command] else { return false }
        if let character, let number = Int(character), (1...9).contains(number) {
            view.number(number - 1)
            return true
        }
        if key == .downArrow { view.focusTokens(); return true }
        switch character {
        case "t": model.toggleShorthand(); view.focusSearch()
        case "i": view.showDetails()
        case "l": view.focusLocator()
        case "y": view.togglePeek()
        case "k": if !event.isARepeat { view.moreMenu() }
        case ",": if !event.isARepeat { openSettings?() }
        case "o": view.openSource()
        case "z": if model.canUndo { model.undoChange() } else { return false }
        case "+", "=": model.changeSize(10)
        case "-": model.changeSize(-10)
        default: return false
        }
        return true
    }
}

@MainActor
private final class PickerSurface: NSView {
    private let host: NSHostingView<PickerView>
    private var material: NSView?
    private var tint: NSView?
    private let outline = PickerOutline()
    private var corner: CGFloat = 16
    private var reducedTransparency: Bool?
    private let tintAmount = 0.70

    init(view: NSHostingView<PickerView>) {
        host = view
        super.init(frame: .zero)
        if #available(macOS 13.3, *) { host.safeAreaRegions = [] }
        if #available(macOS 13.0, *) { host.sizingOptions = [] }
        wantsLayer = true
        layer?.masksToBounds = true
        configure(scale: 1)
    }

    required init?(coder: NSCoder) { nil }

    func configure(scale: CGFloat) {
        corner = 16 * scale
        layer?.cornerRadius = corner
        outline.corner = corner
        outline.thickness = scale
        outline.isHidden = !NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast
        outline.needsDisplay = true
        let reduce = NSWorkspace.shared.accessibilityDisplayShouldReduceTransparency
        if reducedTransparency == reduce {
            #if compiler(>=6.2)
            if #available(macOS 26.0, *), let glass = material as? NSGlassEffectView {
                glass.cornerRadius = corner
            }
            #endif
            (material as? NSVisualEffectView)?.layer?.cornerRadius = corner
            return
        }
        reducedTransparency = reduce
        host.removeFromSuperview()
        material?.removeFromSuperview()
        tint?.removeFromSuperview()
        outline.removeFromSuperview()
        material = nil
        tint = nil
        if reduce {
            addSubview(host)
        } else {
            #if compiler(>=6.2)
            if #available(macOS 26.0, *) {
                let glass = NSGlassEffectView()
                glass.style = .regular
                glass.cornerRadius = corner
                glass.contentView = host
                addSubview(glass)
                material = glass
            } else {
                addPopover()
            }
            #else
            addPopover()
            #endif
        }
        addSubview(outline)
        colors()
        needsLayout = true
    }

    private func addPopover() {
        let popover = NSVisualEffectView()
        popover.material = .popover
        popover.blendingMode = .behindWindow
        popover.state = .active
        popover.wantsLayer = true
        popover.layer?.cornerRadius = corner
        popover.layer?.masksToBounds = true
        let overlay = NSView()
        overlay.wantsLayer = true
        addSubview(popover)
        addSubview(overlay)
        addSubview(host)
        material = popover
        tint = overlay
    }

    private func colors() {
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer?.backgroundColor = (material == nil ? NSColor.windowBackgroundColor : .clear).cgColor
            let tinted = NSColor.windowBackgroundColor.usingColorSpace(.sRGB)?.withAlphaComponent(tintAmount)
            tint?.layer?.backgroundColor = tinted?.cgColor
            #if compiler(>=6.2)
            if #available(macOS 26.0, *), let glass = material as? NSGlassEffectView {
                glass.tintColor = tinted
            }
            #endif
        }
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        colors()
        outline.needsDisplay = true
    }

    override func layout() {
        super.layout()
        material?.frame = bounds
        tint?.frame = bounds
        host.frame = bounds
        outline.frame = bounds
    }
}

private final class PickerOutline: NSView {
    var corner: CGFloat = 16
    var thickness: CGFloat = 1

    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    override func draw(_ dirtyRect: NSRect) {
        let inset = thickness / 2
        let radius = corner - inset
        let path = NSBezierPath(roundedRect: bounds.insetBy(dx: inset, dy: inset), xRadius: radius, yRadius: radius)
        path.lineWidth = thickness
        NSColor.separatorColor.setStroke()
        path.stroke()
    }
}
