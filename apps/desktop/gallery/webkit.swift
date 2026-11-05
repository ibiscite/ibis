// Renders the real canvas in the system's own WKWebView, offscreen, and checks the colours a
// person sees, for every accent in System, Light and Dark, then the layout rules of
// canvas/geometry.ts on every picker screen. Chromium cannot show these: WebKit's
// system colours and light-dark() both behave differently there. macOS only, invented data.
// pnpm -C apps/desktop build:canvas
// swiftc -O apps/desktop/gallery/webkit.swift -o /tmp/ibis-webkit
// /tmp/ibis-webkit apps/desktop/dist-canvas OUT_DIR
import AppKit
import WebKit

final class Loaded: NSObject, WKNavigationDelegate {
  var done: CheckedContinuation<Void, Never>?
  func webView(_ view: WKWebView, didFinish navigation: WKNavigation!) { done?.resume(); done = nil }
}

let dist = URL(fileURLWithPath: CommandLine.arguments[1]).standardizedFileURL
let out = URL(fileURLWithPath: CommandLine.arguments[2])
let accents = ["system", "ibis"]
let screens = ["results", "settings-look"]
// Picker screens whose layout rules are checked; the list grows with the canvas.
let geometryScreens = ["results", "first-run", "shelf", "reference-details", "show-as-menu", "update", "undo-notice", "larger-text", "right-to-left", "compact", "long-titles", "stale", "format-menu", "library-menu"]
// The Mac's appearance, then Ibis's own Appearance setting.
let modes: [(mac: NSAppearance.Name, name: String, chosen: String)] = [
  (.aqua, "light", "system"), (.darkAqua, "dark", "system"), (.aqua, "dark", "dark"), (.darkAqua, "light", "light"),
]
let checks = """
(() => {
  const root = document.documentElement, probe = document.body.appendChild(document.createElement("i"));
  const rgb = (value) => { probe.style.color = value; const [r, g, b, a = 1] = getComputedStyle(probe).color.match(/[\\d.]+/g).map(Number);
    return getComputedStyle(probe).color.startsWith("color(") ? [r, g, b, a] : [r / 255, g / 255, b / 255, a]; };
  const light = (c) => { const l = c.slice(0, 3).map((v) => v <= .04045 ? v / 12.92 : ((v + .055) / 1.055) ** 2.4); return .2126 * l[0] + .7152 * l[1] + .0722 * l[2]; };
  const ratio = (a, b) => { const [x, y] = [light(a), light(b)].sort((p, q) => q - p); return (x + .05) / (y + .05); };
  const pair = (fg, bg) => +ratio(rgb(`var(${fg})`), rgb(`var(${bg})`)).toFixed(2);
  const opaque = (name) => rgb(`var(${name})`)[3] === 1;
  const result = { scheme: getComputedStyle(root).colorScheme, opaque: opaque("--selected") && opaque("--primary") && opaque("--ring"),
    selected: pair("--selected-text", "--selected"), meta: pair("--selected-text-secondary", "--selected"), primary: pair("--text-on-accent", "--primary"),
    pressed: pair("--text-on-accent", "--accent-pressed"), ring: pair("--ring", "--surface") };
  probe.remove();
  return JSON.stringify(result);
})()
"""
func accentRGB(_ appearance: NSAppearance) -> [Double] {
  var rgb: [Double] = []
  appearance.performAsCurrentDrawingAppearance {
    let colour = NSColor.controlAccentColor.usingColorSpace(.sRGB)!
    rgb = [colour.redComponent, colour.greenComponent, colour.blueComponent].map(Double.init)
  }
  return rgb
}

