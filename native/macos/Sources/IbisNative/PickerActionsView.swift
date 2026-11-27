import AppKit

@MainActor
final class PickerActionsView: NSViewController, NSTableViewDataSource, NSTableViewDelegate {
    private enum Row {
        case heading(String)
        case action(PickerAction)
    }

    let search = PickerSearchField()
    var chosen: ((PickerAction) -> Void)?
    var cancelled: (() -> Void)?
    var contentSize: NSSize { NSSize(width: metrics.pt(352), height: metrics.pt(346)) }

    private var actions: [PickerAction]
    private let metrics: PickerMetrics
    private let scroll = NSScrollView()
    private let table = PickerActionsTable()
    private let run = PickerDeliveryButton()
    private let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("action"))
    private let empty = pickerLabel(PickerWords.text(.noActions))
    private var rows: [Row] = []
    private var filteredQuery: String?

    init(actions: [PickerAction], metrics: PickerMetrics) {
        self.actions = actions
        self.metrics = metrics
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) { nil }

    override func loadView() {
        let content = NSView(frame: NSRect(origin: .zero, size: contentSize))
        let rule = PickerRule()
        let footerRule = PickerRule()
        let back = PickerButton()
        let prompt = PickerWords.text(.searchActions)
        search.placeholderString = prompt
        search.setAccessibilityLabel(prompt)
        search.font = metrics.font(13)
        search.change = { [weak self] query in self?.filter(query) }

        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.scrollerStyle = .overlay
        scroll.automaticallyAdjustsContentInsets = false
        scroll.documentView = table
        table.headerView = nil
        table.style = .inset
        table.backgroundColor = .clear
        table.intercellSpacing = .zero
        table.allowsEmptySelection = true
        table.allowsColumnResizing = false
        table.columnAutoresizingStyle = .lastColumnOnlyAutoresizingStyle
        table.floatsGroupRows = false
        table.setAccessibilityLabel(PickerWords.text(.actionsTitle))
        table.pointed = { [weak self] row in
            guard let self, action(at: row) != nil, row != table.selectedRow else { return }
            select(row, reveal: false)
        }
        column.minWidth = 0
        table.addTableColumn(column)
        table.dataSource = self
        table.delegate = self
        table.target = self
        table.action = #selector(chooseClicked)
        let runLabel = PickerWords.text(.runAction)
        run.attributedTitle = NSAttributedString(string: runLabel + "  ↩", attributes: [.font: metrics.font(13, .medium)])
        run.setAccessibilityLabel(runLabel)
        run.setAccessibilityHelp(PickerWords.text(.returnKey))
        run.perform = { [weak self] in self?.chooseSelected() }
        back.metrics = metrics
        back.surface = .link
        back.text = PickerWords.text(.backFromActions)
        back.key = "⎋"
        back.setAccessibilityLabel(back.text)
        back.perform = { [weak self] in self?.cancelled?() }

        empty.alignment = .center
        empty.font = metrics.font(13)
        empty.textColor = .secondaryLabelColor
        empty.setAccessibilityElement(true)
        [search, rule, scroll, empty, footerRule, back, run].forEach {
            $0.translatesAutoresizingMaskIntoConstraints = false
            content.addSubview($0)
        }
        let inset = metrics.pt(12)
        NSLayoutConstraint.activate([
            search.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: inset),
            search.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -inset),
            search.topAnchor.constraint(equalTo: content.topAnchor, constant: inset),
            search.heightAnchor.constraint(equalToConstant: metrics.pt(28)),
            rule.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            rule.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            rule.topAnchor.constraint(equalTo: search.bottomAnchor, constant: inset),
            rule.heightAnchor.constraint(equalToConstant: metrics.line),
            scroll.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            scroll.topAnchor.constraint(equalTo: rule.bottomAnchor, constant: metrics.pt(4)),
            scroll.bottomAnchor.constraint(equalTo: footerRule.topAnchor, constant: -metrics.pt(4)),
            empty.leadingAnchor.constraint(equalTo: search.leadingAnchor),
            empty.trailingAnchor.constraint(equalTo: search.trailingAnchor),
            empty.centerYAnchor.constraint(equalTo: scroll.centerYAnchor),
            footerRule.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            footerRule.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            footerRule.heightAnchor.constraint(equalToConstant: metrics.line),
            footerRule.bottomAnchor.constraint(equalTo: content.bottomAnchor, constant: -metrics.pt(38)),
            back.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: inset),
            back.widthAnchor.constraint(equalToConstant: back.preferredWidth),
            back.heightAnchor.constraint(equalToConstant: metrics.pt(26)),
            back.bottomAnchor.constraint(equalTo: content.bottomAnchor, constant: -metrics.pt(6)),
            run.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -inset),
            run.heightAnchor.constraint(equalTo: back.heightAnchor),
            run.bottomAnchor.constraint(equalTo: back.bottomAnchor)
        ])
        view = content
        filter("")
    }

    override func viewDidLayout() {
        super.viewDidLayout()
        table.sizeLastColumnToFit()
    }

    func handleKey(_ event: NSEvent) -> Bool {
        guard event.type == .keyDown,
              event.modifierFlags.intersection([.command, .option, .control, .shift]).isEmpty,
              (search.currentEditor() as? NSTextInputClient)?.hasMarkedText() != true else { return false }
        let available = rows.indices.filter { action(at: $0) != nil }
        switch event.specialKey {
        case .upArrow, .downArrow:
            if let index = available.firstIndex(of: table.selectedRow) {
                let direction = event.specialKey == .upArrow ? -1 : 1
                select(available[min(max(index + direction, 0), available.count - 1)])
            } else {
                select(event.specialKey == .upArrow ? available.last : available.first)
            }
        case .home, .end:
            guard search.stringValue.isEmpty else { return false }
            select(event.specialKey == .home ? available.first : available.last)
        case .carriageReturn, .enter:
            if !event.isARepeat { chooseSelected() }
        default:
            return false
        }
        return true
    }

    func update(_ actions: [PickerAction]) {
        guard self.actions != actions else { return }
        let previous = action(at: table.selectedRow)
        self.actions = actions
        rebuild(query: search.stringValue, preserving: previous, selectFirst: false)
    }

    private func filter(_ query: String) {
        rebuild(query: query, preserving: nil, selectFirst: true)
    }

    private func rebuild(query: String, preserving previous: PickerAction?, selectFirst: Bool) {
        let words = folded(query).split(whereSeparator: \.isWhitespace)
        let normalized = words.joined(separator: " ")
        guard !selectFirst || filteredQuery != normalized else { return }
        filteredQuery = normalized
        let matches: [PickerAction]
        if words.isEmpty {
            matches = actions
        } else {
            matches = actions.compactMap { action -> (action: PickerAction, rank: Int)? in
                let title = folded(action.title)
                let searchable = title + " " + folded(action.aliases)
                guard words.allSatisfy({ searchable.contains($0) }) else { return nil }
                let rank: Int
                if title == normalized { rank = 0 }
                else if title.hasPrefix(normalized) { rank = 1 }
                else { rank = words.allSatisfy { title.contains($0) } ? 2 : 3 }
                return (action, rank)
            }.sorted { $0.rank < $1.rank }.map(\.action)
        }
        var group: String?
        rows = matches.flatMap { action -> [Row] in
            guard !action.group.isEmpty, action.group != group else { return [.action(action)] }
            group = action.group
            return [.heading(action.group), .action(action)]
        }
        table.reloadData()
        scroll.isHidden = rows.isEmpty
        empty.isHidden = !rows.isEmpty
        select(rows.indices.first { index in
            guard let action = action(at: index) else { return false }
            return selectFirst || action == previous
        })
    }

    private func folded(_ text: String) -> String {
        text.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: .current)
    }

    private func action(at row: Int) -> PickerAction? {
        guard rows.indices.contains(row), case let .action(action) = rows[row] else { return nil }
        return action
    }

    private func select(_ row: Int?, reveal: Bool = true) {
        if let row {
            table.selectRowIndexes(IndexSet(integer: row), byExtendingSelection: false)
            if reveal {
                table.rememberPointer()
                table.scrollRowToVisible(row)
            }
        } else {
            table.deselectAll(nil)
        }
        refreshRun()
    }

    private func refreshRun() {
        let canRun = action(at: table.selectedRow) != nil
        run.isEnabled = canRun
        run.isHidden = !canRun
    }

    @objc private func chooseClicked() {
        if let action = action(at: table.clickedRow) { chosen?(action) }
    }

    private func chooseSelected() {
        if let action = action(at: table.selectedRow) { chosen?(action) }
    }

    func tableViewSelectionDidChange(_ notification: Notification) {
        refreshRun()
    }

    func numberOfRows(in tableView: NSTableView) -> Int { rows.count }

    func tableView(_ tableView: NSTableView, shouldSelectRow row: Int) -> Bool { action(at: row) != nil }

    func tableView(_ tableView: NSTableView, isGroupRow row: Int) -> Bool { action(at: row) == nil }

    func tableView(_ tableView: NSTableView, heightOfRow row: Int) -> CGFloat {
        metrics.pt(action(at: row) == nil ? 24 : 30)
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        switch rows[row] {
        case let .heading(title):
            let label = pickerLabel(title)
            label.font = metrics.font(11, .semibold)
            label.textColor = .secondaryLabelColor
            label.setAccessibilityElement(true)
            let heading = NSView()
            label.translatesAutoresizingMaskIntoConstraints = false
            heading.addSubview(label)
            NSLayoutConstraint.activate([
                label.leadingAnchor.constraint(equalTo: heading.leadingAnchor, constant: metrics.pt(14)),
                label.trailingAnchor.constraint(equalTo: heading.trailingAnchor, constant: -metrics.pt(14)),
                label.centerYAnchor.constraint(equalTo: heading.centerYAnchor)
            ])
            return heading
        case let .action(action):
            return PickerActionCell(action, metrics: metrics) { [weak self] in
                self?.chosen?(action)
            }
        }
    }
}

