import AppKit

struct PickerMetrics {
    let scale: CGFloat
    let density: Density
    func pt(_ points: CGFloat) -> CGFloat { points * scale }
    var header: CGFloat { pt(52) }
    var footer: CGFloat { pt(44) }
    var line: CGFloat { pt(1) }
    var inset: CGFloat { pt(20) }
    var tileCenter: CGFloat { pt(36) }
    var column: CGFloat { pt(64) }
    var row: CGFloat { pt(density == .compact ? 36 : 48) }
    var section: CGFloat { pt(23) }
    func font(_ size: CGFloat, _ weight: NSFont.Weight = .regular) -> NSFont {
        .systemFont(ofSize: pt(size), weight: weight)
    }
}

extension NSColor {
    static var pickerQuaternaryFill: NSColor {
        if #available(macOS 14, *) { return .quaternarySystemFill }
        return NSColor.unemphasizedSelectedContentBackgroundColor.withAlphaComponent(0.45)
    }

}

enum PickerArt {
    private static let images: [String: NSImage] = {
        guard let folder = Bundle.main.resourceURL?.appendingPathComponent("macos-picker"),
              let urls = try? FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil)
        else { return [:] }
        return Dictionary(uniqueKeysWithValues: urls.compactMap { url in
            guard url.pathExtension == "pdf", let image = NSImage(contentsOf: url) else { return nil }
            let name = url.deletingPathExtension().lastPathComponent
            image.isTemplate = !name.hasPrefix("pickerMark")
            return (name, image)
        })
    }()

    private struct Colors: Decodable { let scarlet: [String: String] }
    private static let scarlet: [String: String] = {
        guard let url = Bundle.main.url(forResource: "colors", withExtension: "json"),
              let data = try? Data(contentsOf: url),
              let colors = try? JSONDecoder().decode(Colors.self, from: data) else { return [:] }
        return colors.scarlet
    }()

    static func isDark(_ appearance: NSAppearance) -> Bool {
        appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
    }

    static func image(_ name: String) -> NSImage? { images[name] }

    static func accent(_ appearance: NSAppearance) -> NSColor {
        guard let hex = scarlet[isDark(appearance) ? "400" : "600"],
              let rgb = UInt32(hex.dropFirst(), radix: 16) else { return .labelColor }
        return NSColor(srgbRed: CGFloat((rgb >> 16) & 255) / 255, green: CGFloat((rgb >> 8) & 255) / 255,
                       blue: CGFloat(rgb & 255) / 255, alpha: 1)
    }
}

func pickerLabel(_ text: String = "") -> NSTextField {
    let label = NSTextField(labelWithString: text)
    label.lineBreakMode = .byTruncatingTail
    label.setAccessibilityElement(false)
    return label
}

private let pickerLabelAllowance: CGFloat = 6

func pickerWidth(_ text: String, font: NSFont) -> CGFloat {
    ceil((text as NSString).size(withAttributes: [.font: font]).width) + pickerLabelAllowance
}

func pickerLine(_ label: NSTextField, x: CGFloat, y: CGFloat, width: CGFloat, height: CGFloat) {
    let line = min(height, ceil(label.font?.boundingRectForFont.height ?? height))
    label.frame = NSRect(x: x, y: y + (height - line) / 2, width: max(0, width), height: line)
}

@MainActor
final class PickerRule: NSView {
    override func draw(_ dirtyRect: NSRect) {
        NSColor.separatorColor.setFill()
        bounds.fill()
    }
}

@MainActor
final class PickerSearchField: NSSearchField {
    var change: ((String) -> Void)?

    init() {
        super.init(frame: .zero)
        sendsSearchStringImmediately = true
        focusRingType = .default
        target = self
        action = #selector(reportChange)
    }

    required init?(coder: NSCoder) { nil }

    override func textDidChange(_ notification: Notification) {
        super.textDidChange(notification)
        reportChange()
    }

