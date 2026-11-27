import AppKit
import SwiftUI

struct PickerView: NSViewRepresentable {
    @ObservedObject var model: PickerModel

    func makeNSView(context: Context) -> PickerCanvas { PickerCanvas(model: model) }
    func updateNSView(_ view: PickerCanvas, context: Context) { view.refresh() }
}

@MainActor
final class PickerCanvas: NSView {
    let search = PickerSearchField()
    private let header = PickerDragView()
    private let bird = NSImageView()
    private let format = PickerPopup()
    private let mode = PickerCapsule()
    private let modeLabel = pickerLabel(PickerWords.text(.citation))
    private let headerRule = PickerRule()
    private let heading = pickerLabel()
    private let count = pickerLabel()
    private let spinner = NSProgressIndicator()
    private let statusGlyph = NSImageView()
    private let fixButton = PickerButton()
    private let results = PickerResults()
    private let empty = PickerEmpty()
    private let shelf = PickerShelf()
    private let popover: PickerPopover
    private let model: PickerModel
    private var statusAction: PickerStatus?
    private var numbers = false
    private var announcement: Task<Void, Never>?
    private var announcedRevision = 0
    private var announcedQuery = ""
    private var activeMenu: NSMenu?

    override var isFlipped: Bool { true }
    private var panel: PickerPanel? { window as? PickerPanel }
    var hasOpenPopover: Bool { popover.isOpen }
    var locatorHasFocus: Bool { shelf.locatorHasFocus }
    private var metrics: PickerMetrics { PickerMetrics(scale: model.scale, density: model.density) }

    init(model: PickerModel) {
        self.model = model
        popover = PickerPopover(model: model)
        super.init(frame: .zero)
        [header, mode, search, format, headerRule, heading, count, spinner, statusGlyph, fixButton,
         results, empty, shelf].forEach(addSubview)
        header.addSubview(bird)
        mode.addSubview(modeLabel)
        bird.imageScaling = .scaleProportionallyUpOrDown
        bird.setAccessibilityElement(false)
        bird.unregisterDraggedTypes()
        search.setAccessibilityLabel(PickerWords.text(.search))
        search.change = { [weak model] in model?.query = $0 }
        format.change = { [weak model] index in
            guard let model, let formats = model.catalog?.formats, formats.indices.contains(index) else { return }
            model.format = formats[index].id
        }
        heading.setAccessibilityElement(true)
        count.alignment = .right
        count.setAccessibilityElement(true)
        spinner.style = .spinning
        spinner.isDisplayedWhenStopped = false
        statusGlyph.imageScaling = .scaleProportionallyUpOrDown
        statusGlyph.contentTintColor = .secondaryLabelColor
        fixButton.surface = .link
        fixButton.perform = { [weak self] in self?.runFix(self?.statusAction) }
        empty.primary.perform = { [weak self] in self?.emptyAction(primary: true) }
        empty.secondary.perform = { [weak self] in self?.emptyAction(primary: false) }
        results.didSelect = { [weak model] in model?.select($0) }
        results.didAdd = { [weak model] in model?.toggleCitation($0) }
        results.didDoubleClick = { [weak self] in self?.panel?.submit(whenReady: true) }
        results.restoreSearch = { [weak self] in self?.focusSearch(caretAtEnd: true) }
        shelf.library.perform = { [weak self] in self?.libraryMenu() }
        shelf.membership.perform = { [weak self] in self?.toggleSelectionMembership() }
        shelf.action.perform = { [weak self] in self?.panel?.submit() }
        shelf.more.perform = { [weak self] in self?.moreMenu() }
        shelf.edit = { [weak self] id in self?.showDetails(id) }
        shelf.select = { [weak model] id in model?.selectCitation(id) }
        shelf.remove = { [weak self] id in self?.removeToken(id) }
        popover.routeKey = { [weak self] event in
            guard let self else { return false }
            return panel?.routeKey(event, view: self) ?? false
        }
        popover.closed = { [weak self] subject, restore in self?.popoverClosed(subject, restore: restore) }
    }

