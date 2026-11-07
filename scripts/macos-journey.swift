// The macOS hands of smoke-native.py --journey: accessibility reads, key
// posting and text selection, one command per run, one JSON line out.
// It reads and writes only what the journey names; it never searches other
// apps' windows. Build: swiftc -O scripts/macos-journey.swift -o helper
import AppKit
import ApplicationServices

func out(_ value: [String: Any]) {
    let data = try! JSONSerialization.data(withJSONObject: value, options: [.sortedKeys])
    FileHandle.standardOutput.write(data + Data("\n".utf8))
}

func fail(_ code: String) -> Never {
    out(["error": code])
    exit(1)
}

func attribute(_ element: AXUIElement, _ name: String) -> AnyObject? {
    var value: AnyObject?
    return AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success ? value : nil
}

func string(_ element: AXUIElement, _ name: String) -> String {
    attribute(element, name) as? String ?? ""
}

func elements(_ element: AXUIElement, _ name: String) -> [AXUIElement] {
    (attribute(element, name) as? [AnyObject] ?? []).map { $0 as! AXUIElement }
}

func pid(of element: AXUIElement) -> pid_t {
    var pid: pid_t = 0
    AXUIElementGetPid(element, &pid)
    return pid
}

func range(_ element: AXUIElement) -> [Int]? {
    guard let value = attribute(element, kAXSelectedTextRangeAttribute) else { return nil }
    var range = CFRange()
    guard AXValueGetValue(value as! AXValue, .cfRange, &range) else { return nil }
    return [range.location, range.length]
}

/// The app's one document text area, found by role (TextEdit has one per window).
func textArea(_ app: AXUIElement) -> AXUIElement? {
    var stack = elements(app, kAXWindowsAttribute)
    var seen = 0
    while let item = stack.popLast(), seen < 2000 {
        seen += 1
        if string(item, kAXRoleAttribute) == kAXTextAreaRole { return item }
        stack.append(contentsOf: elements(item, kAXChildrenAttribute))
    }
    return nil
}

/// Every window title and visible string of one app, bounded; used only on
/// Ibis's own windows, whose content is the journey's invented library.
func strings(_ app: AXUIElement) -> [String: Any] {
    let windows = elements(app, kAXWindowsAttribute)
    let titles = windows.map { string($0, kAXTitleAttribute) }
    var found: [String] = []
    var stack = Array(windows.reversed())
    var seen = 0
    while let item = stack.popLast(), seen < 4000 {
        seen += 1
        for name in [kAXValueAttribute, kAXTitleAttribute, kAXDescriptionAttribute] {
            let text = string(item, name)
            if !text.isEmpty { found.append(text) }
        }
        // Reversed, so strings come out in reading order and a title split by
        // match highlighting reads back joined.
        stack.append(contentsOf: elements(item, kAXChildrenAttribute).reversed())
    }
    return ["windows": titles, "strings": found, "complete": seen < 4000]
}

let keyCodes: [String: CGKeyCode] = ["a": 0, "y": 16, "return": 36, "escape": 53]

func post(_ code: CGKeyCode, _ flags: CGEventFlags, unicode: [UniChar] = []) {
    let source = CGEventSource(stateID: .hidSystemState)
    for down in [true, false] {
        guard let event = CGEvent(keyboardEventSource: source, virtualKey: code, keyDown: down) else { fail("event") }
        event.flags = flags
        if !unicode.isEmpty { event.keyboardSetUnicodeString(stringLength: unicode.count, unicodeString: unicode) }
        event.post(tap: .cghidEventTap)
        usleep(2_000)  // The HID tap drops a key-up posted in the same instant.
    }
}