@MainActor
private final class PickerActionsTable: NSTableView {
    var pointed: ((Int) -> Void)?
    private var tracking: NSTrackingArea?
    private var pointer: NSPoint?

    override func updateTrackingAreas() {
        if let tracking { removeTrackingArea(tracking) }
        let area = NSTrackingArea(rect: .zero, options: [.activeInKeyWindow, .inVisibleRect, .mouseMoved],
                                  owner: self, userInfo: nil)
        addTrackingArea(area)
        tracking = area
        super.updateTrackingAreas()
    }

    func rememberPointer() {
        pointer = window?.mouseLocationOutsideOfEventStream
    }

    override func mouseMoved(with event: NSEvent) {
        let point = event.locationInWindow
        guard pointer != point else { return }
        pointer = point
        pointed?(row(at: convert(point, from: nil)))
    }

    override func scrollWheel(with event: NSEvent) {
        pointer = event.locationInWindow
        super.scrollWheel(with: event)
    }
}

@MainActor
private final class PickerActionCell: NSTableCellView {
    private let title: NSTextField
    private let shortcut: NSTextField
    private let check = pickerLabel("✓")
    private let metrics: PickerMetrics
    private let perform: () -> Void

    init(_ action: PickerAction, metrics: PickerMetrics, perform: @escaping () -> Void) {
        self.metrics = metrics
        self.perform = perform
        title = pickerLabel(action.title)
        shortcut = pickerLabel(action.shortcut)
        super.init(frame: .zero)
        title.font = metrics.font(13)
        shortcut.font = metrics.font(12)
        shortcut.alignment = .right
        check.font = metrics.font(12, .medium)
        check.isHidden = action.checked != true
        [title, shortcut, check].forEach(addSubview)
        setAccessibilityElement(true)
        setAccessibilityRole(action.checked == nil ? .button : .checkBox)
        setAccessibilityLabel(action.title)
        let help = [action.group, action.shortcut].filter { !$0.isEmpty }.joined(separator: " · ")
        if !help.isEmpty { setAccessibilityHelp(help) }
        if let checked = action.checked { setAccessibilityValue(NSNumber(value: checked)) }
        updateForeground()
    }