    required init?(coder: NSCoder) { nil }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        panel?.attach(self)
    }

    func refresh() {
        panel?.viewPreferencesChanged()
        if announcedQuery != model.query {
            announcement?.cancel()
            announcedQuery = model.query
        }
        if announcedRevision != model.searchRevision {
            announcedRevision = model.searchRevision
            announcement?.cancel()
            if let label = model.searchReply?.countLabel {
                announcement = Task { [weak self] in
                    try? await Task.sleep(for: .milliseconds(500))
                    guard !Task.isCancelled, let self, window?.isKeyWindow == true else { return }
                    NSAccessibility.post(element: search, notification: .announcementRequested,
                                         userInfo: [.announcement: label, .priority: NSAccessibilityPriorityLevel.medium.rawValue])
                }
            }
        }
        needsLayout = true
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        needsLayout = true
    }

    override func layout() {
        super.layout()
        let m = metrics
        let width = bounds.width
        let height = bounds.height
        guard width > 0, height > 0 else { return }
        arrangeHeader(width: width, metrics: m)
        let headingY = m.header + m.line
        let hasHeading = arrangeHeading(width: width, y: headingY, metrics: m)
        let resultsY = headingY + (hasHeading ? m.section : 0)
        let shelfHeight = shelf.height(hasDraft: model.hasDraft, metrics: m, width: width)
        let shelfY = height - shelfHeight
        results.frame = NSRect(x: 0, y: resultsY, width: width, height: max(0, shelfY - resultsY))
        results.configure(sections: model.sections, metrics: m, recent: model.searchReply?.total == nil,
                          selected: model.selected, cited: Set(model.citations.map { $0.row.id }), numbers: numbers)
        empty.frame = results.frame
        empty.configure(state: emptyState, metrics: m)
        shelf.frame = NSRect(x: 0, y: shelfY, width: width, height: shelfHeight)
        shelf.configure(model: model, metrics: m, width: width)
        makeKeyLoop()
        popover.refresh(m)
    }

    private func arrangeHeader(width: CGFloat, metrics m: PickerMetrics) {
        header.frame = NSRect(x: 0, y: 0, width: width, height: m.header)
        let markSize = m.pt(32)
        bird.image = PickerArt.image(PickerArt.isDark(effectiveAppearance) ? "pickerMark-dark" : "pickerMark")
        bird.frame = NSRect(x: m.tileCenter - markSize / 2, y: (m.header - markSize) / 2, width: markSize, height: markSize)
        headerRule.frame = NSRect(x: 0, y: m.header, width: width, height: m.line)

        let formats = model.catalog?.formats ?? []
        let selected = formats.firstIndex(where: { $0.id == model.format }) ?? 0
        let name = formats.indices.contains(selected) ? formats[selected].name : ""
        format.font = m.font(13)
        format.isHidden = !model.libraryReady
        if !format.isHidden { format.setOptions(formats.map(\.name), selected: selected) }
        format.setAccessibilityLabel(PickerWords.text(.formatValue, name))
        let buttonWidth = format.isHidden ? 0 : min(width * 0.33, pickerWidth(name, font: m.font(13)) + m.pt(24))
        let formatX = width - m.inset - buttonWidth
        format.frame = NSRect(x: formatX, y: m.pt(12), width: buttonWidth, height: m.pt(28))

        mode.isHidden = !model.shorthand
        mode.scarlet = true
        mode.radius = m.pt(6)
        modeLabel.font = m.font(13, .medium)
        modeLabel.textColor = PickerArt.accent(effectiveAppearance)
        let modeWidth = pickerWidth(modeLabel.stringValue, font: m.font(13, .medium)) + m.pt(16)
        mode.frame = NSRect(x: m.column, y: m.pt(14), width: modeWidth, height: m.pt(24))
        pickerLine(modeLabel, x: m.pt(8), y: 0, width: modeWidth - m.pt(16), height: mode.bounds.height)
        let left = m.column + (model.shorthand ? modeWidth + m.pt(8) : 0)
        search.font = m.font(16)
        search.placeholderString = PickerWords.text(model.shorthand ? .shorthandExample : .search)
        let searchHeight = m.pt(32)
        search.frame = NSRect(x: left, y: (m.header - searchHeight) / 2,
                              width: max(1, formatX - left - m.pt(12)), height: searchHeight)
        search.show(model.query)
    }

    private func arrangeHeading(width: CGFloat, y: CGFloat, metrics m: PickerMetrics) -> Bool {
        let libraryProblem = model.catalog?.library?.condition == .missing
        let status = model.isOpening || libraryProblem ? nil
            : (model.preview?.problem ?? model.searchReply?.status.first ?? model.catalog?.status.first)
        let notice = model.isOpening ? PickerWords.text(.opening) : model.notice
        let label = notice ?? status?.text ?? model.sections.first?.kind.label
        let countText = model.searchReply?.countLabel ?? ""
        let isStatus = notice != nil || status != nil
        heading.isHidden = label == nil
        heading.stringValue = label ?? ""
        count.isHidden = label == nil || countText.isEmpty
        count.stringValue = countText
        count.textColor = .secondaryLabelColor
        count.font = m.font(11)
        heading.font = m.font(11, .semibold)
        heading.textColor = .secondaryLabelColor
        heading.toolTip = isStatus ? label : nil
        statusGlyph.isHidden = !isStatus || model.isOpening
        statusGlyph.image = PickerArt.image(status?.kind == .citation ? "error" : "warning")
        spinner.isHidden = !model.isOpening
        if model.isOpening { spinner.startAnimation(nil) } else { spinner.stopAnimation(nil) }

        statusAction = notice == nil ? status : nil
        fixButton.metrics = m
        fixButton.isHidden = statusAction?.fix == nil
        fixButton.text = statusAction?.fix.map(fixTitle) ?? ""
        fixButton.setAccessibilityLabel(fixButton.text)
        let fixWidth = fixButton.isHidden ? 0 : fixButton.preferredWidth + m.pt(8)
        let countWidth = count.isHidden ? 0 : pickerWidth(countText, font: m.font(11))
        let left = m.inset + (isStatus ? m.pt(20) : 0)
        let right = width - m.inset - countWidth
        pickerLine(heading, x: left, y: y + m.pt(2), width: right - left - m.pt(6) - fixWidth, height: m.section)
        pickerLine(count, x: right, y: y + m.pt(2), width: countWidth, height: m.section)
        let glyph = NSRect(x: m.inset, y: y + m.pt(7), width: m.pt(14), height: m.pt(14))
        spinner.frame = glyph
        statusGlyph.frame = glyph
        fixButton.frame = NSRect(x: right - fixWidth - m.pt(4), y: y, width: fixWidth, height: m.section)
        return label != nil
    }

    private var emptyState: PickerEmpty.State? {
        guard results.shown.isEmpty, !model.isOpening else { return nil }
        if let catalog = model.catalog {
            if let library = catalog.library {
                switch library.condition {
                case .missing:
                    return .missing(catalog.status.first?.text ?? library.name, library.name)
                case .unreadable: return .choose
                case .ready: return model.searchReply?.total == nil ? .newLibrary : .noMatches
                }
            }
            return .choose
        }
        return model.notice == nil ? nil : .choose
    }

    private func fixTitle(_ fix: PickerFix) -> String {
        switch fix {
        case .chooseLibrary: PickerWords.text(.choose)
        case .locateLibrary: PickerWords.text(.findLibrary, model.catalog?.library?.name ?? "")
        case .reloadLibrary: PickerWords.text(.reload)
        case .clearSearch: PickerWords.text(.clear)
        case .chooseFormat: PickerWords.text(.format)
        case .editLocator: PickerWords.text(.page)
        case .editCitation: PickerWords.text(.details)
        }
    }

    private func runFix(_ status: PickerStatus?) {
        switch status?.fix {
        case .chooseLibrary: panel?.chooseLibrary()
        case .locateLibrary: panel?.chooseLibrary(locate: true)
        case .reloadLibrary: model.reload()
        case .clearSearch: model.query = ""; focusSearch()
        case .chooseFormat: openFormat()
        case .editLocator: focusLocator(status?.item)
        case .editCitation: showDetails(status?.item)
        case nil: break
        }
    }

    private func emptyAction(primary: Bool) {
        switch emptyState {
        case .choose: panel?.chooseLibrary()
        case .missing: panel?.chooseLibrary(locate: primary)
        case .noMatches:
            if primary { model.query = ""; focusSearch() } else { panel?.openZotero() }
        default: break
        }
    }

    private func makeKeyLoop() {
        let list = results.shown.isEmpty ? [] : [results.keyView]
        let options: [NSView] = empty.isHidden ? [] : [empty.primary, empty.secondary]
        let keyViews = [search, format] + list + options + shelf.keyViews
        let enabled = keyViews.filter { !$0.isHidden && (($0 as? NSControl)?.isEnabled ?? true) }
        for (index, view) in enabled.enumerated() {
            view.nextKeyView = enabled[(index + 1) % enabled.count]
        }
    }

    func focusSearch(caretAtEnd: Bool = false) {
        guard let window else { return }
        if search.currentEditor() !== window.firstResponder { window.makeFirstResponder(search) }
        if caretAtEnd {
            search.currentEditor()?.selectedRange = NSRange(location: model.query.utf16.count, length: 0)
        }
    }

    func move(_ step: Int) { model.move(step, within: results.shown) }

    func number(_ index: Int) {
        guard results.shown.indices.contains(index) else { return }
        model.select(results.shown[index].id)
    }

    func showNumbers(_ show: Bool) {
        if numbers != show { numbers = show; refresh() }
    }

    func openFormat() { if !format.isHidden { format.performClick(nil) } }

    func toggleSelectionMembership() {
        if let row = model.selection { model.toggleCitation(row) }
    }

    func focusTokens() { _ = shelf.focus() }

    func tokenKey(_ key: NSEvent.SpecialKey?, shift: Bool) -> Bool {
        guard let token = shelf.focused, let index = shelf.displayed.firstIndex(of: token) else { return false }
        switch key {
        case .leftArrow, .rightArrow:
            let step = key == .leftArrow ? -1 : 1
            if shift { model.reorder(token, by: step, displayed: shelf.displayed) }
            else if shelf.displayed.indices.contains(index + step) { _ = shelf.focus(shelf.displayed[index + step]) }
        case .delete, .deleteForward: removeToken(token)
        case .downArrow: showDetails(token)
        case .upArrow: focusSearch()
        default: return false
        }
        return true
    }

    private func removeToken(_ token: ItemRef) {
        model.remove(token)
        focusSearch()
    }

    func showDetails(_ id: ItemRef? = nil) {
        guard let item = id ?? shelf.focused ?? model.citationItem else { return }
        if popover.subject == .citation(item) {
            popover.close(restoringFocus: true)
            return
        }
        layoutSubtreeIfNeeded()
        guard let anchor = shelf.anchor(for: item) else { return }
        popover.showCitation(item, at: anchor, metrics: metrics)
    }

    func focusLocator(_ item: ItemRef? = nil) {
        closePopover()
        if let item {
            model.selectCitation(item)
            needsLayout = true
            layoutSubtreeIfNeeded()
        }
        if window?.isKeyWindow == false { window?.makeKey() }
        shelf.focusLocator()
    }

    func togglePeek() {
        guard let row = model.selection else { return }
        if popover.subject == .peek(row.id) {
            popover.close(restoringFocus: true)
        } else if let anchor = results.anchor(for: row.id) {
            popover.showPeek(row, at: anchor, metrics: metrics)
        }
    }

    func closePopover() { popover.close() }

    private func popoverClosed(_ subject: PickerPopover.Subject, restore: Bool) {
        guard let panel, panel.pickerIsOpen else { return }
        if restore {
            if !panel.isKeyWindow { panel.makeKey() }
            if case let .citation(item) = subject, shelf.focus(item) { return }
            focusSearch()
        } else {
            // AppKit returns key status after closing its transient window.
            DispatchQueue.main.async { [weak self, weak panel] in
                guard let panel, panel.pickerIsOpen, self?.hasOpenPopover == false,
                      !panel.isKeyWindow, !panel.choosingLibrary else { return }
                panel.dismiss()
            }
        }
    }

    func escape() -> Bool {
        if let menu = activeMenu { menu.cancelTracking(); return true }
        if hasOpenPopover { popover.close(restoringFocus: true); return true }
        let handled = model.escape()
        if handled { focusSearch() }
        return handled
    }

    func openSource() {
        if case .noMatches = emptyState { panel?.openZotero() }
        else { panel?.openSource() }
    }

    private func libraryMenu() {
        open(libraryCommands(), at: shelf.library)
    }

    private func libraryCommands() -> NSMenu {
        let menu = NSMenu()
        menu.autoenablesItems = false
        if let name = model.catalog?.library?.name {
            let source = NSMenuItem(title: name, action: nil, keyEquivalent: "")
            source.isEnabled = false
            menu.addItem(source)
            if model.libraryReady { menu.addItem(command(.openSource) { $0.panel?.openSource() }) }
            if model.catalog?.library?.condition == .missing {
                menu.addItem(PickerMenuItem(fixTitle(.locateLibrary)) { [weak self] in self?.panel?.chooseLibrary(locate: true) })
            }
            menu.addItem(command(.reload) { $0.model.reload() })
            menu.addItem(.separator())
        }
        menu.addItem(command(.choose) { $0.panel?.chooseLibrary() })
        return menu
    }

    func moreMenu() {
        if hasOpenPopover { popover.close(restoringFocus: true) }
        let menu = NSMenu()
        let type = command(.typeCitation, key: "t") { $0.model.toggleShorthand(); $0.focusSearch() }
        type.state = model.shorthand ? .on : .off
        menu.addItem(type)
        if !model.isSelectionStaged {
            menu.addItem(command(.add, key: "\r", modifiers: [.shift], enabled: model.canChangeSelectionMembership) {
                $0.toggleSelectionMembership()
            })
        }
        menu.addItem(command(.page, key: "l", enabled: model.citationItem != nil) { $0.focusLocator() })
        menu.addItem(command(.details, key: "i", enabled: model.citationItem != nil) { $0.showDetails() })
        menu.addItem(command(.remove, key: "\u{8}", modifiers: [], enabled: model.citationItem != nil) {
            if let item = $0.model.citationItem { $0.removeToken(item) }
        })
        menu.addItem(command(.peek, key: "y", enabled: model.selection != nil) { $0.togglePeek() })
        let order = command(.keepOrder, enabled: model.citations.count > 1) { $0.model.toggleKeepOrder() }
        order.state = model.keepOrder ? .on : .off
        menu.addItem(order)
        menu.addItem(command(.undo, key: "z", enabled: model.canUndo) { $0.model.undoChange() })
        menu.addItem(.separator())
        let library = NSMenuItem(title: PickerWords.text(.library), action: nil, keyEquivalent: "")
        library.submenu = libraryCommands()
        menu.addItem(library)
        menu.addItem(command(.format, key: "f", modifiers: [.shift, .command], enabled: model.libraryReady) { $0.openFormat() })
        menu.addItem(.separator())
        menu.addItem(command(.textLarger, key: "+", enabled: model.textPercent < 200) { $0.model.changeSize(10) })
        menu.addItem(command(.textSmaller, key: "-", enabled: model.textPercent > 100) { $0.model.changeSize(-10) })
        menu.addItem(command(.settings, key: ",") { $0.panel?.openSettings?() })
        open(menu, at: shelf.more)
    }

    private func open(_ menu: NSMenu, at button: NSView) {
        menu.autoenablesItems = false
        activeMenu = menu
        menu.popUp(positioning: nil, at: NSPoint(x: button.bounds.minX, y: button.bounds.maxY), in: button)
        activeMenu = nil
    }

    private func command(_ word: PickerWord, key: String = "", modifiers: NSEvent.ModifierFlags = [.command],
                         enabled: Bool = true, action: @escaping (PickerCanvas) -> Void) -> NSMenuItem {
        let item = PickerMenuItem(PickerWords.text(word), key: key) { [weak self] in
            if let self { action(self) }
        }
        item.keyEquivalentModifierMask = modifiers
        item.isEnabled = enabled
        return item
    }
}

