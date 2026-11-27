import AppKit

@MainActor
final class PickerDrawer: NSView {
    private let rule = PickerRule()
    private let titleLabel = NSTextField(wrappingLabelWithString: "")
    private let byline = NSTextField(wrappingLabelWithString: "")
    private let warnings = NSTextField(wrappingLabelWithString: "")
    private let peekFacts = NSTextField(wrappingLabelWithString: "")
    private let showAsLabel = pickerLabel(PickerWords.text(.showAs))
    private let forms = PickerPopup()
    private let capsule = PickerCapsule()
    private let locatorTypes = PickerPopup()
    let locator = PickerField()
    private let beforeButton = PickerButton()
    private let afterButton = PickerButton()
    private let beforeField = PickerField()
    private let afterField = PickerField()
    private var beforeOpen = false
    private var afterOpen = false
    private var subject: PickerModel.Drawer?
    private var contentHeight: CGFloat = 0
    private unowned let model: PickerModel

    override var isFlipped: Bool { true }

    init(model: PickerModel) {
        self.model = model
        super.init(frame: .zero)
        [rule, titleLabel, byline, warnings, peekFacts, showAsLabel, forms, capsule,
         beforeButton, afterButton, beforeField, afterField].forEach(addSubview)
        capsule.addSubview(locatorTypes)
        capsule.addSubview(locator)
        [titleLabel, byline, warnings, peekFacts].forEach {
            $0.maximumNumberOfLines = 0
            $0.cell?.wraps = true
        }
        [byline, peekFacts, warnings, showAsLabel].forEach { $0.textColor = .secondaryLabelColor }
        [beforeButton, afterButton].forEach { $0.surface = .link }
        beforeButton.text = "+ " + PickerWords.text(.before)
        afterButton.text = "+ " + PickerWords.text(.after)
        beforeButton.setAccessibilityLabel(PickerWords.text(.before))
        afterButton.setAccessibilityLabel(PickerWords.text(.after))
        forms.setAccessibilityLabel(PickerWords.text(.showAs))
        locatorTypes.setAccessibilityLabel(PickerWords.text(.locatorType))
        label(locator, .locatorValue)
        label(beforeField, .before)
        label(afterField, .after)
        locator.change = { [weak model] in model?.details.locator = $0 }
        beforeField.change = { [weak model] in model?.details.prefix = $0 }
        afterField.change = { [weak model] in model?.details.suffix = $0 }
        forms.change = { [weak model] index in
            guard let model, model.forms.indices.contains(index) else { return }
            model.details.form = model.forms[index].form
        }
        locatorTypes.change = { [weak model] index in
            guard let model, let options = model.catalog?.locators, options.indices.contains(index) else { return }
            model.details.term = options[index].term
        }
        beforeButton.perform = { [weak self] in self?.reveal(before: true) }
        afterButton.perform = { [weak self] in self?.reveal(before: false) }
    }

    required init?(coder: NSCoder) { nil }

    private func label(_ field: PickerField, _ word: PickerWord) {
        field.setAccessibilityLabel(PickerWords.text(word))
        field.placeholderString = PickerWords.text(word)
    }

    var keys: [NSView] {
        guard !isHidden, case .citation = subject else { return [] }
        return [locator, locatorTypes, forms,
                beforeOpen ? beforeField : beforeButton, afterOpen ? afterField : afterButton].filter { !$0.isHidden }
    }

    private func reveal(before: Bool) {
        if before { beforeOpen = true } else { afterOpen = true }
        superview?.needsLayout = true
        superview?.layoutSubtreeIfNeeded()
        window?.makeFirstResponder(before ? beforeField : afterField)
    }