    required init?(coder: NSCoder) { nil }

    override func accessibilityPerformPress() -> Bool {
        perform()
        return true
    }

    override var backgroundStyle: NSView.BackgroundStyle {
        didSet { updateForeground() }
    }

    private func updateForeground() {
        let selected = backgroundStyle == .emphasized
        title.textColor = selected ? .alternateSelectedControlTextColor : .labelColor
        shortcut.textColor = selected ? .alternateSelectedControlTextColor : .secondaryLabelColor
        check.textColor = shortcut.textColor
    }

    override func layout() {
        super.layout()
        let left = metrics.pt(30)
        let trailing = bounds.width - metrics.pt(14)
        let keyWidth = shortcut.stringValue.isEmpty ? 0
            : min(trailing / 3, pickerWidth(shortcut.stringValue, font: metrics.font(12)))
        let gap = keyWidth > 0 ? metrics.pt(8) : 0
        pickerLine(check, x: metrics.pt(10), y: 0, width: metrics.pt(17), height: bounds.height)
        let titleWidth = max(0, trailing - left - keyWidth - gap)
        pickerLine(title, x: left, y: 0, width: titleWidth, height: bounds.height)
        toolTip = pickerWidth(title.stringValue, font: metrics.font(13)) > titleWidth ? title.stringValue : nil
        pickerLine(shortcut, x: trailing - keyWidth, y: 0, width: keyWidth, height: bounds.height)
    }
}
