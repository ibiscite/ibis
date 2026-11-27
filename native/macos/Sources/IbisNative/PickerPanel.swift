import AppKit
import CIbisNative
import SwiftUI

// A nonactivating panel can take text input without making Ibis the menu-bar app.
@MainActor
final class PickerPanel: NSPanel, NSWindowDelegate {
    private static let initialSize = CGSize(width: 680, height: 380)
    private let model: PickerModel
    private var isOrderingOut = false
    weak var searchField: NSSearchField?

    init(model: PickerModel, chooseLibrary: @escaping () -> Void) {
        self.model = model
        super.init(
            contentRect: CGRect(origin: .zero, size: Self.initialSize),
            styleMask: [.titled, .fullSizeContentView, .nonactivatingPanel],
            backing: .buffered, defer: false
        )
        title = "Ibis Picker"
        titleVisibility = .hidden
        titlebarAppearsTransparent = true
        isFloatingPanel = true
        hidesOnDeactivate = false
        becomesKeyOnlyIfNeeded = false
        isExcludedFromWindowsMenu = true
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .ignoresCycle]
        let host = NSHostingView(rootView: PickerView(model: model, chooseLibrary: chooseLibrary) { [weak self] in
            self?.submit()
        })
        host.sizingOptions = []
        contentView = host
        delegate = self
    }

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    func summon() {
        let target = NSWorkspace.shared.frontmostApplication?.bundleIdentifier
        model.present(app: target)
        ibis_native_capture(windowNumber)
        if let screen = NSScreen.screens.first(where: { $0.frame.contains(NSEvent.mouseLocation) }) ?? NSScreen.main {
            // The upper quarter is the search anchor. The frame stays on screen.
            let usable = screen.visibleFrame
            setFrameTopLeftPoint(CGPoint(
                x: max(usable.minX, usable.midX - frame.width / 2),
                y: min(usable.maxY, max(usable.minY + frame.height, usable.maxY - usable.height / 4))
            ))
        }
        contentView?.layoutSubtreeIfNeeded()
        makeKeyAndOrderFront(nil)
        if let searchField {
            makeFirstResponder(searchField)
            searchField.selectText(nil)
        }
    }

    func dismiss(forDelivery: Bool = false) {
        isOrderingOut = true
        defer { isOrderingOut = false }
        model.dismiss(forDelivery: forDelivery)
        orderOut(nil)
    }

    func windowDidResignKey(_ notification: Notification) {
        if isVisible, !isOrderingOut { dismiss() }
    }

    private func submit() {
        model.deliver { [weak self] in self?.dismiss(forDelivery: true) }
    }

    override func sendEvent(_ event: NSEvent) {
        if event.type == .keyDown,
           event.modifierFlags.intersection([.command, .option, .control, .shift]).isEmpty,
           (firstResponder as? NSTextInputClient)?.hasMarkedText() != true {
            if event.charactersIgnoringModifiers == "\u{1b}" {
                if !event.isARepeat, !model.escape() { dismiss() }
                return
            }
            if let searchField, searchField.currentEditor() === firstResponder || searchField === firstResponder {
                switch event.specialKey {
                case .upArrow: model.move(-1); return
                case .downArrow: model.move(1); return
                case .carriageReturn, .enter:
                    if !event.isARepeat { submit() }
                    return
                default: break
                }
            }
        }
        super.sendEvent(event)
    }
}
