import AppKit

@MainActor
final class PickerShelf: NSView {
    let library = PickerButton()
    let action = PickerButton()
    let more = PickerButton()
    private let beak = NSImageView()
    private let scroll = NSScrollView()
    private let document = NSView()
    private let rule = PickerRule()
    private let pending = pickerLabel(PickerWords.text(.formatting))
    private var spans: [(view: NSView, width: CGFloat)] = []
    private var tokens: [(id: ItemRef, button: PickerButton)] = []
    private var rendered: PickerPreviewResult?
    private var renderedScale: CGFloat = 0
    private var turnedDark: Bool?
    var edit: ((ItemRef) -> Void)?
    var remove: ((ItemRef) -> Void)?

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
        more.secondary = true
        more.key = "⌘K"
        more.setAccessibilityLabel(PickerWords.text(.more))
        action.setAccessibilityHelp(PickerWords.text(.returnKey))
        action.medium = true
        library.setAccessibilityLabel(PickerWords.text(.library))
        [rule, beak, scroll, pending, library, action, more].forEach(addSubview)
    }

    required init?(coder: NSCoder) { nil }

    var keyViews: [NSView] {
        tokens.map(\.button).filter { !$0.isHidden && $0.isEnabled } + [library, action, more]
    }
    var displayed: [ItemRef] { tokens.map(\.id) }
    var focused: ItemRef? {
        tokens.first { $0.button === window?.firstResponder }?.id
    }

    func focus(_ id: ItemRef? = nil) -> Bool {
        guard let token = id.flatMap({ id in tokens.first { $0.id == id } }) ?? tokens.last,
              !scroll.isHidden, window?.makeFirstResponder(token.button) == true else { return false }
        document.scrollToVisible(token.button.frame)
        return true
    }

    func configure(model: PickerModel, option: Bool, metrics m: PickerMetrics, width: CGFloat) {
        for button in [library, action, more] { button.metrics = m }
        let showPreview = model.preview?.text.isEmpty == false || model.isFormatting
        let name = model.catalog?.library?.name
        let needingAttention = model.searchReply?.status.first(where: { $0.kind == .library })?.text
            ?? model.catalog?.status.first?.text
        let alert = needingAttention != nil || (name != nil && !model.libraryReady)
        library.text = alert ? (name ?? PickerWords.text(.library)) : ""
        library.icon = alert ? nil : "library"
        library.maxCaption = m.pt(100)
        library.toolTip = [name, needingAttention].compactMap { $0 }.joined(separator: "\n")

        let destination = (model.pasteExpected ? model.targetName : nil).map { PickerWords.text(.paste, $0) }
        let label = option ? PickerWords.text(.add) : destination ?? PickerWords.text(.copy)
        action.text = label
        action.setAccessibilityLabel(label)
        action.key = option ? "⇧↩" : "↩"
        action.accentKey = !option
        action.maxCaption = max(m.pt(34), min(m.pt(235), width * 0.33))
        action.isEnabled = option ? model.selection != nil : model.canDeliver
        action.toolTip = label
        action.setAccessibilityHelp(option ? action.key : PickerWords.text(.returnKey))
        let previewProblem = model.preview?.problem != nil
        let presenting = model.isFormatting
        let saved = focused
        let dark = PickerArt.isDark(effectiveAppearance)
        if rendered != model.preview || renderedScale != m.scale || turnedDark != dark {
            rendered = model.preview
            renderedScale = m.scale
            turnedDark = dark
            buildSpans(preview: model.preview, entries: model.previewEntries, metrics: m)
        }
        scroll.alphaValue = model.canDeliver ? 1 : 0.55
        scroll.isHidden = presenting || !showPreview
        pending.isHidden = !presenting
        beak.isHidden = !showPreview
        pending.font = m.font(13)
        beak.contentTintColor = PickerArt.accent(effectiveAppearance)

        let y = m.line + m.pt(8)
        let h = m.pt(28)
        let moreWidth = more.preferredWidth
        let libraryWidth = library.preferredWidth
        let actionWidth = action.preferredWidth
        let moreX = width - m.inset - moreWidth
        let actionX = moreX - m.pt(12) - actionWidth
        let libraryX = actionX - m.pt(8) - libraryWidth
        more.frame = NSRect(x: moreX, y: y, width: moreWidth, height: h)
        action.frame = NSRect(x: actionX, y: y, width: actionWidth, height: h)
        library.frame = NSRect(x: libraryX, y: y, width: libraryWidth, height: h)
        [more, action, library].forEach { $0.needsLayout = true }
        let icon = m.pt(20)
        beak.frame = NSRect(x: m.tileCenter - icon / 2, y: y + (h - icon) / 2, width: icon, height: icon)
        let clipWidth = max(0, libraryX - m.column - m.pt(8))
        scroll.frame = NSRect(x: m.column, y: y, width: clipWidth, height: h)
        pickerLine(pending, x: m.column, y: y, width: clipWidth, height: h)
        var x: CGFloat = 0
        for (view, width) in spans {
            let token = view as? PickerButton
            let partHeight = token == nil ? h : m.pt(24)
            view.frame = NSRect(x: x, y: (h - partHeight) / 2, width: width, height: partHeight)
            token?.isEnabled = !previewProblem
            x += width
        }
        document.frame = NSRect(x: 0, y: 0, width: max(clipWidth, x), height: h)
        rule.frame = NSRect(x: 0, y: 0, width: width, height: m.line)
        if let saved, saved != focused, !scroll.isHidden { _ = focus(saved) }
    }

    private func buildSpans(preview: PickerPreviewResult?, entries: [PickerModel.Entry], metrics m: PickerMetrics) {
        document.subviews.forEach { $0.removeFromSuperview() }
        tokens = []
        spans = []
        guard let preview else { return }
        var nodes = preview.display.nodes
        if nodes.isEmpty, !preview.text.isEmpty {
            let plain = DisplayRun(bold: nil, italic: nil, script: nil, smallCaps: nil, text: preview.text, underline: nil)
            nodes = [DisplayNode(blocks: [], content: [plain], item: entries.count == 1 ? 0 : nil)]
        }
        for node in nodes {
            let content = styled(node.content, metrics: m)
            guard !content.string.isEmpty else { continue }
            if let index = node.item.map(Int.init), entries.indices.contains(index) {
                let row = entries[index].row
                let button = PickerButton()
                button.metrics = m
                button.surface = .token
                button.maxCaption = m.pt(270)
                button.text = content.string
                button.richText = content
                button.toolTip = content.string
                button.setAccessibilityLabel(PickerWords.text(.token, content.string))
                button.setAccessibilityHelp(PickerWords.text(.removeToken, row.title ?? content.string))
                button.remove = { [weak self] in self?.remove?(row.id) }
                button.perform = { [weak self] in self?.edit?(row.id) }
                document.addSubview(button)
                spans.append((button, button.preferredWidth))
                // Multipart style output keeps its own spans; keyboard walks each reference once.
                if !tokens.contains(where: { $0.id == row.id }) { tokens.append((row.id, button)) }
            } else {
                let label = pickerLabel()
                label.attributedStringValue = content
                label.textColor = .secondaryLabelColor
                document.addSubview(label)
                spans.append((label, ceil(content.size().width) + 1))
            }
        }
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
