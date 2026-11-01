// Renders real AppKit controls at 2x for comparing with Ibis's own parts.
// swift docs/macos-controls.swift OUT_DIR
import AppKit
class Active: NSWindow { override var canBecomeKey: Bool { true }; override var isKeyWindow: Bool { true }; override var isMainWindow: Bool { true } }
let app = NSApplication.shared
app.setActivationPolicy(.accessory)
func label(_ text: String, _ x: CGFloat, _ y: CGFloat, _ v: NSView) {
  let l = NSTextField(labelWithString: text); l.font = .systemFont(ofSize: 11); l.textColor = .secondaryLabelColor; l.sizeToFit(); l.frame.origin = NSPoint(x: x, y: y); v.addSubview(l)
}
for (name, look) in [("light", NSAppearance.Name.aqua), ("dark", .darkAqua)] {
  let view = NSView(frame: NSRect(x: 0, y: 0, width: 920, height: 330))
  view.appearance = NSAppearance(named: look)
  view.wantsLayer = true
  view.layer?.backgroundColor = (look == .aqua ? NSColor.white : NSColor(white: 0.118, alpha: 1)).cgColor
  var y: CGFloat = 280
  func row(_ title: String, _ controls: [NSControl]) {
    label(title, 20, y + 4, view)
    var x: CGFloat = 200
    for c in controls { if c.frame.width == 0 { c.sizeToFit() }; c.frame.origin = NSPoint(x: x, y: y); view.addSubview(c); x += c.frame.width + 24 }
    y -= 46
  }
  let push = NSButton(title: "Check Now", target: nil, action: nil); push.bezelStyle = .push
  let def = NSButton(title: "Copy as Pandoc", target: nil, action: nil); def.bezelStyle = .push; def.keyEquivalent = "\r"
  let off = NSButton(title: "Disabled", target: nil, action: nil); off.bezelStyle = .push; off.isEnabled = false
  let large = NSButton(title: "Large", target: nil, action: nil); large.bezelStyle = .push; large.controlSize = .large
  row("NSButton push, default", [push, def, off, large])
  let sw1 = NSSwitch(); sw1.state = .on; let sw2 = NSSwitch(); sw2.state = .off
  let sw3 = NSSwitch(); sw3.state = .on; sw3.isEnabled = false
  row("NSSwitch", [sw2, sw1, sw3])
  let pop = NSPopUpButton(frame: .zero, pullsDown: false); pop.addItems(withTitles: ["Pandoc key", "APA"]); pop.sizeToFit()
  let seg = NSSegmentedControl(labels: ["Default", "Larger"], trackingMode: .selectOne, target: nil, action: nil); seg.selectedSegment = 0
  row("NSPopUpButton, segmented", [pop, seg])
  let search = NSSearchField(frame: NSRect(x: 0, y: 0, width: 220, height: 28)); search.placeholderString = "Search Settings"
  let field = NSTextField(frame: NSRect(x: 0, y: 0, width: 180, height: 24)); field.placeholderString = "Page"
  row("NSSearchField, NSTextField", [search, field])
  let toolbar = NSButton(image: NSImage(systemSymbolName: "xmark", accessibilityDescription: nil)!, target: nil, action: nil); toolbar.bezelStyle = .toolbar
  let pin = NSButton(image: NSImage(systemSymbolName: "pin", accessibilityDescription: nil)!, target: nil, action: nil); pin.bezelStyle = .accessoryBarAction
  let check = NSButton(checkboxWithTitle: "Check box", target: nil, action: nil); check.state = .on
  row("Toolbar icon, checkbox", [toolbar, pin, check])
  let w = Active(contentRect: view.frame, styleMask: [.borderless], backing: .buffered, defer: false)
  w.contentView = view; w.appearance = NSAppearance(named: look); w.orderFrontRegardless()
  RunLoop.current.run(until: Date().addingTimeInterval(0.6))
  let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: 1840, pixelsHigh: 660, bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
  rep.size = view.bounds.size
  view.cacheDisplay(in: view.bounds, to: rep)
  try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: CommandLine.arguments[1] + "/native-controls-\(name).png"))
  for c in [push, def, large, sw1, pop, seg, search, field, toolbar, pin] { print(name, type(of: c), Int(c.frame.width), Int(c.frame.height), c.layer?.cornerRadius ?? -1) }
  w.orderOut(nil)
}