    @objc private func reportChange() { change?(stringValue) }
}

@MainActor
final class PickerDeliveryButton: NSButton {
    var perform: (() -> Void)?

    init() {
        super.init(frame: .zero)
        bezelStyle = .rounded
        setButtonType(.momentaryPushIn)
        cell?.lineBreakMode = .byTruncatingTail
        target = self
        action = #selector(deliver)
    }

    required init?(coder: NSCoder) { nil }

    @objc private func deliver() { perform?() }
}

@MainActor
final class PickerField: NSTextField {
    var change: ((String) -> Void)?
    private let plainFocus: Bool

    init(plainFocus: Bool = false) {
        self.plainFocus = plainFocus
        super.init(frame: .zero)
        isBordered = false
        isBezeled = false
        drawsBackground = false
        focusRingType = plainFocus ? .none : .default
        usesSingleLineMode = true
        cell?.isScrollable = true
        setContentHuggingPriority(.defaultLow, for: .horizontal)
    }

    required init?(coder: NSCoder) { nil }

    override func textDidBeginEditing(_ notification: Notification) {
        super.textDidBeginEditing(notification)
        if plainFocus { (currentEditor() as? NSTextView)?.focusRingType = .none }
    }

    override func textDidChange(_ notification: Notification) {
        super.textDidChange(notification)
        change?(stringValue)
    }

}

extension NSTextField {
    func show(_ text: String) {
        if stringValue != text {
            stringValue = text
            if let editor = currentEditor() {
                editor.string = text
                editor.selectedRange = NSRange(location: text.utf16.count, length: 0)
            }
        }
    }
}

@MainActor
final class PickerPopup: NSPopUpButton, NSMenuDelegate {
    var change: ((Int) -> Void)?
    private var examples: [String?] = []
    private let displayItem: NSMenuItem?

    init(compactTitle: Bool = false) {
        displayItem = compactTitle ? NSMenuItem() : nil
        super.init(frame: .zero, pullsDown: compactTitle)
        isBordered = false
        if let cell = cell as? NSPopUpButtonCell {
            cell.arrowPosition = .arrowAtCenter
            if let displayItem {
                cell.usesItemFromMenu = false
                cell.menuItem = displayItem
            }
        }
        imagePosition = .noImage
        target = self
        action = #selector(changed)
        focusRingType = .default
        menu?.delegate = self
    }

    required init?(coder: NSCoder) { nil }

    func setOptions(_ titles: [String], selected: Int, examples: [String?] = [], shortTitle: String? = nil) {
        self.examples = examples
        if itemTitles != titles {
            removeAllItems()
            addItems(withTitles: titles)
        }
        if (0..<numberOfItems).contains(selected) { selectItem(at: selected) }
        if let shortTitle, let displayItem {
            displayItem.title = shortTitle
            displayItem.attributedTitle = NSAttributedString(string: shortTitle, attributes: [
                .font: font ?? NSFont.systemFont(ofSize: 13),
                .foregroundColor: NSColor.secondaryLabelColor,
            ])
            for (index, item) in itemArray.enumerated() { item.state = index == selected ? .on : .off }
            needsDisplay = true
        }
    }

    func menuWillOpen(_ menu: NSMenu) {
        for (index, item) in menu.items.enumerated() where examples.indices.contains(index) {
            guard let example = examples[index], !example.isEmpty else { continue }
            item.attributedTitle = NSAttributedString(string: item.title + "  ·  " + example)
        }
    }

    func menuDidClose(_ menu: NSMenu) {
        for item in menu.items { item.attributedTitle = nil }
    }

    @objc private func changed() { change?(indexOfSelectedItem) }
}

@MainActor
final class PickerDragView: NSView {
    override var mouseDownCanMoveWindow: Bool { false }
    override func hitTest(_ point: NSPoint) -> NSView? { super.hitTest(point) == nil ? nil : self }
    override func mouseDown(with event: NSEvent) { window?.performDrag(with: event) }
}

