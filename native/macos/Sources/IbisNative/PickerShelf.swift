import AppKit

@MainActor
final class PickerShelf: NSView {
    let library = PickerButton()
    let action = PickerDeliveryButton()
    let more = PickerButton()
    private let locator = ShelfLocator()
    private let beak = NSImageView()
    private let scroll = NSScrollView()
    private let document = NSView()
    private let rule = PickerRule()
    private let pending = pickerLabel(PickerWords.text(.formatting))
    private var spans: [(view: NSView, width: CGFloat)] = []
    private var tokens: [(id: ItemRef, button: PickerButton)] = []
    private var rendered: PickerPreviewResult?
    private var renderedRows: [PickerRow] = []
    private var renderedScale: CGFloat = 0
    var edit: ((ItemRef) -> Void)?
    var remove: ((ItemRef) -> Void)?
    var select: ((ItemRef) -> Void)?

    override var isFlipped: Bool { true }

    override init(frame: NSRect) {
        super.init(frame: frame)
        beak.image = PickerArt.image("ibis-quote-beak")
        beak.imageScaling = .scaleProportionallyUpOrDown
        beak.setAccessibilityElement(false)
        scroll.drawsBackground = false
        scroll.scrollerStyle = .overlay
        scroll.hasHorizontalScroller = true
        scroll.autohidesScrollers = true
        scroll.automaticallyAdjustsContentInsets = false
        scroll.documentView = document
        pending.textColor = .secondaryLabelColor
        pending.setAccessibilityElement(true)
        more.text = PickerWords.text(.more)
        more.key = "⌘K"
        more.setAccessibilityLabel(PickerWords.text(.more))
        library.setAccessibilityLabel(PickerWords.text(.library))
        [rule, beak, scroll, pending, locator, library, action, more].forEach(addSubview)
    }

    required init?(coder: NSCoder) { nil }

    var keyViews: [NSView] {
        tokens.map(\.button).filter { !$0.isHidden && $0.isEnabled } + locator.keyViews + [library, action, more]
    }
    var displayed: [ItemRef] { tokens.map(\.id) }
    var focused: ItemRef? {
        tokens.first { $0.button === window?.firstResponder }?.id
    }
    var locatorHasFocus: Bool { locator.hasFocus }
    func focusLocator() { locator.focus() }

    func anchor(for id: ItemRef) -> NSView? {
        guard !scroll.isHidden, let button = tokens.first(where: { $0.id == id })?.button else { return nil }
        document.scrollToVisible(button.frame)
        return button
    }

    func focus(_ id: ItemRef? = nil) -> Bool {
        guard let id = id ?? tokens.last?.id, let anchor = anchor(for: id),
              window?.makeFirstResponder(anchor) == true else { return false }
        return true
    }

    func configure(model: PickerModel, option: Bool, metrics m: PickerMetrics, width: CGFloat) {
        for button in [library, more] { button.metrics = m }
        let name = model.catalog?.library?.name
        let needingAttention = model.searchReply?.status.first(where: { $0.kind == .library })?.text
            ?? model.catalog?.status.first?.text
        let alert = needingAttention != nil || (name != nil && !model.libraryReady)
        library.isHidden = !alert
        library.text = name ?? PickerWords.text(.library)
        library.maxCaption = m.pt(100)
        library.toolTip = [name, needingAttention].compactMap { $0 }.joined(separator: "\n")

        let label = option ? PickerWords.text(.add) : model.deliveryTitle
        action.setAccessibilityLabel(label)
        action.isEnabled = option ? model.selection != nil : model.canDeliver
        action.toolTip = label
        action.setAccessibilityHelp(option ? "⇧↩" : PickerWords.text(.returnKey))
        let title = NSMutableAttributedString(string: label, attributes: [.font: m.font(13, .medium)])
        title.append(NSAttributedString(string: option ? "  ⇧↩" : "  ↩", attributes: [
            .font: m.font(12),
            .foregroundColor: action.isEnabled ? NSColor.secondaryLabelColor : NSColor.disabledControlTextColor,
        ]))
        action.attributedTitle = title

        let draft = model.citationRows
        if rendered != model.preview || renderedRows != draft || renderedScale != m.scale {
            rendered = model.preview
            renderedRows = draft
            renderedScale = m.scale
            buildSpans(preview: model.preview, entries: model.previewEntries, draft: draft, metrics: m)
        }
        let locatorWidth = locator.configure(model: model, metrics: m)
        scroll.alphaValue = model.canDeliver || model.editingCitation != nil ? 1 : 0.55
        scroll.isHidden = spans.isEmpty
        pending.isHidden = !model.isFormatting || !spans.isEmpty
        pending.font = m.font(13)
        beak.isHidden = scroll.isHidden && pending.isHidden
        beak.contentTintColor = PickerArt.accent(effectiveAppearance)

        let y = m.line + m.pt(8)
        let h = m.pt(28)
        let moreWidth = more.preferredWidth
        let libraryWidth = library.isHidden ? 0 : library.preferredWidth
        let actionWidth = min(action.fittingSize.width, max(m.pt(92), width * 0.35))
        let moreX = width - m.inset - moreWidth
        let actionX = moreX - m.pt(12) - actionWidth
        let libraryX = actionX - (library.isHidden ? 0 : m.pt(8)) - libraryWidth
        more.frame = NSRect(x: moreX, y: y, width: moreWidth, height: h)
        action.frame = NSRect(x: actionX, y: y, width: actionWidth, height: h)
        library.frame = NSRect(x: libraryX, y: y, width: libraryWidth, height: h)
        [more, action, library].forEach { $0.needsLayout = true }
        let icon = m.pt(20)
        beak.frame = NSRect(x: m.tileCenter - icon / 2, y: y + (h - icon) / 2, width: icon, height: icon)

        let available = max(0, libraryX - m.column - locatorWidth - m.pt(16))
        let spanWidth = spans.reduce(0) { $0 + $1.width }
        let contentWidth = pending.isHidden ? spanWidth : pickerWidth(pending.stringValue, font: m.font(13))
        let clipWidth = min(available, contentWidth)
        scroll.frame = NSRect(x: m.column, y: y, width: clipWidth, height: h)
        pickerLine(pending, x: m.column, y: y, width: clipWidth, height: h)
        locator.frame = NSRect(x: m.column + clipWidth + (clipWidth > 0 ? m.pt(8) : 0),
                               y: y, width: locatorWidth, height: h)

        var x: CGFloat = 0
        for (view, width) in spans {
            let hPart = view is PickerButton ? m.pt(24) : h
            view.frame = NSRect(x: x, y: (h - hPart) / 2, width: width, height: hPart)
            x += width
        }
        document.frame = NSRect(x: 0, y: 0, width: max(clipWidth, x), height: h)
        rule.frame = NSRect(x: 0, y: 0, width: width, height: m.line)
    }