let arguments = Array(CommandLine.arguments.dropFirst())
guard let command = arguments.first else { fail("usage") }
let system = AXUIElementCreateSystemWide()
switch command {
case "trusted":
    out(["accessibility": AXIsProcessTrusted(), "post_event": CGPreflightPostEventAccess()])
case "front":
    let app = NSWorkspace.shared.frontmostApplication
    out(["pid": Int(app?.processIdentifier ?? 0), "bundle": app?.bundleIdentifier ?? ""])
case "focus":
    // Where typed keys go now: the system-wide focused element and its owner.
    guard let focused = attribute(system, kAXFocusedUIElementAttribute) else { out(["pid": 0]); break }
    let element = focused as! AXUIElement
    out(["pid": Int(pid(of: element)), "role": string(element, kAXRoleAttribute),
         "label": string(element, kAXDescriptionAttribute), "value": string(element, kAXValueAttribute)])
case "picker":
    // A non-activating panel takes the keyboard without making Ibis the active
    // app, so the system-wide focus can stay with the app behind it. Report
    // Ibis's own focused element and which of its windows are on screen.
    guard arguments.count == 2, let target = pid_t(arguments[1]) else { fail("usage") }
    let listed = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
        as? [[String: Any]] ?? []
    let onscreen = listed.filter { ($0[kCGWindowOwnerPID as String] as? Int) == Int(target) }
        .map { $0[kCGWindowName as String] as? String ?? "" }
    let app = AXUIElementCreateApplication(target)
    let focused = attribute(app, kAXFocusedUIElementAttribute).map { $0 as! AXUIElement }
    let systemFocus = attribute(system, kAXFocusedUIElementAttribute).map { pid(of: $0 as! AXUIElement) } ?? 0
    var role = "", label = "", value = ""
    if let focused {
        role = string(focused, kAXRoleAttribute)
        label = string(focused, kAXDescriptionAttribute)
        if label.isEmpty { label = string(focused, kAXTitleAttribute) }
        value = string(focused, kAXValueAttribute)
    }
    out(["onscreen": onscreen, "role": role, "label": label, "value": value, "system_pid": Int(systemFocus)])
case "strings":
    guard arguments.count == 2, let target = pid_t(arguments[1]) else { fail("usage") }
    out(strings(AXUIElementCreateApplication(target)))
case "document":
    guard arguments.count == 2, let target = pid_t(arguments[1]) else { fail("usage") }
    guard let area = textArea(AXUIElementCreateApplication(target)) else { out(["available": false]); break }
    out(["available": true, "text": string(area, kAXValueAttribute), "selection": range(area) as Any])
case "select":
    guard arguments.count == 4, let target = pid_t(arguments[1]), let start = Int(arguments[2]),
          let length = Int(arguments[3]) else { fail("usage") }
    guard let area = textArea(AXUIElementCreateApplication(target)) else { fail("no-text-area") }
    var wanted = CFRange(location: start, length: length)
    guard let value = AXValueCreate(.cfRange, &wanted),
          AXUIElementSetAttributeValue(area, kAXSelectedTextRangeAttribute as CFString, value) == .success
    else { fail("select") }
    out(["selection": range(area) as Any])
case "key":
    // key NAME [cmd] [shift] [ctrl] [alt]
    guard arguments.count >= 2, let code = keyCodes[arguments[1]] else { fail("usage") }
    var flags: CGEventFlags = []
    for modifier in arguments.dropFirst(2) {
        switch modifier {
        case "cmd": flags.insert(.maskCommand)
        case "shift": flags.insert(.maskShift)
        case "ctrl": flags.insert(.maskControl)
        case "alt": flags.insert(.maskAlternate)
        default: fail("usage")
        }
    }
    post(code, flags)
    out(["posted": true])
case "type":
    guard arguments.count == 2 else { fail("usage") }
    for character in arguments[1] { post(0, [], unicode: Array(String(character).utf16)) }
    out(["posted": true])
case "close":
    // close PID TITLE-PART: press the close button of that app's matching window.
    guard arguments.count == 3, let target = pid_t(arguments[1]) else { fail("usage") }
    let windows = elements(AXUIElementCreateApplication(target), kAXWindowsAttribute)
        .filter { string($0, kAXTitleAttribute).contains(arguments[2]) }
    var closed = 0
    for window in windows {
        if let button = attribute(window, kAXCloseButtonAttribute),
           AXUIElementPerformAction(button as! AXUIElement, kAXPressAction as CFString) == .success { closed += 1 }
    }
    out(["closed": closed])
default:
    fail("usage")
}