@MainActor
final class PickerButton: NSButton {
    enum Surface { case control, token, link, primary }

    var perform: (() -> Void)?
    var remove: (() -> Void)?
    var didFocus: (() -> Void)?
    var text = "" { didSet { caption.stringValue = text } }
    var richText: NSAttributedString? {
        didSet {
            if let richText { caption.attributedStringValue = richText }
            else { caption.stringValue = text }
        }
    }
    var key = "" { didSet { keyLabel.stringValue = key } }
    var icon: String? { didSet { glyph.image = icon.flatMap(PickerArt.image) } }
    var medium = false
    var captionSize: CGFloat = 13
    var isCurrentToken = false { didSet { if isCurrentToken != oldValue { refreshColors() } } }
    var surface: Surface = .control
    var maxCaption: CGFloat = .greatestFiniteMagnitude
    var metrics = PickerMetrics(scale: 1, density: .compact) { didSet { needsLayout = true } }

    private let caption = pickerLabel()
    private let glyph = NSImageView()
    private let keyLabel = pickerLabel()
    private let removeGlyph = NSImageView()
    private var tracking: NSTrackingArea?
    private var hovered = false { didSet { refreshColors() } }
    private var focused = false { didSet { refreshColors() } }
    private var pressed = false

    init() {
        super.init(frame: .zero)
        isBordered = false
        bezelStyle = .regularSquare
        title = ""
        focusRingType = .exterior
        target = self
        action = #selector(activated)
        setButtonType(.momentaryChange)
        wantsLayer = true
        addSubview(caption)
        addSubview(glyph)
        addSubview(keyLabel)
        addSubview(removeGlyph)
        keyLabel.alignment = .left
        keyLabel.lineBreakMode = .byClipping
        keyLabel.cell?.wraps = false
        glyph.imageScaling = .scaleProportionallyUpOrDown
        removeGlyph.imageScaling = .scaleProportionallyUpOrDown
        glyph.setAccessibilityElement(false)
        removeGlyph.setAccessibilityElement(false)
        removeGlyph.image = PickerArt.image("close")
        removeGlyph.isHidden = true
    }

    required init?(coder: NSCoder) { nil }

    var preferredWidth: CGFloat {
        let m = metrics
        if icon != nil, text.isEmpty { return m.pt(28) }
        let pad = surface == .link ? m.pt(4) : m.pt(8)
        let labelWidth = richText.map { ceil($0.size().width) + m.pt(5) } ?? pickerWidth(text, font: captionFont)
        return pad * 2 + min(maxCaption, labelWidth) + hintWidth
            + (remove == nil ? 0 : m.pt(13))
    }

    private var captionFont: NSFont { metrics.font(captionSize, medium ? .medium : .regular) }
    private var keyWidth: CGFloat { pickerWidth(key, font: metrics.font(13)) }
    private var hintWidth: CGFloat {
        key.isEmpty ? 0 : keyWidth + metrics.pt(4) - pickerLabelAllowance
    }

    override func layout() {
        super.layout()
        let m = metrics
        let pad = surface == .link ? m.pt(4) : m.pt(8)
        caption.font = captionFont
        if let richText { caption.attributedStringValue = richText }
        glyph.isHidden = icon == nil || !text.isEmpty
        let iconSize = m.pt(18)
        glyph.frame = NSRect(x: (bounds.width - iconSize) / 2, y: (bounds.height - iconSize) / 2,
                             width: iconSize, height: iconSize)
        keyLabel.isHidden = key.isEmpty
        keyLabel.font = m.font(13)
        pickerLine(keyLabel, x: bounds.width - pad - keyWidth, y: 0, width: keyWidth, height: bounds.height)
        let close: CGFloat = remove == nil ? 0 : m.pt(13)
        removeGlyph.frame = NSRect(x: bounds.width - pad - close, y: (bounds.height - close) / 2,
                                   width: close, height: close)
        pickerLine(caption, x: pad, y: 0, width: bounds.width - pad * 2 - close - hintWidth,
                   height: bounds.height)
        refreshColors()
    }

