import AppKit

struct PickerActionsPlacement: Equatable {
    let size: NSSize
    let edge: NSRectEdge

    init(size: NSSize, edge: NSRectEdge = .maxY) {
        self.size = size
        self.edge = edge
    }

    init(size: NSSize, anchor: NSRect, visibleFrame: NSRect, minimumWidth: CGFloat, minimumHeight: CGFloat) {
        let display = visibleFrame.insetBy(dx: 12, dy: 12)
        // Leave room between the control and the content for AppKit's popover arrow.
        let arrow: CGFloat = 16
        let above = display.maxY - anchor.maxY - arrow
        let below = anchor.minY - display.minY - arrow
        let left = anchor.minX - display.minX - arrow
        let right = display.maxX - anchor.maxX - arrow
        let verticalRoom = max(above, below)
        let horizontalRoom = max(left, right)

        if verticalRoom < minimumHeight, horizontalRoom >= minimumWidth {
            self.init(size: NSSize(width: min(size.width, horizontalRoom), height: max(1, min(size.height, display.height))),
                      edge: left >= right ? .minX : .maxX)
        } else {
            self.init(size: NSSize(width: max(1, min(size.width, display.width)), height: max(1, min(size.height, verticalRoom))),
                      edge: above >= below ? .maxY : .minY)
        }
    }
}

@MainActor
final class PickerActionsView: NSViewController, NSTableViewDataSource, NSTableViewDelegate {
    private enum Layout {
        static let inset: CGFloat = 12
        static let searchHeight: CGFloat = 28
        static let listGap: CGFloat = 4
        static let footerHeight: CGFloat = 38
        static let groupHeight: CGFloat = 24
        static let rowHeight: CGFloat = 30

        // Keep the search, both footer actions, one heading and four entire rows when space allows.
        static let minimumHeight = 2 * inset + searchHeight + 2 * listGap + 2 + footerHeight + groupHeight + 4 * rowHeight
    }

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
    private var viewportHeight: CGFloat = 0
    private var announcement: Task<Void, Never>?
    private var lastAnnounced: String?

    init(actions: [PickerAction], metrics: PickerMetrics) {
        self.actions = actions
        self.metrics = metrics
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) { nil }