    /// The drawer takes only its content's height; the list owns the space left above it.
    func arrange(width: CGFloat, metrics m: PickerMetrics) -> CGFloat {
        guard let drawer = model.drawer, let row = model.drawerRow else {
            subject = nil
            isHidden = true
            return 0
        }
        if subject != drawer {
            beforeOpen = false
            afterOpen = false
            subject = drawer
        }
        isHidden = false
        let isPeek: Bool
        if case .peek = drawer { isPeek = true } else { isPeek = false }
        rule.frame = NSRect(x: 0, y: 0, width: width, height: m.line)
        let available = max(0, width - m.column - m.inset)
        var y = m.line + m.pt(12)

        func pile(_ label: NSTextField, text: String, font: NSFont) {
            label.font = font
            label.stringValue = text
            label.isHidden = text.isEmpty
            guard !text.isEmpty else { return }
            let fitting = (text as NSString).boundingRect(with: NSSize(width: max(1, available - 4), height: 10_000),
                options: [.usesLineFragmentOrigin, .usesFontLeading], attributes: [.font: font])
            label.frame = NSRect(x: m.column, y: y, width: available, height: ceil(fitting.height) + m.pt(4))
            y += label.frame.height
        }

        pile(titleLabel, text: row.title ?? PickerWords.text(.untitled), font: m.font(13, .semibold))
        pile(byline, text: row.fullByline, font: m.font(12))
        pile(peekFacts, text: isPeek ? ([row.kindLabel, row.citationKey].compactMap { $0 }).joined(separator: " · ") : "",
             font: m.font(12))

        let controls: [NSView] = [showAsLabel, forms, capsule, beforeButton, afterButton, beforeField, afterField]
        controls.forEach { $0.isHidden = isPeek }
        if !isPeek {
            let details = model.details
            beforeOpen = beforeOpen || !details.prefix.isEmpty
            afterOpen = afterOpen || !details.suffix.isEmpty
            beforeButton.isHidden = beforeOpen
            beforeField.isHidden = !beforeOpen
            afterButton.isHidden = afterOpen
            afterField.isHidden = !afterOpen
            let font = m.font(13)
            [showAsLabel, forms, locatorTypes, locator, beforeField, afterField].forEach { $0.font = font }
            beforeButton.metrics = m
            afterButton.metrics = m
            locator.show(details.locator)
            beforeField.show(details.prefix)
            afterField.show(details.suffix)
            let options = model.forms
            let chosen = options.firstIndex(where: { $0.form == (details.form ?? .normal) }) ?? 0
            let examples = options.map { option in model.formExamples.first { $0.form == option.form }?.example }
            forms.isHidden = options.isEmpty
            showAsLabel.isHidden = options.isEmpty
            forms.setOptions(options.map(\.name), selected: chosen, examples: examples)
            let types = model.catalog?.locators ?? []
            let type = types.firstIndex(where: { $0.term == details.term }) ?? 0
            locatorTypes.setOptions(types.map(\.name), selected: type)
            let line = m.pt(28)
            let showWidth = options.isEmpty ? 0 : pickerWidth(showAsLabel.stringValue, font: font) + m.pt(6)
            let formWidth = options.indices.contains(chosen)
                ? min(m.pt(214), pickerWidth(options[chosen].name, font: font) + m.pt(24)) : 0
            let typeWidth = types.indices.contains(type)
                ? min(m.pt(125), pickerWidth(types[type].name, font: font) + m.pt(24)) : 0
            let gap = options.isEmpty ? 0 : m.pt(12)
            var x = m.column
            y += m.pt(3)
            pickerLine(showAsLabel, x: x, y: y, width: showWidth, height: line)
            x += showWidth
            forms.frame = NSRect(x: x, y: y, width: formWidth, height: line)
            x += formWidth + gap
            let locatorWidth = min(typeWidth + m.pt(120), max(typeWidth + m.pt(42), width - m.inset - x))
            capsule.radius = m.pt(6)
            capsule.frame = NSRect(x: x, y: y, width: locatorWidth, height: line)
            locatorTypes.frame = NSRect(x: m.pt(3), y: 0, width: typeWidth, height: line)
            pickerLine(locator, x: typeWidth + m.pt(5), y: 0, width: locatorWidth - typeWidth - m.pt(10), height: line)
            y += line

            let beforeWidth = beforeOpen ? min(available / 2, m.pt(180)) : beforeButton.preferredWidth
            beforeButton.frame = NSRect(x: m.column, y: y, width: beforeWidth, height: line)
            pickerLine(beforeField, x: m.column, y: y, width: beforeWidth, height: line)
            x = m.column + beforeWidth + m.pt(12)
            let afterWidth = afterOpen ? min(max(0, width - m.inset - x), m.pt(180)) : afterButton.preferredWidth
            afterButton.frame = NSRect(x: x, y: y, width: afterWidth, height: line)
            pickerLine(afterField, x: x, y: y, width: afterWidth, height: line)
            y += line
        }
        let entry = model.previewEntries.firstIndex { $0.row.id == row.id }.map(UInt64.init)
        let notes = model.preview?.warnings.filter { $0.item == entry && entry != nil }.map(\.text) ?? []
        pile(warnings, text: isPeek ? "" : notes.joined(separator: "\n"), font: m.font(12))
        contentHeight = y + m.pt(12)
        return contentHeight
    }
}
