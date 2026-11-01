// Prints the Mac's own design values (colours, type, control sizes) as JSON.
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

let data = try! JSONSerialization.data(withJSONObject: out, options: [.prettyPrinted, .sortedKeys])
print(String(data: data, encoding: .utf8)!)
