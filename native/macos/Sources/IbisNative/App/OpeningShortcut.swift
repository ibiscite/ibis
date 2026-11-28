import AppKit
import Carbon
import CoreGraphics

enum ShortcutRefusal: Error {
    case invalid
    case needsModifier
    case reserved
}

/// A portable shortcut distinguishes a letter on the current keyboard layout
/// from a physical key such as KeyY or Digit1.
struct OpeningShortcut: Equatable {
    private struct Key: Equatable {
        let code: UInt16
        let name: String
        let symbol: String
        let display: String?

        init(_ code: Int, _ name: String, _ symbol: String, display: String? = nil) {
            self.code = UInt16(code)
            self.name = name
            self.symbol = symbol
            self.display = display
        }

        var isKeypad: Bool { name.hasPrefix("Numpad") }
    }

    private let key: Key
    private let followsLetter: Bool
    let modifiers: NSEvent.ModifierFlags
    private var registeredCode: UInt16?

    static let repoDefault = Self(key: Key(kVK_ANSI_Y, "KeyY", "y"), followsLetter: true,
                                  modifiers: [.command, .shift, .option])

    static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.key == rhs.key && lhs.followsLetter == rhs.followsLetter && lhs.modifiers == rhs.modifiers
    }

    static func parse(_ text: String) throws -> Self {
        var pieces = text.split(separator: "+", omittingEmptySubsequences: false)
        guard let last = pieces.popLast() else { throw ShortcutRefusal.invalid }
        let name = last.trimmingCharacters(in: .whitespaces)
        let letter = asciiLetter(name)
        guard let key = keys.first(where: { key in
            key.name.caseInsensitiveCompare(name) == .orderedSame
                || (key.display == nil && key.symbol.caseInsensitiveCompare(name) == .orderedSame)
        }) ?? keyAlias(name) else { throw ShortcutRefusal.invalid }
        var modifiers: NSEvent.ModifierFlags = []
        for piece in pieces {
            switch piece.trimmingCharacters(in: .whitespaces).lowercased() {
            case "ctrl", "control": modifiers.insert(.control)
            case "option", "alt": modifiers.insert(.option)
            case "shift": modifiers.insert(.shift)
            case "command", "cmd", "super", "commandorcontrol", "commandorctrl", "cmdorcontrol", "cmdorctrl":
                modifiers.insert(.command)
            default: throw ShortcutRefusal.invalid
            }
        }
        return try Self(key: key, followsLetter: letter != nil, modifiers: modifiers).validated()
    }

    static func record(_ event: NSEvent) throws -> Self {
        guard event.type == .keyDown,
              let physical = keys.first(where: { $0.code == event.keyCode }) else { throw ShortcutRefusal.invalid }
        let modifiers = event.modifierFlags.intersection([.control, .option, .shift, .command])
        let layer = modifiers.intersection(.command)
        let letter = physical.display == nil && !physical.isKeypad
            ? asciiLetter(event.characters(byApplyingModifiers: layer) ?? event.charactersIgnoringModifiers ?? "")
            : nil
        let logical = letter.flatMap { letter in letters.first { $0.symbol == letter } }
        return try Self(key: logical ?? physical, followsLetter: logical != nil, modifiers: modifiers).validated()
    }

    var savedValue: String {
        (modifierNames.map(\.name) + [followsLetter ? key.symbol.uppercased() : key.name]).joined(separator: "+")
    }

    var label: String {
        let character = menuKey
        let uppercase = character.uppercased()
        let symbol = key.display ?? (uppercase.count == 1 ? uppercase : character)
        return modifierNames.map(\.symbol).joined() + symbol
    }

    var menuKey: String {
        guard !key.isKeypad else { return "" }
        if key.display != nil { return key.symbol }
        let code = registeredCode ?? resolvedCode()
        return Self.character(code, on: modifiers.intersection(.command)) ?? key.symbol
    }

    var carbonModifiers: UInt32 {
        modifierNames.reduce(0) { result, modifier in result | modifier.carbon }
    }

    func resolvedCode() -> UInt16 {
        guard followsLetter else { return key.code }
        let layer = modifiers.intersection(.command)
        let source = CGEventSource(stateID: .privateState)
        let matches: (UInt16) -> Bool = { Self.character($0, on: layer, source: source) == key.symbol }
        if matches(key.code) { return key.code }
        // Keep the physical ANSI key usable if the input source has no such letter.
        return Self.printableKeys.first { matches($0.code) }?.code ?? key.code
    }

    func registered(at code: UInt16) -> Self {
        var shortcut = self
        shortcut.registeredCode = code
        return shortcut
    }

    func conflictsWithPicker(at code: UInt16) -> Bool {
        if modifiers == [.option], code == kVK_Return || code == kVK_ANSI_KeypadEnter { return true }
        let keyText = Self.character(code, on: modifiers.intersection([.command, .shift])) ?? key.symbol
        let isLetter: (String, Int) -> Bool = { letter, position in
            keyText == letter || (code == position && Self.asciiLetter(keyText) == nil)
        }
        if modifiers == [.command, .option] { return isLetter("o", kVK_ANSI_O) }
        guard modifiers == [.command, .shift] else { return false }
        return isLetter("f", kVK_ANSI_F) || isLetter("o", kVK_ANSI_O) || isLetter("p", kVK_ANSI_P)
            || ["=", "+", "/", "?"].contains(keyText)
    }

    private func validated() throws -> Self {
        guard !modifiers.intersection([.control, .option, .command]).isEmpty else {
            throw ShortcutRefusal.needsModifier
        }
        guard modifiers != [.command], !conflictsWithPicker(at: resolvedCode()) else { throw ShortcutRefusal.reserved }
        return self
    }

    private var modifierNames: [(name: String, symbol: String, carbon: UInt32)] {
        let values: [(NSEvent.ModifierFlags, String, String, Int)] = [
            (.control, "Control", "⌃", controlKey), (.option, "Alt", "⌥", optionKey),
            (.shift, "Shift", "⇧", shiftKey), (.command, "Command", "⌘", cmdKey),
        ]
        return values.compactMap { flag, name, symbol, carbon in
            modifiers.contains(flag) ? (name, symbol, UInt32(carbon)) : nil
        }
    }

    private static func asciiLetter(_ text: String) -> String? {
        let bytes = text.utf8
        guard bytes.count == 1, let byte = bytes.first,
              (65...90).contains(byte) || (97...122).contains(byte) else { return nil }
        return text.lowercased()
    }

    private static func character(_ code: UInt16, on modifiers: NSEvent.ModifierFlags,
                                  source: CGEventSource? = nil) -> String? {
        guard let key = CGEvent(keyboardEventSource: source, virtualKey: code, keyDown: true) else { return nil }
        var flags: CGEventFlags = []
        if modifiers.contains(.command) { flags.insert(.maskCommand) }
        if modifiers.contains(.shift) { flags.insert(.maskShift) }
        key.flags = flags
        guard let text = NSEvent(cgEvent: key)?.characters, text.count == 1 else { return nil }
        return text.lowercased()
    }

    private static func keyAlias(_ name: String) -> Key? {
        let aliases = [
            "esc": "Escape", "up": "ArrowUp", "down": "ArrowDown", "left": "ArrowLeft", "right": "ArrowRight",
            "numadd": "NumpadAdd", "numplus": "NumpadAdd", "numpadplus": "NumpadAdd",
            "numdecimal": "NumpadDecimal", "numdivide": "NumpadDivide", "numenter": "NumpadEnter",
            "numequal": "NumpadEqual", "nummultiply": "NumpadMultiply", "numsubtract": "NumpadSubtract",
        ]
        let name = name.lowercased()
        let canonical = aliases[name] ?? (name.count == 4 && name.hasPrefix("num") ? "Numpad" + name.suffix(1) : "")
        return keys.first { $0.name == canonical }
    }

    private static let letters = [
        (kVK_ANSI_A, "A"), (kVK_ANSI_B, "B"), (kVK_ANSI_C, "C"), (kVK_ANSI_D, "D"), (kVK_ANSI_E, "E"),
        (kVK_ANSI_F, "F"), (kVK_ANSI_G, "G"), (kVK_ANSI_H, "H"), (kVK_ANSI_I, "I"), (kVK_ANSI_J, "J"),
        (kVK_ANSI_K, "K"), (kVK_ANSI_L, "L"), (kVK_ANSI_M, "M"), (kVK_ANSI_N, "N"), (kVK_ANSI_O, "O"),
        (kVK_ANSI_P, "P"), (kVK_ANSI_Q, "Q"), (kVK_ANSI_R, "R"), (kVK_ANSI_S, "S"), (kVK_ANSI_T, "T"),
        (kVK_ANSI_U, "U"), (kVK_ANSI_V, "V"), (kVK_ANSI_W, "W"), (kVK_ANSI_X, "X"), (kVK_ANSI_Y, "Y"),
        (kVK_ANSI_Z, "Z"),
    ].map { code, letter in Key(code, "Key" + letter, letter.lowercased()) }

    private static let printableKeys = letters + [
        Key(kVK_ANSI_0, "Digit0", "0"), Key(kVK_ANSI_1, "Digit1", "1"), Key(kVK_ANSI_2, "Digit2", "2"),
        Key(kVK_ANSI_3, "Digit3", "3"), Key(kVK_ANSI_4, "Digit4", "4"), Key(kVK_ANSI_5, "Digit5", "5"),
        Key(kVK_ANSI_6, "Digit6", "6"), Key(kVK_ANSI_7, "Digit7", "7"), Key(kVK_ANSI_8, "Digit8", "8"),
        Key(kVK_ANSI_9, "Digit9", "9"),
        Key(kVK_ANSI_Grave, "Backquote", "`"), Key(kVK_ANSI_Backslash, "Backslash", "\\"),
        Key(kVK_ANSI_LeftBracket, "BracketLeft", "["), Key(kVK_ANSI_RightBracket, "BracketRight", "]"),
        Key(kVK_ANSI_Comma, "Comma", ","), Key(kVK_ANSI_Equal, "Equal", "="), Key(kVK_ANSI_Minus, "Minus", "-"),
        Key(kVK_ANSI_Period, "Period", "."), Key(kVK_ANSI_Quote, "Quote", "'"),
        Key(kVK_ANSI_Semicolon, "Semicolon", ";"), Key(kVK_ANSI_Slash, "Slash", "/"),
    ]

    private static let functionKeys = [
        kVK_F1, kVK_F2, kVK_F3, kVK_F4, kVK_F5, kVK_F6, kVK_F7, kVK_F8, kVK_F9, kVK_F10,
        kVK_F11, kVK_F12, kVK_F13, kVK_F14, kVK_F15, kVK_F16, kVK_F17, kVK_F18, kVK_F19, kVK_F20,
    ].enumerated().map { index, code in
        let label = "F\(index + 1)"
        return Key(code, label, functionCharacter(Int(NSF1FunctionKey) + index), display: label)
    }

    private static let keypad = [
        kVK_ANSI_Keypad0, kVK_ANSI_Keypad1, kVK_ANSI_Keypad2, kVK_ANSI_Keypad3, kVK_ANSI_Keypad4, kVK_ANSI_Keypad5,
        kVK_ANSI_Keypad6, kVK_ANSI_Keypad7, kVK_ANSI_Keypad8, kVK_ANSI_Keypad9,
    ].enumerated().map { number, code in Key(code, "Numpad\(number)", "", display: "#\(number)") }

    private static let keys = printableKeys + functionKeys + keypad + [
        Key(kVK_Space, "Space", " ", display: "␣"),
        Key(kVK_Return, "Enter", "\r", display: "↩"), Key(kVK_Tab, "Tab", "\t", display: "⇥"),
        Key(kVK_Escape, "Escape", "\u{1b}", display: "⎋"),
        Key(kVK_Delete, "Backspace", "\u{8}", display: "⌫"),
        Key(kVK_ForwardDelete, "Delete", functionCharacter(NSDeleteFunctionKey), display: "⌦"),
        Key(kVK_UpArrow, "ArrowUp", functionCharacter(NSUpArrowFunctionKey), display: "↑"),
        Key(kVK_DownArrow, "ArrowDown", functionCharacter(NSDownArrowFunctionKey), display: "↓"),
        Key(kVK_LeftArrow, "ArrowLeft", functionCharacter(NSLeftArrowFunctionKey), display: "←"),
        Key(kVK_RightArrow, "ArrowRight", functionCharacter(NSRightArrowFunctionKey), display: "→"),
        Key(kVK_Home, "Home", functionCharacter(NSHomeFunctionKey), display: "↖"),
        Key(kVK_End, "End", functionCharacter(NSEndFunctionKey), display: "↘"),
        Key(kVK_PageUp, "PageUp", functionCharacter(NSPageUpFunctionKey), display: "⇞"),
        Key(kVK_PageDown, "PageDown", functionCharacter(NSPageDownFunctionKey), display: "⇟"),
        Key(kVK_ANSI_KeypadDecimal, "NumpadDecimal", "", display: "#."),
        Key(kVK_ANSI_KeypadMultiply, "NumpadMultiply", "", display: "#*"),
        Key(kVK_ANSI_KeypadPlus, "NumpadAdd", "", display: "#+"),
        Key(kVK_ANSI_KeypadDivide, "NumpadDivide", "", display: "#/"),
        Key(kVK_ANSI_KeypadEnter, "NumpadEnter", "", display: "⌤"),
        Key(kVK_ANSI_KeypadMinus, "NumpadSubtract", "", display: "#−"),
        Key(kVK_ANSI_KeypadEquals, "NumpadEqual", "", display: "#="),
    ]

    private static func functionCharacter(_ value: Int) -> String {
        UnicodeScalar(value).map(String.init) ?? ""
    }
}