    private func buildSpans(preview: PickerPreviewResult?, entries: [PickerModel.Entry], draft: [PickerRow],
                            metrics m: PickerMetrics) {
        let previous = Dictionary(grouping: spans.compactMap { $0.view as? PickerButton }, by: \.identifier)
        var retained = Set<ObjectIdentifier>()
        tokens = []
        spans = []

        func append(_ content: NSAttributedString, row: PickerRow?, notes: [String] = []) {
            guard !content.string.isEmpty else { return }
            if let row {
                // Spans can split a reference. Only its first button is a keyboard stop.
                let id = NSUserInterfaceItemIdentifier(row.id.library + "\n" + row.id.key)
                let button = previous[id]?.first { !retained.contains(ObjectIdentifier($0)) } ?? makeToken(row.id)
                button.identifier = id
                button.metrics = m
                button.maxCaption = m.pt(270)
                button.text = content.string
                button.richText = content
                button.toolTip = ([content.string] + notes).joined(separator: "\n")
                button.setAccessibilityLabel(PickerWords.text(.token, content.string))
                button.setAccessibilityHelp(PickerWords.text(.removeToken, row.title ?? content.string))
                spans.append((button, button.preferredWidth))
                if !tokens.contains(where: { $0.id == row.id }) { tokens.append((row.id, button)) }
                retained.insert(ObjectIdentifier(button))
                if button.superview == nil { document.addSubview(button) }
            } else {
                let label = pickerLabel()
                label.attributedStringValue = content
                label.textColor = .secondaryLabelColor
                spans.append((label, ceil(content.size().width) + 1))
                retained.insert(ObjectIdentifier(label))
                document.addSubview(label)
            }
        }
        if let preview, !preview.display.nodes.isEmpty,
           Set(entries.map(\.row.id)) == Set(draft.map(\.id)) {
            for node in preview.display.nodes {
                let item = node.item.flatMap(Int.init(exactly:)).flatMap { entries.indices.contains($0) ? entries[$0].row : nil }
                let notes = preview.warnings.filter { node.item != nil && $0.item == node.item }.map(\.text)
                append(styled(node.content, metrics: m), row: item, notes: notes)
            }
        } else {
            // Keep an editable reference anchored while a preview is refused or being rebuilt.
            for row in draft {
                let text = draft.count == 1 && !((preview?.text ?? "").isEmpty) && entries.first?.row.id == row.id
                    ? (preview?.text ?? "") : row.title ?? PickerWords.text(.untitled)
                append(NSAttributedString(string: text, attributes: [.font: m.font(13)]), row: row)
            }
        }
        for view in document.subviews where !retained.contains(ObjectIdentifier(view)) { view.removeFromSuperview() }
    }

    private func makeToken(_ item: ItemRef) -> PickerButton {
        let button = PickerButton()
        button.surface = .token
        button.remove = { [weak self] in self?.remove?(item) }
        button.perform = { [weak self] in self?.edit?(item) }
        button.didFocus = { [weak self] in self?.select?(item) }
        let menu = NSMenu()
        let removeItem = PickerMenuItem(PickerWords.text(.remove), key: "\u{8}") { [weak self] in self?.remove?(item) }
        removeItem.keyEquivalentModifierMask = []
        menu.addItem(removeItem)
        button.menu = menu
        return button
    }