    func placement(around anchor: NSRect, in visibleFrame: NSRect) -> PickerActionsPlacement {
        PickerActionsPlacement(size: contentSize, anchor: anchor, visibleFrame: visibleFrame,
                               minimumWidth: metrics.pt(240), minimumHeight: metrics.pt(Layout.minimumHeight))
    }

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
            cancelAnnouncement()
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
        let inset = metrics.pt(Layout.inset)
        NSLayoutConstraint.activate([
            search.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: inset),
            search.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -inset),
            search.topAnchor.constraint(equalTo: content.topAnchor, constant: inset),
            search.heightAnchor.constraint(equalToConstant: metrics.pt(Layout.searchHeight)),
            rule.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            rule.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            rule.topAnchor.constraint(equalTo: search.bottomAnchor, constant: inset),
            rule.heightAnchor.constraint(equalToConstant: metrics.line),
            scroll.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            scroll.topAnchor.constraint(equalTo: rule.bottomAnchor, constant: metrics.pt(Layout.listGap)),
            scroll.bottomAnchor.constraint(equalTo: footerRule.topAnchor, constant: -metrics.pt(Layout.listGap)),
            empty.leadingAnchor.constraint(equalTo: search.leadingAnchor),
            empty.trailingAnchor.constraint(equalTo: search.trailingAnchor),
            empty.centerYAnchor.constraint(equalTo: scroll.centerYAnchor),
            footerRule.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            footerRule.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            footerRule.heightAnchor.constraint(equalToConstant: metrics.line),
            footerRule.bottomAnchor.constraint(equalTo: content.bottomAnchor, constant: -metrics.pt(Layout.footerHeight)),
            back.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: inset),
            back.widthAnchor.constraint(equalToConstant: back.preferredWidth),
            back.heightAnchor.constraint(equalToConstant: metrics.pt(26)),
            back.bottomAnchor.constraint(equalTo: content.bottomAnchor, constant: -metrics.pt(6)),
            run.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -inset),
            run.leadingAnchor.constraint(greaterThanOrEqualTo: back.trailingAnchor, constant: metrics.pt(8)),
            run.heightAnchor.constraint(equalTo: back.heightAnchor),
            run.bottomAnchor.constraint(equalTo: back.bottomAnchor)
        ])
        view = content
        filter("")
    }

    override func viewDidLayout() {
        super.viewDidLayout()
        table.sizeLastColumnToFit()
        let height = scroll.contentView.bounds.height
        if height != viewportHeight {
            viewportHeight = height
            revealSelectionIfClipped()
        }
    }

    func focusSearch() {
        guard let window = view.window else { return }
        window.acceptsMouseMovedEvents = true
        window.makeFirstResponder(search)
        announceSelection(after: .milliseconds(300))
    }

    func cancelAnnouncement() {
        announcement?.cancel()
        lastAnnounced = nil
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
            return true
        default:
            return false
        }
        announceSelection(after: .milliseconds(100))
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
        let query = SearchQuery(query)
        guard !selectFirst || filteredQuery != query.normalized else { return }
        filteredQuery = query.normalized
        let matches = query.ranked(actions, title: \.title, aliases: \.aliases)
        var group: String?
        rows = matches.flatMap { action -> [Row] in
            guard !action.group.isEmpty, action.group != group else { return [.action(action)] }
            group = action.group
            return [.heading(action.group), .action(action)]
        }
        table.reloadData()
        scroll.isHidden = rows.isEmpty
        empty.isHidden = !rows.isEmpty
        let selected = rows.indices.first { index in
            guard let action = action(at: index) else { return false }
            return selectFirst || action == previous
        }
        // Start a fresh query at the heading above the first match. This also
        // avoids scrolling the first match into an as-yet unsized viewport.
        select(selected, reveal: !selectFirst)
        if selectFirst {
            table.rememberPointer()
            scroll.contentView.scroll(to: .zero)
            scroll.reflectScrolledClipView(scroll.contentView)
            revealSelectionIfClipped()
        }
        announceSelection(after: .milliseconds(300))
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

    private func revealSelectionIfClipped() {
        let row = table.selectedRow
        let visible = table.visibleRect
        guard row >= 0, visible.height > 0 else { return }
        let frame = table.rect(ofRow: row)
        guard frame.minY < visible.minY || frame.maxY > visible.maxY else { return }
        table.rememberPointer()
        table.scrollRowToVisible(row)
    }

    private func refreshRun() {
        let selected = action(at: table.selectedRow)
        run.isEnabled = selected != nil
        run.isHidden = selected == nil
        run.setAccessibilityHelp(selected.map { [PickerWords.text(.returnKey), spokenName($0)].joined(separator: ". ") })
    }

    private func spokenName(_ action: PickerAction) -> String {
        guard let checked = action.checked else { return action.title }
        return [action.title, PickerWords.text(checked ? .actionStateOn : .actionStateOff)].joined(separator: ". ")
    }

    private func announceSelection(after delay: Duration) {
        announcement?.cancel()
        let fallback = PickerWords.text(rows.isEmpty ? .noActions : .noActionSelected)
        let message = action(at: table.selectedRow).map(spokenName) ?? fallback
        guard message != lastAnnounced else { return }
        announcement = Task { [weak self] in
            try? await Task.sleep(for: delay)
            guard !Task.isCancelled, let self, let window = search.window, window.isVisible,
                  let responder = window.firstResponder,
                  responder === search || responder === search.currentEditor() else { return }
            lastAnnounced = message
            // The actual focus stays in Search so arrowing can be followed by typing.
            NSAccessibility.post(element: search, notification: .announcementRequested,
                                 userInfo: [.announcement: message, .priority: NSAccessibilityPriorityLevel.medium.rawValue])
        }
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
        metrics.pt(action(at: row) == nil ? Layout.groupHeight : Layout.rowHeight)
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        switch rows[row] {
        case let .heading(title):
            let label = pickerLabel(title)
            label.font = metrics.font(11, .semibold)
            label.textColor = .secondaryLabelColor
            label.setAccessibilityElement(true)
            label.setAccessibilityRole(.staticText)
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
        for label in [title, shortcut, check] {
            label.setAccessibilityHidden(true)
            addSubview(label)
        }
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