@MainActor
final class PickerMenuItem: NSMenuItem {
    let perform: () -> Void

    init(_ title: String, key: String = "", perform: @escaping () -> Void) {
        self.perform = perform
        super.init(title: title, action: #selector(activate), keyEquivalent: key)
        target = self
    }

    required init(coder: NSCoder) { fatalError() }
    @objc private func activate() { perform() }
}

@MainActor
private final class PickerEmpty: NSView {
    enum State {
        case newLibrary
        case noMatches
        case choose
        case missing(String, String)
    }

    let primary = PickerButton()
    let secondary = PickerButton()
    private let titleLabel = pickerLabel()
    private let hint = pickerLabel()
    override var isFlipped: Bool { true }

    override init(frame: NSRect) {
        super.init(frame: frame)
        titleLabel.alignment = .center
        hint.alignment = .center
        hint.textColor = .secondaryLabelColor
        titleLabel.setAccessibilityElement(true)
        hint.setAccessibilityElement(true)
        secondary.surface = .link
        primary.surface = .primary
        primary.medium = true
        [titleLabel, hint, primary, secondary].forEach(addSubview)
    }

    required init?(coder: NSCoder) { nil }

    func configure(state: State?, metrics m: PickerMetrics) {
        isHidden = state == nil
        guard let state else { return }
        let content: (title: String, hint: PickerWord, primary: String?, secondary: String?, key: String, quietKey: String)
        switch state {
        case .newLibrary:
            content = (PickerWords.text(.emptyLibrary), .emptyLibraryHint, nil, nil, "", "")
        case .noMatches:
            content = (PickerWords.text(.noMatches), .noMatchesHint, PickerWords.text(.clear), PickerWords.text(.openZotero), "", "⌘O")
        case .choose:
            content = (PickerWords.text(.chooseTitle), .chooseHint, PickerWords.text(.choose), nil, "⇧⌘O", "")
        case let .missing(title, name):
            content = (title, .missingHint, PickerWords.text(.findLibrary, name), PickerWords.text(.choose), "", "")
        }
        titleLabel.stringValue = content.title
        hint.stringValue = PickerWords.text(content.hint)
        titleLabel.font = m.font(15, .semibold)
        hint.font = m.font(13)
        primary.metrics = m
        secondary.metrics = m
        primary.text = content.primary ?? ""
        secondary.text = content.secondary ?? ""
        primary.key = content.key
        secondary.key = content.quietKey
        primary.setAccessibilityLabel(primary.text)
        secondary.setAccessibilityLabel(secondary.text)
        primary.isHidden = content.primary == nil
        secondary.isHidden = content.secondary == nil
        let totalHeight = m.pt(primary.isHidden ? 47 : 85)
        // Centre the complete action group within the list's available area.
        let y = max(0, (bounds.height - totalHeight) / 2)
        let width = bounds.width - m.inset * 2
        pickerLine(titleLabel, x: m.inset, y: y, width: width, height: m.pt(24))
        pickerLine(hint, x: m.inset, y: y + m.pt(25), width: width, height: m.pt(22))
        let firstWidth = primary.isHidden ? 0 : primary.preferredWidth
        let secondWidth = secondary.isHidden ? 0 : secondary.preferredWidth
        let gap = secondary.isHidden ? 0 : m.pt(12)
        let x = (bounds.width - firstWidth - gap - secondWidth) / 2
        primary.frame = NSRect(x: x, y: y + m.pt(55), width: firstWidth, height: m.pt(28))
        secondary.frame = NSRect(x: x + firstWidth + gap, y: y + m.pt(55), width: secondWidth, height: m.pt(28))
    }
}