@MainActor func run() async -> Int32 {
  try? FileManager.default.createDirectory(at: out, withIntermediateDirectories: true)
  let page = dist.appendingPathComponent("webkit.html")
  try! "<!doctype html><meta charset=utf-8><link rel=stylesheet href=ibis-canvas.css><div id=host style=height:100vh></div><script>Object.defineProperty(navigator,'platform',{value:'MacIntel'})</script><script src=ibis-canvas.js></script>".write(to: page, atomically: true, encoding: .utf8)
  defer { try? FileManager.default.removeItem(at: page) }
  var failures: [String] = []
  for mode in modes { for screen in screens { for accent in accents {
    let appearance = NSAppearance(named: mode.mac)!
    let size = NSSize(width: 920, height: screen == "results" ? 480 : 1320)
    let window = NSWindow(contentRect: NSRect(origin: NSPoint(x: -30000, y: -30000), size: size), styleMask: .borderless, backing: .buffered, defer: false)
    window.appearance = appearance
    let view = WKWebView(frame: NSRect(origin: .zero, size: size))
    window.contentView = view; window.orderBack(nil)
    let loaded = Loaded(); view.navigationDelegate = loaded
    await withCheckedContinuation { loaded.done = $0; view.loadFileURL(page, allowingReadAccessTo: dist) }
    _ = try? await view.evaluateJavaScript("IbisCanvas.mount(document.getElementById('host'), { scenario: '\(screen)', theme: '\(mode.name)' }); 0")
    try? await Task.sleep(nanoseconds: 500_000_000)
    let native = accent == "system" ? "IbisCanvas.setNativeAccent(\(accentRGB(appearance)));" : ""
    _ = try? await view.evaluateJavaScript("\(native) const r = document.documentElement; r.dataset.appearance = '\(mode.chosen)'; r.dataset.accent = '\(accent)'; r.dataset.nativeAppearance = 'liquid-glass'; 0")
    try? await Task.sleep(nanoseconds: 200_000_000)
    let name = "\(screen)-\(mode.mac == .aqua ? "mac-light" : "mac-dark")-\(mode.chosen)-\(accent)"
    let raw = (try? await view.evaluateJavaScript(checks)) as? String ?? "{}"
    let seen = (try? JSONSerialization.jsonObject(with: Data(raw.utf8))) as? [String: Any] ?? [:]
    let number = { (key: String) in seen[key] as? Double ?? 0 }
    var wrong: [String] = []
    if seen["opaque"] as? Bool != true { wrong.append("a role has no solid colour") }
    if (seen["scheme"] as? String ?? "normal") == "normal" { wrong.append("no colour scheme") }
    if number("selected") < 4.5 { wrong.append("selected text \(number("selected"))") }
    if number("meta") < 4.5 { wrong.append("selected secondary text \(number("meta"))") }
    // Apple's own blue default button is 4.02:1, kept as measured.
    if number("primary") < 4 { wrong.append("primary text \(number("primary"))") }
    if number("pressed") < 4.5 { wrong.append("pressed primary text \(number("pressed"))") }
    if number("ring") < 3 { wrong.append("focus ring \(number("ring"))") }
    if !wrong.isEmpty { failures.append("\(name): \(wrong.joined(separator: ", "))") }
    if let image = try? await view.takeSnapshot(configuration: nil), let tiff = image.tiffRepresentation,
       let png = NSBitmapImageRep(data: tiff)?.representation(using: .png, properties: [:]) {
      try? png.write(to: out.appendingPathComponent("\(name).png"))
    }
    window.orderOut(nil); window.contentView = nil
  } } }
  // The layout rules (canvas/geometry.ts), once per screen: they hold in every mode.
  for screen in geometryScreens {
    let size = NSSize(width: 720, height: 480)
    let window = NSWindow(contentRect: NSRect(origin: NSPoint(x: -30000, y: -30000), size: size), styleMask: .borderless, backing: .buffered, defer: false)
    let view = WKWebView(frame: NSRect(origin: .zero, size: size))
    window.contentView = view; window.orderBack(nil)
    let loaded = Loaded(); view.navigationDelegate = loaded
    await withCheckedContinuation { loaded.done = $0; view.loadFileURL(page, allowingReadAccessTo: dist) }
    _ = try? await view.evaluateJavaScript("IbisCanvas.mount(document.getElementById('host'), { scenario: '\(screen)', theme: 'light' }); 0")
    try? await Task.sleep(nanoseconds: 900_000_000)
    let raw = (try? await view.evaluateJavaScript("JSON.stringify(IbisCanvas.geometry())")) as? String ?? "[\"geometry did not run\"]"
    let broken = (try? JSONSerialization.jsonObject(with: Data(raw.utf8))) as? [String] ?? [raw]
    failures += broken.map { "\(screen): \($0)" }
    window.orderOut(nil); window.contentView = nil
  }
  print(failures.isEmpty ? "WebKit colours and layout: every accent, mode and screen passes." : failures.joined(separator: "\n"))
  return failures.isEmpty ? 0 : 1
}

let app = NSApplication.shared
app.setActivationPolicy(.prohibited)
Task { @MainActor in exit(await run()) }
app.run()