    private func styled(_ runs: [DisplayRun], metrics m: PickerMetrics) -> NSAttributedString {
        let text = NSMutableAttributedString(string: "")
        for run in runs {
            let superscript = run.script == .superscript ? 1 : (run.script == .`subscript` ? -1 : 0)
            var font = m.font(superscript == 0 ? 13 : 10, run.bold == true ? .semibold : .regular)
            if run.italic == true { font = NSFontManager.shared.convert(font, toHaveTrait: .italicFontMask) }
            if run.smallCaps == true, let caps = NSFont(descriptor: font.fontDescriptor.addingAttributes([
                .featureSettings: [[NSFontDescriptor.FeatureKey.typeIdentifier: kLowerCaseType,
                                    NSFontDescriptor.FeatureKey.selectorIdentifier: kLowerCaseSmallCapsSelector]]
            ]), size: font.pointSize) { font = caps }
            let attributes: [NSAttributedString.Key: Any] = [
                .font: font,
                .baselineOffset: m.pt(CGFloat(superscript * 3)),
                .underlineStyle: run.underline == true ? NSUnderlineStyle.single.rawValue : 0,
            ]
            text.append(NSAttributedString(string: run.text.replacingOccurrences(of: "\n", with: " "), attributes: attributes))
        }
        return text
    }
}

/// Keeps the locator beside the preview even when the preview must scroll.
@MainActor
private final class ShelfLocator: NSView {
    private let group = NSBox()
    private let types = PickerPopup(compactTitle: true)
    private let value = PickerField()
    private weak var model: PickerModel?
    private var item: ItemRef?

    override init(frame: NSRect) {
        super.init(frame: frame)
        group.boxType = .custom
        group.titlePosition = .noTitle
        group.borderColor = .separatorColor
        group.fillColor = .controlBackgroundColor.withAlphaComponent(0.24)
        group.setAccessibilityElement(false)
        addSubview(group)
        addSubview(types)
        addSubview(value)
        types.setAccessibilityLabel(PickerWords.text(.locatorType))
        value.isBezeled = false
        value.drawsBackground = false
        value.setAccessibilityHelp("⌘L")
        types.change = { [weak self] index in
            guard let self, let item, let choices = model?.catalog?.locators, choices.indices.contains(index) else { return }
            model?.changeDetails(for: item) { $0.term = choices[index].term }
        }
        value.change = { [weak self] text in
            guard let self, let item else { return }
            model?.changeDetails(for: item) { $0.locator = text }
        }
    }

    required init?(coder: NSCoder) { nil }

    var keyViews: [NSView] { [types, value] }
    var hasFocus: Bool {
        guard let first = window?.firstResponder else { return false }
        return value.currentEditor() === first || value === first
    }

    func focus() {
        guard value.isEnabled else { return }
        window?.makeFirstResponder(value)
    }

    func configure(model: PickerModel, metrics m: PickerMetrics) -> CGFloat {
        self.model = model
        item = model.citationItem
        let details = item.map(model.details(for:)) ?? CitationDetails()
        let options = model.catalog?.locators ?? []
        let index = options.firstIndex(where: { $0.term == details.term }) ?? 0
        let term = options.indices.contains(index) ? options[index] : nil
        let title = term?.name ?? PickerWords.text(.page)
        let short = term?.shortName ?? PickerWords.text(.pageShort)
        let font = m.font(13)
        types.font = font
        types.setOptions(options.map(\.name), selected: index, shortTitle: short)
        types.toolTip = title
        types.isEnabled = item != nil && !options.isEmpty
        value.font = font
        value.isEnabled = item != nil
        value.placeholderAttributedString = NSAttributedString(string: title, attributes: [
            .foregroundColor: value.isEnabled ? NSColor.labelColor.withAlphaComponent(0.76) : NSColor.placeholderTextColor,
        ])
        value.setAccessibilityLabel(title)
        value.show(details.locator)
        let typeWidth = min(m.pt(80), max(m.pt(36), pickerWidth(short, font: font) + m.pt(20)))
        let valueWidth = max(m.pt(44), min(m.pt(80), pickerWidth(details.locator.isEmpty ? title : details.locator, font: font) + m.pt(10)))
        types.frame = NSRect(x: 0, y: 0, width: typeWidth, height: m.pt(28))
        value.frame = NSRect(x: typeWidth + m.pt(2), y: m.pt(3), width: valueWidth, height: m.pt(22))
        let width = typeWidth + valueWidth + m.pt(6)
        group.cornerRadius = m.pt(6)
        group.borderWidth = m.line
        group.frame = NSRect(x: 0, y: m.pt(2), width: width, height: m.pt(24))
        return width
    }
}
