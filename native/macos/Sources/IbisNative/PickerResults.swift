import AppKit

private final class PickerTable: NSTableView {
    var hover: ((Int) -> Void)?
    var commandClick: ((Int) -> Void)?
    var restoreSearch: (() -> Void)?
    private var tracking: NSTrackingArea?

    override func updateTrackingAreas() {
        if let tracking { removeTrackingArea(tracking) }
        let area = NSTrackingArea(
            rect: .zero, options: [.activeInKeyWindow, .inVisibleRect, .mouseMoved, .mouseEnteredAndExited],
            owner: self, userInfo: nil
        )
        addTrackingArea(area)
        tracking = area
        super.updateTrackingAreas()
    }

    override func mouseMoved(with event: NSEvent) {
        hover?(row(at: convert(event.locationInWindow, from: nil)))
    }

    override func mouseExited(with event: NSEvent) { hover?(-1) }

    override func mouseDown(with event: NSEvent) {
        if event.modifierFlags.contains(.command) {
            commandClick?(row(at: convert(event.locationInWindow, from: nil)))
        } else {
            super.mouseDown(with: event)
        }
        restoreSearch?()
    }
}

private final class PickerTableRow: NSTableRowView {
    var hovered = false { didSet { needsDisplay = true } }
    var metrics = PickerMetrics(scale: 1, density: .compact)

    private func fill(_ color: NSColor) {
        let fill = bounds.insetBy(dx: metrics.pt(8), dy: metrics.pt(2))
        color.setFill()
        NSBezierPath(roundedRect: fill, xRadius: metrics.pt(8), yRadius: metrics.pt(8)).fill()
    }

    override func drawSelection(in dirtyRect: NSRect) {
        fill(isEmphasized ? .selectedContentBackgroundColor : .unemphasizedSelectedContentBackgroundColor)
    }
    override func drawBackground(in dirtyRect: NSRect) {
        if hovered && !isSelected { fill(.pickerQuaternaryFill) }
    }
}

private final class PickerRecordCell: NSTableCellView {
    private let icon = NSImageView()
    private let titleLabel = pickerLabel()
    private let byline = pickerLabel()
    private let date = pickerLabel()
    private let shortcutLabel = pickerLabel()
    private let record: PickerRow
    private let metrics: PickerMetrics

    init(record: PickerRow, metrics: PickerMetrics) {
        self.record = record
        self.metrics = metrics
        super.init(frame: .zero)
        icon.image = PickerArt.image(record.kind.rawValue)
        icon.imageScaling = .scaleProportionallyUpOrDown
        icon.contentTintColor = .secondaryLabelColor
        icon.setAccessibilityElement(false)
        titleLabel.stringValue = record.title ?? PickerWords.text(.untitled)
        titleLabel.font = metrics.font(13)
        titleLabel.textColor = .labelColor
        byline.font = metrics.font(12)
        date.font = .monospacedDigitSystemFont(ofSize: metrics.pt(12), weight: .regular)
        date.stringValue = record.year ?? ""
        date.alignment = .right
        shortcutLabel.font = metrics.font(11, .medium)
        shortcutLabel.alignment = .center
        [icon, titleLabel, byline, date, shortcutLabel].forEach(addSubview)
        updateForeground()
        setAccessibilityElement(true)
        setAccessibilityRole(.cell)
        toolTip = ([titleLabel.stringValue, record.fullByline, record.kindLabel] + [record.citationKey].compactMap { $0 })
            .joined(separator: "\n")
    }

    required init?(coder: NSCoder) { nil }

    override var backgroundStyle: NSView.BackgroundStyle {
        didSet { updateForeground() }
    }

    private func updateForeground() {
        let selected = backgroundStyle == .emphasized
        let text = selected ? NSColor.alternateSelectedControlTextColor : .labelColor
        titleLabel.textColor = text
        byline.textColor = selected ? text : .secondaryLabelColor
        date.textColor = byline.textColor
        shortcutLabel.textColor = byline.textColor
        icon.contentTintColor = selected ? text : .secondaryLabelColor
    }

    func describe(cited: Bool, shortcut: Int?) {
        shortcutLabel.stringValue = shortcut.map { "⌘\($0)" } ?? ""
        shortcutLabel.isHidden = shortcut == nil
        icon.isHidden = shortcut != nil
        setAccessibilityLabel(([titleLabel.stringValue, record.fullByline, record.kindLabel]
            + (cited ? [PickerWords.text(.cited)] : []) + [record.citationKey].compactMap { $0 }).joined(separator: ", "))
    }