    func refreshColors() {
        let m = metrics
        let contrast = NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast
        effectiveAppearance.performAsCurrentDrawingAppearance {
            let fill: NSColor = pressed && isEnabled ? .selectedControlColor
                : hovered && isEnabled ? .pickerQuaternaryFill : .clear
            let background: NSColor = switch surface {
            case .token: isCurrentToken ? .controlAccentColor.withAlphaComponent(0.15) : .pickerQuaternaryFill
            case .primary: hovered ? .selectedContentBackgroundColor : .controlAccentColor
            default: fill
            }
            layer?.backgroundColor = background.cgColor
            layer?.cornerRadius = surface == .token ? m.pt(6) : bounds.height / 2
            layer?.borderWidth = surface == .token && (contrast || isCurrentToken) ? m.line : 0
            layer?.borderColor = (isCurrentToken ? NSColor.controlAccentColor : .separatorColor).cgColor
            caption.textColor = !isEnabled ? .disabledControlTextColor
                : (surface == .primary ? .alternateSelectedControlTextColor : .labelColor)
            glyph.contentTintColor = .secondaryLabelColor
            removeGlyph.contentTintColor = .secondaryLabelColor
            removeGlyph.isHidden = remove == nil || !(focused || hovered)
            keyLabel.textColor = isEnabled ? .secondaryLabelColor : .disabledControlTextColor
        }
        needsDisplay = true
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        refreshColors()
    }

    override func updateTrackingAreas() {
        if let tracking { removeTrackingArea(tracking) }
        let area = NSTrackingArea(rect: .zero, options: [.mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect],
                                  owner: self, userInfo: nil)
        addTrackingArea(area)
        tracking = area
        super.updateTrackingAreas()
    }

    override func mouseEntered(with event: NSEvent) { hovered = true }
    override func mouseExited(with event: NSEvent) { hovered = false }

    override func hitTest(_ point: NSPoint) -> NSView? { super.hitTest(point) == nil ? nil : self }

    override func mouseDown(with event: NSEvent) {
        if !removeGlyph.isHidden, removeGlyph.frame.insetBy(dx: -metrics.pt(3), dy: -metrics.pt(4))
            .contains(convert(event.locationInWindow, from: nil)) {
            remove?()
        } else {
            super.mouseDown(with: event)
        }
    }

    override func becomeFirstResponder() -> Bool {
        let accepted = super.becomeFirstResponder()
        focused = accepted
        if accepted { didFocus?() }
        return accepted
    }

    override func highlight(_ flag: Bool) {
        super.highlight(flag)
        pressed = flag
        refreshColors()
    }

    override func resignFirstResponder() -> Bool {
        if super.resignFirstResponder() {
            focused = false
            return true
        }
        return false
    }

    override func drawFocusRingMask() {
        NSBezierPath(roundedRect: bounds, xRadius: layer?.cornerRadius ?? 0, yRadius: layer?.cornerRadius ?? 0).fill()
    }

    override var focusRingMaskBounds: NSRect { bounds }
    @objc private func activated() { perform?() }
}

@MainActor
final class PickerCapsule: NSView {
    var radius: CGFloat = 0
    var scarlet = false

    override func draw(_ dirtyRect: NSRect) {
        let outline = NSBezierPath(roundedRect: bounds, xRadius: radius, yRadius: radius)
        let color = scarlet
            ? PickerArt.accent(effectiveAppearance).withAlphaComponent(PickerArt.isDark(effectiveAppearance) ? 0.13 : 0.08)
            : NSColor.pickerQuaternaryFill
        color.setFill()
        outline.fill()
        if NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast {
            NSColor.separatorColor.setStroke()
            outline.stroke()
        }
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        needsDisplay = true
    }
}
