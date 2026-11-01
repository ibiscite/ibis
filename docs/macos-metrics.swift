// Prints the Mac's own design values (colours, type, control sizes, menus, sidebar rows, window chrome) as JSON.
// Run on a Mac: swift docs/macos-metrics.swift > /tmp/macos-metrics.json (see docs/design.md, "Exact macOS values")
// Reads only system values; no files, no network, no personal data.
import AppKit

func hex(_ c: NSColor) -> String {
  guard let s = c.usingColorSpace(.sRGB) else { return "n/a" }
  let r = Int((s.redComponent * 255).rounded()), g = Int((s.greenComponent * 255).rounded()), b = Int((s.blueComponent * 255).rounded())
  return String(format: "#%02X%02X%02X", r, g, b) + (s.alphaComponent < 0.999 ? String(format: " a%.3f", s.alphaComponent) : "")
}

var colors: [(String, NSColor)] = [
  ("label", .labelColor), ("secondaryLabel", .secondaryLabelColor), ("tertiaryLabel", .tertiaryLabelColor),
  ("quaternaryLabel", .quaternaryLabelColor), ("placeholderText", .placeholderTextColor),
  ("disabledControlText", .disabledControlTextColor), ("separator", .separatorColor), ("grid", .gridColor),
  ("windowBackground", .windowBackgroundColor), ("underPageBackground", .underPageBackgroundColor),
  ("controlBackground", .controlBackgroundColor), ("textBackground", .textBackgroundColor), ("control", .controlColor),
  ("controlAccent", .controlAccentColor), ("selectedContentBackground", .selectedContentBackgroundColor),
  ("unemphasizedSelectedContentBackground", .unemphasizedSelectedContentBackgroundColor),
  ("selectedTextBackground", .selectedTextBackgroundColor), ("selectedMenuItemText", .selectedMenuItemTextColor),
  ("alternateSelectedControlText", .alternateSelectedControlTextColor), ("keyboardFocusIndicator", .keyboardFocusIndicatorColor),
  ("link", .linkColor), ("findHighlight", .findHighlightColor),
  ("systemBlue", .systemBlue), ("systemPurple", .systemPurple), ("systemPink", .systemPink), ("systemRed", .systemRed),
  ("systemOrange", .systemOrange), ("systemYellow", .systemYellow), ("systemGreen", .systemGreen), ("systemGray", .systemGray)
]
if #available(macOS 14.0, *) {
  colors += [("quinaryLabel", .quinaryLabelColor), ("systemFill", .systemFill), ("secondarySystemFill", .secondarySystemFill),
             ("tertiarySystemFill", .tertiarySystemFill), ("quaternarySystemFill", .quaternarySystemFill), ("quinarySystemFill", .quinarySystemFill)]
}

var out: [String: Any] = ["macOS": ProcessInfo.processInfo.operatingSystemVersionString]
for (mode, name) in [("light", NSAppearance.Name.aqua), ("dark", .darkAqua), ("lightHighContrast", .accessibilityHighContrastAqua), ("darkHighContrast", .accessibilityHighContrastDarkAqua)] {
  var m: [String: String] = [:]
  NSAppearance(named: name)!.performAsCurrentDrawingAppearance { for (k, c) in colors { m[k] = hex(c) } }
  out[mode] = m
}

var type: [String: String] = [:]
let styles: [(String, NSFont.TextStyle)] = [("largeTitle", .largeTitle), ("title1", .title1), ("title2", .title2), ("title3", .title3), ("headline", .headline), ("subheadline", .subheadline), ("body", .body), ("callout", .callout), ("footnote", .footnote), ("caption1", .caption1), ("caption2", .caption2)]
for (k, s) in styles {
  let f = NSFont.preferredFont(forTextStyle: s)
  let w = (f.fontDescriptor.object(forKey: .traits) as? [NSFontDescriptor.TraitKey: Any])?[.weight] as? CGFloat ?? 0
  type[k] = String(format: "%.0f pt, weight %.2f, line %.1f", f.pointSize, w, NSLayoutManager().defaultLineHeight(for: f))
}
type["system"] = String(format: "%.0f", NSFont.systemFontSize); type["small"] = String(format: "%.0f", NSFont.smallSystemFontSize)
type["label"] = String(format: "%.0f", NSFont.labelFontSize); type["menu"] = String(format: "%.0f", NSFont.menuFont(ofSize: 0).pointSize)
out["type"] = type

var sizes: [String: [String: String]] = [:]
let controlSizes: [(String, NSControl.ControlSize)] = [("mini", .mini), ("small", .small), ("regular", .regular), ("large", .large)]
for (k, cs) in controlSizes {
  func h(_ v: NSControl) -> String { v.controlSize = cs; v.sizeToFit(); return String(format: "%.0f×%.0f", v.fittingSize.width, v.fittingSize.height) }
  let push = NSButton(title: "Copy", target: nil, action: nil); push.bezelStyle = .push
  let pop = NSPopUpButton(frame: .zero, pullsDown: false); pop.addItems(withTitles: ["Pandoc"])
  let seg = NSSegmentedControl(labels: ["Default", "Larger"], trackingMode: .selectOne, target: nil, action: nil)
  let search = NSSearchField(string: ""); search.placeholderString = "Search"
  let field = NSTextField(string: "12")
  let sw = NSSwitch()
  let check = NSButton(checkboxWithTitle: "Show", target: nil, action: nil)
  sizes[k] = ["pushButton": h(push), "popUp": h(pop), "segmented": h(seg), "searchField": h(search), "textField": h(field), "switch": h(sw), "checkbox": h(check)]
}
out["controls"] = sizes

// Menus: one item against five gives the item height and the menu's own padding.
func sz(_ s: NSSize) -> String { String(format: "%.1f×%.1f", s.width, s.height) }
func menu(_ n: Int, separator: Bool = false) -> NSMenu {
  let m = NSMenu(); for i in 0..<n { m.addItem(withTitle: "Item \(i)", action: nil, keyEquivalent: "") }
  if separator { m.addItem(.separator()); m.addItem(withTitle: "Last", action: nil, keyEquivalent: "") }
  return m
}
let m1 = menu(1).size, m5 = menu(5).size, m5s = menu(5, separator: true).size
let item = (m5.height - m1.height) / 4
out["menu"] = ["itemHeight": String(format: "%.1f", item), "verticalPadding": String(format: "%.1f", m1.height - item),
               "separatorHeight": String(format: "%.1f", m5s.height - m5.height - item)]

// Sidebar (source list) rows per size style. A plain table reports 24 for every style, so it is left out.
var rows: [String: String] = [:]
for (k, st) in [("small", NSTableView.RowSizeStyle.small), ("medium", .medium), ("large", .large), ("default", .default)] {
  let t = NSTableView(); t.style = .sourceList; t.rowSizeStyle = st; t.addTableColumn(NSTableColumn(identifier: .init("c")))
  rows[k] = String(format: "%.0f", t.rowHeight)
}
out["sidebarRows"] = rows

// Window chrome of a titled window (created, not shown).
let w = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 600, height: 400), styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: true)
let close = w.standardWindowButton(.closeButton)
out["window"] = ["titlebarHeight": String(format: "%.0f", w.frame.height - w.contentLayoutRect.height),
                 "closeButton": sz(close?.frame.size ?? .zero), "closeOrigin": String(format: "%.0f,%.0f", close?.frame.origin.x ?? 0, close?.frame.origin.y ?? 0)]

let data = try! JSONSerialization.data(withJSONObject: out, options: [.prettyPrinted, .sortedKeys])
print(String(data: data, encoding: .utf8)!)