    override func layout() {
        super.layout()
        let m = metrics
        let right = bounds.width - m.inset
        let iconSize = m.pt(18)
        let textWidth = max(0, right - m.column)
        let dateFont = date.font ?? m.font(12)
        let dateWidth = min(textWidth * 0.30, pickerWidth(date.stringValue, font: dateFont))
        let baseline = (bounds.height - m.pt(32)) / 2
        icon.frame = NSRect(x: m.tileCenter - iconSize / 2, y: (bounds.height - iconSize) / 2,
                            width: iconSize, height: iconSize)
        pickerLine(shortcutLabel, x: m.tileCenter - m.pt(17), y: 0, width: m.pt(34), height: bounds.height)
        pickerLine(titleLabel, x: m.column, y: baseline + m.pt(15),
                   width: textWidth - dateWidth - m.pt(12), height: m.pt(17))
        pickerLine(date, x: right - dateWidth, y: baseline + m.pt(15), width: dateWidth, height: m.pt(17))
        pickerLine(byline, x: m.column, y: baseline, width: textWidth, height: m.pt(15))
        if let font = byline.font {
            byline.stringValue = record.bylines.first { pickerWidth($0, font: font) <= byline.frame.width } ?? ""
        }
    }
}

@MainActor
final class PickerResults: NSView, NSTableViewDataSource, NSTableViewDelegate {
    private enum Cell: Hashable {
        case section(String)
        case record(PickerRow)
        case hint
    }

