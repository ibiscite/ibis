import AppKit

/// Owns the transient window and the reference it is editing until it closes.
@MainActor
final class PickerPopover: NSObject, NSPopoverDelegate {
    enum Subject: Equatable {
        case citation(ItemRef)
        case peek(ItemRef)
        case actions
    }

    private let model: PickerModel
    private var popover: NSPopover?
    private var editor: CitationEditor?
    private var actionList: PickerActionsView?
    private var keyMonitor: Any?
    private(set) var subject: Subject?
    var routeKey: ((NSEvent) -> Bool)?
    var closed: ((Subject, Bool) -> Void)?
    var actionChosen: ((PickerAction) -> Void)?
    var actionsTabBack: (() -> Void)?

    init(model: PickerModel) {
        self.model = model
    }

    deinit {
        if let keyMonitor { NSEvent.removeMonitor(keyMonitor) }
    }

    var isOpen: Bool { popover != nil }

    func showCitation(_ item: ItemRef, at anchor: NSView, metrics: PickerMetrics) {
        close()
        model.beginCitationEdit(item)
        guard model.editingCitation == item else { return }
        let editor = CitationEditor(model: model, item: item)
        self.editor = editor
        let size = editor.configure(metrics)
        show(.citation(item), controller: editor, size: size, at: anchor)
        editor.view.window?.makeFirstResponder(editor.before)
    }

    func showPeek(_ row: PickerRow, at anchor: NSView, metrics m: PickerMetrics) {
        close()
        let title = row.title ?? PickerWords.text(.untitled)
        let facts = [row.kindLabel, row.citationKey].compactMap { $0 }.joined(separator: " · ")
        let stack = NSStackView()
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = m.pt(5)
        stack.edgeInsets = NSEdgeInsets(top: m.pt(12), left: m.pt(14), bottom: m.pt(12), right: m.pt(14))
        for (text, font) in [(title, m.font(13, .semibold)), (row.fullByline, m.font(12)), (facts, m.font(12))] {
            let label = NSTextField(wrappingLabelWithString: text)
            label.font = font
            label.textColor = text == title ? .labelColor : .secondaryLabelColor
            label.preferredMaxLayoutWidth = m.pt(306)
            stack.addArrangedSubview(label)
        }
        let controller = NSViewController()
        controller.view = stack
        show(.peek(row.id), controller: controller, size: stack.fittingSize, at: anchor)
    }

    func showActions(_ actions: [PickerAction], at anchor: NSView, metrics: PickerMetrics) {
        close()
        let controller = PickerActionsView(actions: actions, metrics: metrics)
        controller.chosen = { [weak self, weak controller] action in
            guard let self, let controller, actionList === controller else { return }
            close(restoringFocus: true)
            actionChosen?(action)
        }
        controller.cancelled = { [weak self, weak controller] in
            guard let self, let controller, actionList === controller else { return }
            close(restoringFocus: true)
        }
        actionList = controller
        var size = controller.contentSize
        if let visible = anchor.window?.screen?.visibleFrame {
            size.width = min(size.width, visible.width - 24)
            size.height = min(size.height, visible.height - 24)
        }
        show(.actions, controller: controller, size: size, at: anchor)
        controller.view.window?.acceptsMouseMovedEvents = true
        controller.view.window?.makeFirstResponder(controller.search)
    }

    func handleActionKey(_ event: NSEvent) -> Bool {
        guard subject == .actions, let actionList,
              (actionList.search.currentEditor() as? NSTextInputClient)?.hasMarkedText() != true else { return false }
        let flags = event.modifierFlags.intersection([.command, .option, .control, .shift])
        let key = event.charactersIgnoringModifiers?.lowercased()
        if (flags.isEmpty && key == "\u{1b}") || (flags == [.command] && key == "k") {
            if !event.isARepeat { close(restoringFocus: true) }
            return true
        }
        if (flags.isEmpty || flags == [.shift]), key == "\t" || key == "\u{19}" {
            if !event.isARepeat {
                close(restoringFocus: true)
                actionsTabBack?()
            }
            return true
        }
        return actionList.handleKey(event)
    }

