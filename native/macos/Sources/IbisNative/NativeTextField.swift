import AppKit
import SwiftUI

/// An AppKit text field for places that need exact focus and caret control.
/// SwiftUI lays it out; the field itself is the system's.
struct NativeTextField: NSViewRepresentable {
    enum Style { case plain, rounded }

    let text: String
    var placeholder = ""
    var label = ""
    var style = Style.rounded
    var font = NSFont.systemFont(ofSize: NSFont.systemFontSize)
    var isEnabled = true
    var register: (NSTextField) -> Void = { _ in }
    var began: () -> Void = {}
    let change: (String) -> Void

    func makeNSView(context: Context) -> Field {
        let field = Field()
        field.delegate = context.coordinator
        field.usesSingleLineMode = true
        field.lineBreakMode = .byTruncatingTail
        field.cell?.isScrollable = true
        field.cell?.wraps = false
        field.setContentHuggingPriority(.defaultLow, for: .horizontal)
        field.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        switch style {
        case .plain:
            field.isBordered = false
            field.isBezeled = false
            field.drawsBackground = false
            field.focusRingType = .none
        case .rounded:
            field.isBezeled = true
            field.bezelStyle = .roundedBezel
        }
        register(field)
        return field
    }

    func updateNSView(_ field: Field, context: Context) {
        context.coordinator.parent = self
        field.began = began
        if field.font != font { field.font = font }
        if field.placeholderString != placeholder { field.placeholderString = placeholder }
        field.setAccessibilityLabel(label.isEmpty ? placeholder : label)
        field.isEnabled = isEnabled
        if field.stringValue != text {
            field.stringValue = text
            if let editor = field.currentEditor() {
                editor.string = text
                editor.selectedRange = NSRange(location: text.utf16.count, length: 0)
            }
        }
    }

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    @MainActor
    final class Coordinator: NSObject, NSTextFieldDelegate {
        var parent: NativeTextField
        init(_ parent: NativeTextField) { self.parent = parent }

        func controlTextDidChange(_ notification: Notification) {
            guard let field = notification.object as? NSTextField else { return }
            parent.change(field.stringValue)
        }
    }

    final class Field: NSTextField {
        var began: () -> Void = {}

        override func becomeFirstResponder() -> Bool {
            let accepted = super.becomeFirstResponder()
            if accepted { began() }
            return accepted
        }
    }
}

extension NSTextField {
    var hasFocus: Bool {
        guard let window, let editor = currentEditor() else { return false }
        return window.firstResponder === editor
    }

    func focus(restoring selection: NSRange? = nil, caretAtEnd: Bool = false) {
        guard let window, isEnabled else { return }
        if !hasFocus { window.makeFirstResponder(self) }
        guard let editor = currentEditor() else { return }
        let length = editor.string.utf16.count
        if let selection {
            let location = min(selection.location, length)
            editor.selectedRange = NSRange(location: location, length: min(selection.length, length - location))
        } else if caretAtEnd {
            editor.selectedRange = NSRange(location: length, length: 0)
        }
    }
}

/// The system's own material, as a SwiftUI background.
struct SystemMaterial: NSViewRepresentable {
    var material: NSVisualEffectView.Material = .menu

    func makeNSView(context: Context) -> NSVisualEffectView {
        let view = NSVisualEffectView()
        view.material = material
        view.blendingMode = .withinWindow
        view.state = .active
        return view
    }

    func updateNSView(_ view: NSVisualEffectView, context: Context) { view.material = material }
}