    let scroll = NSScrollView()
    private let table = PickerTable()
    private let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("reference"))
    private var cells: [Cell] = []
    private var metrics = PickerMetrics(scale: 1, density: .compact)
    private var shownCount = 0
    private var selected: ItemRef?
    private var cited: Set<ItemRef> = []
    private var numbers = false
    private var hovered = -1
    private var hintHeight: CGFloat = 0
    private var updatingSelection = false
    private var boundsObserver: NSObjectProtocol?
    var didSelect: ((ItemRef) -> Void)?
    var didAdd: ((PickerRow) -> Void)?
    var didDoubleClick: (() -> Void)?
    var restoreSearch: (() -> Void)?

    var keyView: NSView { table }
    var shown: [PickerRow] {
        cells.compactMap { cell in if case let .record(row) = cell { row } else { nil } }
    }

    func anchor(for item: ItemRef) -> NSView? {
        guard let index = cells.indices.first(where: { record(at: $0)?.id == item }) else { return nil }
        table.scrollRowToVisible(index)
        return table.view(atColumn: 0, row: index, makeIfNecessary: true)
    }

    override init(frame: NSRect) {
        super.init(frame: frame)
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.automaticallyAdjustsContentInsets = false
        scroll.contentView.postsBoundsChangedNotifications = true
        table.headerView = nil
        table.style = .plain
        table.intercellSpacing = .zero
        table.backgroundColor = .clear
        table.allowsEmptySelection = true
        table.focusRingType = .default
        table.allowsColumnResizing = false
        table.columnAutoresizingStyle = .lastColumnOnlyAutoresizingStyle
        column.minWidth = 0
        table.addTableColumn(column)
        table.delegate = self
        table.dataSource = self
        table.target = self
        table.doubleAction = #selector(doubleClick)
        table.setAccessibilityLabel(PickerWords.text(.references))
        table.hover = { [weak self] in self?.hover($0) }
        table.commandClick = { [weak self] index in
            guard let self, let record = self.record(at: index) else { return }
            didSelect?(record.id)
            didAdd?(record)
        }
        table.restoreSearch = { [weak self] in self?.restoreSearch?() }
        scroll.documentView = table
        addSubview(scroll)
        boundsObserver = NotificationCenter.default.addObserver(
            forName: NSView.boundsDidChangeNotification, object: scroll.contentView, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.hover(-1) }
        }
    }

    required init?(coder: NSCoder) { nil }
    deinit { if let boundsObserver { NotificationCenter.default.removeObserver(boundsObserver) } }

    override func layout() {
        super.layout()
        scroll.frame = bounds
        scroll.tile()
        column.width = scroll.contentSize.width
        table.sizeLastColumnToFit()
    }

    func configure(sections: [PickerSection], metrics: PickerMetrics, recent: Bool, selected: ItemRef?,
                   cited: Set<ItemRef>, numbers: Bool) {
        let oldHeight = self.metrics.row
        let oldScale = self.metrics.scale
        self.metrics = metrics
        self.cited = cited
        self.numbers = numbers
        scroll.scrollerInsets = NSEdgeInsets(top: metrics.pt(4), left: 0, bottom: metrics.pt(4), right: 0)
        var next: [Cell] = []
        var used: CGFloat = 0
        shownCount = 0
        for (index, section) in sections.enumerated() where !section.rows.isEmpty {
            if index > 0 {
                if recent && used + metrics.section + metrics.pt(8) + metrics.row > bounds.height { break }
                next.append(.section(section.kind.label))
                used += metrics.section + metrics.pt(8)
            }
            for record in section.rows {
                if recent && used + metrics.row > bounds.height { break }
                next.append(.record(record))
                shownCount += 1
                used += metrics.row
            }
        }
        let left = max(0, bounds.height - used)
        if recent, shownCount > 0, shownCount == sections.reduce(0, { $0 + $1.rows.count }),
           left >= metrics.pt(16) { next.append(.hint) }
        let oldHintHeight = hintHeight
        hintHeight = left
        let changed = cells != next
        let wantsScroll = self.selected != selected
        self.selected = selected
        if changed || oldHeight != metrics.row || oldScale != metrics.scale || oldHintHeight != hintHeight {
            cells = next
            updatingSelection = true
            table.rowHeight = metrics.row
            table.reloadData()
            updatingSelection = false
        }
        scroll.isHidden = next.isEmpty
        let index = cells.firstIndex { cell in
            if case let .record(row) = cell { return row.id == selected }
            return false
        }
        updatingSelection = true
        let indices = index.map { IndexSet(integer: $0) } ?? IndexSet()
        if indices != table.selectedRowIndexes { table.selectRowIndexes(indices, byExtendingSelection: false) }
        updatingSelection = false
        if let index, wantsScroll || changed { table.scrollRowToVisible(index) }
        if !cells.isEmpty {
            let visible = table.rows(in: table.visibleRect)
            if visible.location != NSNotFound {
                for row in visible.location..<(visible.location + visible.length) { refresh(row) }
            }
        }
    }

    private func record(at row: Int) -> PickerRow? {
        guard cells.indices.contains(row), case let .record(record) = cells[row] else { return nil }
        return record
    }

    private func hover(_ new: Int) {
        let next = record(at: new) == nil ? -1 : new
        guard next != hovered else { return }
        let old = hovered
        hovered = next
        refresh(old)
        refresh(next)
    }

    private func refresh(_ index: Int) {
        guard let row = record(at: index) else { return }
        (table.rowView(atRow: index, makeIfNecessary: false) as? PickerTableRow)?.hovered = index == hovered
        paint(table.view(atColumn: 0, row: index, makeIfNecessary: false) as? PickerRecordCell, record: row)
    }

    private func paint(_ cell: PickerRecordCell?, record: PickerRow) {
        let position = numbers ? shown.prefix(9).firstIndex { $0.id == record.id }.map { $0 + 1 } : nil
        cell?.describe(cited: cited.contains(record.id), shortcut: position)
    }

    func numberOfRows(in tableView: NSTableView) -> Int { cells.count }

    func tableView(_ tableView: NSTableView, heightOfRow row: Int) -> CGFloat {
        switch cells[row] {
        case .section: metrics.section + metrics.pt(8)
        case .record: metrics.row
        case .hint: hintHeight
        }
    }

    func tableView(_ tableView: NSTableView, rowViewForRow row: Int) -> NSTableRowView? {
        let view = PickerTableRow()
        view.metrics = metrics
        view.hovered = row == hovered
        return view
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        switch cells[row] {
        case let .record(record):
            let cell = PickerRecordCell(record: record, metrics: metrics)
            paint(cell, record: record)
            return cell
        case let .section(title):
            return PickerListLabel(title, metrics: metrics, centered: false)
        case .hint:
            return PickerListLabel(PickerWords.text(.resultHint), metrics: metrics, centered: true)
        }
    }

    func tableView(_ tableView: NSTableView, shouldSelectRow row: Int) -> Bool { record(at: row) != nil }

    func tableViewSelectionDidChange(_ notification: Notification) {
        if !updatingSelection, let record = record(at: table.selectedRow) { didSelect?(record.id) }
    }

    @objc private func doubleClick() {
        if record(at: table.clickedRow) != nil { didDoubleClick?() }
    }
}

private final class PickerListLabel: NSView {
    private let label: NSTextField
    private let metrics: PickerMetrics
    private let centered: Bool

    init(_ title: String, metrics: PickerMetrics, centered: Bool) {
        self.metrics = metrics
        self.centered = centered
        label = pickerLabel(title)
        super.init(frame: .zero)
        label.alignment = centered ? .center : .left
        label.setAccessibilityElement(true)
        label.font = metrics.font(centered ? 12 : 11, centered ? .regular : .semibold)
        label.textColor = centered ? .tertiaryLabelColor : .secondaryLabelColor
        addSubview(label)
    }

    required init?(coder: NSCoder) { nil }

    override func layout() {
        super.layout()
        pickerLine(label, x: metrics.inset, y: 0, width: bounds.width - 2 * metrics.inset,
                   height: centered ? bounds.height : metrics.section)
    }
}

extension PickerSectionKind {
    var label: String {
        let word: PickerWord = switch self {
        case .best: .best
        case .closest: .closest
        case .citedHere: .citedHere
        case .recentlyAdded: .recentlyAdded
        case .pasted: .pasted
        }
        return PickerWords.text(word)
    }
}
