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
    var register: (NSTextField) -> Void = { _ in }
    let change: (String) -> Void

    func makeNSView(context: Context) -> NSTextField {
        let field = NSTextField()
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

    func updateNSView(_ field: NSTextField, context: Context) {
        context.coordinator.parent = self
        if field.font != font { field.font = font }
        if field.placeholderString != placeholder { field.placeholderString = placeholder }
        field.setAccessibilityLabel(label.isEmpty ? placeholder : label)
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
}

extension NSTextField {
    var hasFocus: Bool {
        guard let window, let editor = currentEditor() else { return false }
        return window.firstResponder === editor
    }

    func focus(caretAtEnd: Bool = false) {
        guard let window, isEnabled else { return }
        if !hasFocus { window.makeFirstResponder(self) }
        guard caretAtEnd, let editor = currentEditor() else { return }
        editor.selectedRange = NSRange(location: editor.string.utf16.count, length: 0)
    }
}