    private func show(_ subject: Subject, controller: NSViewController, size: NSSize, at anchor: NSView) {
        let popover = NSPopover()
        popover.appearance = anchor.effectiveAppearance
        popover.behavior = .transient
        popover.animates = !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        popover.contentViewController = controller
        popover.contentSize = size
        popover.delegate = self
        self.subject = subject
        self.popover = popover
        popover.show(relativeTo: anchor.bounds, of: anchor, preferredEdge: .maxY)
        keyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self, weak popover] event in
            let handled = MainActor.assumeIsolated {
                guard let window = popover?.contentViewController?.view.window, window === event.window else {
                    return false
                }
                guard let self else { return false }
                if self.subject == .actions { return self.handleActionKey(event) }
                return self.routeKey?(event) == true
            }
            return handled ? nil : event
        }
    }

    func updateActions(_ actions: [PickerAction]) { actionList?.update(actions) }

    func refresh(_ metrics: PickerMetrics) {
        switch subject {
        case let .citation(item):
            guard model.editingCitation == item else { close(); return }
            if let size = editor?.configure(metrics), popover?.contentSize != size { popover?.contentSize = size }
        case let .peek(item):
            if item != model.selected { close() }
        case .actions, nil:
            break
        }
    }

    func close(restoringFocus: Bool = false) {
        guard let popover, let subject else { return }
        finish(subject)
        popover.close()
        if restoringFocus { closed?(subject, true) }
    }

    func popoverDidClose(_ notification: Notification) {
        guard let popover, popover === notification.object as? NSPopover, let subject else { return }
        finish(subject)
        closed?(subject, false)
    }

    private func finish(_ subject: Subject) {
        if let keyMonitor { NSEvent.removeMonitor(keyMonitor) }
        keyMonitor = nil
        popover = nil
        editor = nil
        actionList = nil
        self.subject = nil
        if case .citation = subject { model.endCitationEdit() }
    }
}

@MainActor
private final class CitationEditor: NSViewController {
    let before = PickerField()
    private let after = PickerField()
    private let form = PickerPopup()
    private let labels = [PickerWord.before, .after, .showAs].map { pickerLabel(PickerWords.text($0)) }
    private let model: PickerModel
    private let item: ItemRef
    private var forms: [PickerForm] = []

    init(model: PickerModel, item: ItemRef) {
        self.model = model
        self.item = item
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) { nil }

    override func loadView() {
        let content = NSView()
        let fields = [before, after]
        for (index, field) in fields.enumerated() {
            field.isBezeled = true
            field.bezelStyle = .roundedBezel
            field.drawsBackground = true
            field.setAccessibilityLabel(labels[index].stringValue)
        }
        before.placeholderString = PickerWords.text(.beforeExample)
        before.change = { [weak model, item] text in model?.changeDetails(for: item) { $0.prefix = text } }
        after.change = { [weak model, item] text in model?.changeDetails(for: item) { $0.suffix = text } }
        form.setAccessibilityLabel(PickerWords.text(.showAs))
        form.change = { [weak self] index in
            guard let self, forms.indices.contains(index) else { return }
            let selected = forms[index].form
            model.changeDetails(for: item) { $0.form = selected }
        }
        before.nextKeyView = after
        after.nextKeyView = form
        form.nextKeyView = before
        labels.forEach(content.addSubview)
        let controls: [NSView] = [before, after, form]
        controls.forEach(content.addSubview)
        view = content
    }

    func configure(_ m: PickerMetrics) -> NSSize {
        let content = view
        let details = model.details(for: item)
        before.show(details.prefix)
        after.show(details.suffix)
        form.font = m.font(13)
        let supported: [CitationForm] = [.normal, .narrative, .yearOnly]
        let selected = details.form ?? .normal
        let canReset = selected != .normal && model.preview?.problem?.fix == .editCitation
        let choices: [(PickerForm, String?)] = supported.compactMap { choice in
            guard let option = model.forms.first(where: { $0.form == choice }) else { return nil }
            let example = model.formExamples.first { $0.form == choice }
            if let text = example?.example, example?.reason == nil, !text.isEmpty { return (option, text) }
            return selected == choice || (canReset && choice == .normal) ? (option, nil) : nil
        }
        forms = choices.map(\.0)
        let index = forms.firstIndex(where: { $0.form == selected }) ?? 0
        form.setOptions(forms.map(\.name), selected: index, examples: choices.map(\.1))
        form.isEnabled = choices.contains { $0.1 != nil } || forms.count > 1

        let font = m.font(13)
        let pad = m.pt(12)
        let line = m.pt(24)
        let stride = m.pt(32)
        let width = m.pt(328)
        let height = pad * 2 + stride * 2 + line
        let labelWidth = labels.map { pickerWidth($0.stringValue, font: font) }.max() ?? 0
        let start = pad + labelWidth + m.pt(10)
        let controls: [NSControl] = [before, after, form]
        for (index, control) in controls.enumerated() {
            let y = height - pad - line - CGFloat(index) * stride
            labels[index].font = font
            labels[index].textColor = .secondaryLabelColor
            pickerLine(labels[index], x: pad, y: y, width: labelWidth, height: line)
            control.font = font
            control.frame = NSRect(x: start, y: y, width: width - start - pad, height: line)
        }
        content.frame.size = NSSize(width: width, height: height)
        return content.frame.size
    }
}
