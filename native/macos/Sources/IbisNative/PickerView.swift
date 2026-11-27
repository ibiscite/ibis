import AppKit
import SwiftUI

struct PickerView: NSViewRepresentable {
    @ObservedObject var model: PickerModel

    func makeNSView(context: Context) -> PickerCanvas { PickerCanvas(model: model) }
    func updateNSView(_ view: PickerCanvas, context: Context) { view.refresh() }
}

@MainActor
final class PickerCanvas: NSView {
    private enum ActionsFocus {
        case search(NSRange)
        case locator(NSRange?)
        case responder(NSResponder)
    }

    let search = PickerSearchField()
    private let header = PickerDragView()
    private let bird = NSImageView()
    private let format = PickerButton()
    private let mode = PickerCapsule()
    private let modeLabel = pickerLabel(PickerWords.text(.citation))
    private let headerRule = PickerRule()
    private let heading = pickerLabel()
    private let count = pickerLabel()
    private let source = PickerButton()
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
    private var actionsFocus: ActionsFocus?

    override var isFlipped: Bool { true }
    private var panel: PickerPanel? { window as? PickerPanel }
    var hasOpenPopover: Bool { popover.isOpen }
    var hasOpenActions: Bool { popover.subject == .actions }
    var locatorHasFocus: Bool { shelf.locatorHasFocus }
    private var metrics: PickerMetrics { PickerMetrics(scale: model.scale, density: model.density) }

    init(model: PickerModel) {
        self.model = model
        popover = PickerPopover(model: model)
        super.init(frame: .zero)
        [header, mode, search, format, headerRule, heading, count, source, spinner, statusGlyph, fixButton,
         results, empty, shelf].forEach(addSubview)
        header.addSubview(bird)
        mode.addSubview(modeLabel)
        bird.imageScaling = .scaleProportionallyUpOrDown
        bird.setAccessibilityElement(false)
        bird.unregisterDraggedTypes()
        search.setAccessibilityLabel(PickerWords.text(.search))
        search.change = { [weak model] in model?.query = $0 }
        format.surface = .link
        format.key = "⌄"
        format.setAccessibilityRole(.popUpButton)
        format.perform = { [weak self] in self?.openFormat() }
        heading.setAccessibilityElement(true)
        count.alignment = .right
        count.setAccessibilityElement(true)
        source.surface = .link
        source.captionSize = 11
        source.perform = { [weak self] in self?.libraryMenu() }
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
        shelf.membership.perform = { [weak self] in self?.toggleSelectionMembership() }
        shelf.action.perform = { [weak self] in self?.panel?.submit() }
        shelf.more.perform = { [weak self] in self?.showActions() }
        shelf.edit = { [weak self] id in self?.showDetails(id) }
        shelf.select = { [weak model] id in model?.selectCitation(id) }
        shelf.remove = { [weak self] id in self?.removeToken(id) }
        popover.routeKey = { [weak self] event in
            guard let self else { return false }
            return panel?.routeKey(event, view: self) ?? false
        }
        popover.closed = { [weak self] subject, restore in self?.popoverClosed(subject, restore: restore) }
        popover.actionChosen = { [weak self] action in
            guard let self, describe(action.command) == action else { return }
            runCommand(action.command)
        }
        popover.actionsTabBack = { [weak self] in self?.focusSearch() }
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
        shelf.isHidden = !model.libraryReady || model.isOpening
        let shelfHeight = shelf.isHidden ? 0 : shelf.height(hasDraft: model.hasDraft, metrics: m, width: width)
        let shelfY = height - shelfHeight
        results.frame = NSRect(x: 0, y: resultsY, width: width, height: max(0, shelfY - resultsY))
        results.configure(sections: model.sections, metrics: m, recent: model.searchReply?.total == nil,
                          selected: model.selected, cited: Set(model.citations.map { $0.row.id }),
                          numbers: numbers && !hasOpenPopover)
        empty.frame = results.frame
        empty.configure(state: emptyState, actionsOpen: hasOpenActions, metrics: m)
        shelf.frame = NSRect(x: 0, y: shelfY, width: width, height: shelfHeight)
        if !shelf.isHidden { shelf.configure(model: model, actionsOpen: hasOpenActions, metrics: m, width: width) }
        makeKeyLoop()
        if hasOpenActions { popover.updateActions(availableActions()) }
        popover.refresh(m)
    }

    private func arrangeHeader(width: CGFloat, metrics m: PickerMetrics) {
        header.frame = NSRect(x: 0, y: 0, width: width, height: m.header)
        let markSize = m.pt(32)
        bird.image = PickerArt.image(PickerArt.isDark(effectiveAppearance) ? "pickerMark-dark" : "pickerMark")
        bird.frame = NSRect(x: m.tileCenter - markSize / 2, y: (m.header - markSize) / 2, width: markSize, height: markSize)
        headerRule.frame = NSRect(x: 0, y: m.header, width: width, height: m.line)

        let name = model.formatName
        format.metrics = m
        format.maxCaption = width * 0.28
        format.isHidden = !model.libraryReady
        format.isEnabled = model.canChooseFormat
        format.text = name
        format.setAccessibilityLabel(PickerWords.text(.formatValue, name))
        format.setAccessibilityHelp(model.formatContext)
        format.toolTip = model.formatContext
        let buttonWidth = format.isHidden ? 0 : format.preferredWidth
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
        let status = model.isOpening || !model.libraryReady ? nil
            : (model.preview?.problem ?? model.searchReply?.status.first ?? model.catalog?.status.first)
        let notice = if model.isOpening { PickerWords.text(.opening) }
            else if model.formatScope?.requiresBibliography == true { model.formatContext }
            else { model.notice }
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
        let library = model.libraryReady && !model.isOpening ? model.catalog?.library : nil
        source.isHidden = library == nil
        source.metrics = m
        source.maxCaption = min(m.pt(145), width * 0.24)
        source.icon = "library"
        source.text = width >= m.pt(540) ? library?.name ?? "" : ""
        source.key = source.text.isEmpty ? "" : "⌄"
        source.toolTip = library?.name
        source.setAccessibilityLabel(library.map { PickerWords.text(.scopeLabel, $0.name) } ?? PickerWords.text(.library))
        let sourceWidth = source.isHidden ? 0 : source.preferredWidth + m.pt(6)
        let left = m.inset + (isStatus ? m.pt(20) : 0)
        let right = width - m.inset - countWidth - sourceWidth
        pickerLine(heading, x: left, y: y + m.pt(2), width: right - left - m.pt(6) - fixWidth, height: m.section)
        pickerLine(count, x: right, y: y + m.pt(2), width: countWidth, height: m.section)
        source.frame = NSRect(x: right + countWidth + (sourceWidth > 0 ? m.pt(6) : 0), y: y + m.pt(2),
                              width: max(0, sourceWidth - m.pt(6)), height: m.section)
        let glyph = NSRect(x: m.inset, y: y + m.pt(7), width: m.pt(14), height: m.pt(14))
        spinner.frame = glyph
        statusGlyph.frame = glyph
        fixButton.frame = NSRect(x: right - fixWidth - m.pt(4), y: y, width: fixWidth, height: m.section)
        return label != nil || library != nil
    }

    private var emptyState: PickerEmpty.State? {
        guard results.shown.isEmpty, !model.isOpening else { return nil }
        if let catalog = model.catalog {
            if let library = catalog.library {
                switch library.condition {
                case .missing:
                    return .missing(catalog.status.first?.text ?? library.name, library.name)
                case .unreadable: return .unreadable(catalog.status.first?.text ?? library.name)
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
        case .choose:
            if primary { panel?.chooseLibrary() } else { panel?.openSettings?() }
        case .unreadable: panel?.chooseLibrary()
        case .missing: panel?.chooseLibrary(locate: primary)
        case .noMatches:
            if primary { model.query = ""; focusSearch() }
        default: break
        }
    }

    func resolveLibraryIfNeeded() -> Bool {
        switch emptyState {
        case .choose, .missing, .unreadable:
            emptyAction(primary: true)
            return true
        default: return false
        }
    }

    private func makeKeyLoop() {
        let list = results.shown.isEmpty ? [] : [results.keyView]
        let options: [NSView] = empty.isHidden ? [] : [empty.primary, empty.secondary]
        let keyViews = [search, format, source] + list + options + (shelf.isHidden ? [] : shelf.keyViews)
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
        guard !hasOpenPopover, let reference = results.numberedReference(at: index) else { return }
        model.select(reference)
    }

    func showNumbers(_ show: Bool) {
        if numbers != show { numbers = show; refresh() }
    }

    func openFormat() {
        guard model.canChooseFormat, let scope = model.formatScope, let formats = model.catalog?.formats else { return }
        let menu = NSMenu()
        menu.addItem(formatHeading(model.formatContext))
        let remember = scope.project == nil && model.formatOwner != nil
        if model.formatIsHeld {
            let usual = formats.first { $0.id == scope.format }?.name ?? PickerWords.text(.formatDefault)
            menu.addItem(PickerMenuItem(PickerWords.text(.formatLibraryUsual, usual)) { [weak model] in
                model?.useFormatOnce(nil)
            })
        }
        if let owner = model.rememberedFormatOwner {
            let forget = PickerMenuItem(PickerWords.text(.formatForget, owner)) { [weak model] in model?.forgetFormat() }
            if let usual = formats.first(where: { $0.id == scope.usualFormat }) {
                forget.toolTip = PickerWords.text(.formatLibraryUsual, usual.name)
            }
            menu.addItem(forget)
        }
        if remember, let owner = model.formatOwner {
            let once = NSMenu()
            once.autoenablesItems = false
            addFormats(formats, to: once, remembering: false)
            let action = NSMenuItem(title: PickerWords.text(.formatForOne), action: nil, keyEquivalent: "")
            action.submenu = once
            menu.addItem(action)
            menu.addItem(.separator())
            menu.addItem(formatHeading(PickerWords.text(.formatRememberFor, owner)))
        } else {
            menu.addItem(.separator())
            menu.addItem(formatHeading(PickerWords.text(.formatForOne)))
        }
        addFormats(formats, to: menu, remembering: remember)
        open(menu, at: format)
    }

    private func addFormats(_ formats: [PickerFormat], to menu: NSMenu, remembering: Bool) {
        for styles in [false, true] {
            let group = formats.filter { $0.canKeepOrder == styles }
            guard !group.isEmpty else { continue }
            menu.addItem(formatHeading(PickerWords.text(styles ? .formatStyleGroup : .formatKeyGroup)))
            for option in group {
                let item = PickerMenuItem(option.name) { [weak model] in
                    if remembering { model?.rememberFormat(option.id) }
                    else { model?.useFormatOnce(option.id) }
                }
                item.state = model.format == option.id && (remembering ? !model.formatIsHeld : model.formatIsHeld || model.formatOwner == nil)
                    ? .on : .off
                menu.addItem(item)
            }
        }
    }

    private func formatHeading(_ name: String) -> NSMenuItem {
        let item = NSMenuItem(title: name, action: nil, keyEquivalent: "")
        item.isEnabled = false
        return item
    }

    func toggleSelectionMembership() {
        if let row = model.selection { model.toggleCitation(row) }
    }

    func focusTokens() { if let item = model.citationItem { _ = shelf.focus(item) } }

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

    func closePopover() {
        popover.close()
        actionsFocus = nil
    }

    private func popoverClosed(_ subject: PickerPopover.Subject, restore: Bool) {
        let focus = actionsFocus
        if subject == .actions {
            actionsFocus = nil
            needsLayout = true
        }
        guard let panel, panel.pickerIsOpen else { return }
        if restore {
            if !panel.isKeyWindow { panel.makeKey() }
            if case let .citation(item) = subject, shelf.focus(item) { return }
            if subject == .actions {
                switch focus {
                case let .search(selection):
                    focusSearch()
                    search.restoreSelection(selection)
                    return
                case let .locator(selection):
                    shelf.focusLocator(restoring: selection)
                    return
                case let .responder(responder) where panel.makeFirstResponder(responder):
                    return
                default: break
                }
            }
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

    private func libraryMenu() {
        open(libraryCommands(), at: source)
    }

    private func libraryCommands() -> NSMenu {
        let menu = NSMenu()
        menu.autoenablesItems = false
        if let name = model.catalog?.library?.name {
            let source = NSMenuItem(title: name, action: nil, keyEquivalent: "")
            source.isEnabled = false
            menu.addItem(source)
            for command in [PickerCommand.openSource, .locateLibrary, .reload] where canRun(command) {
                menu.addItem(menuItem(command))
            }
            menu.addItem(.separator())
        }
        menu.addItem(menuItem(.chooseLibrary))
        return menu
    }

    func showActions() {
        if hasOpenActions { popover.close(restoringFocus: true); return }
        if hasOpenPopover { popover.close(restoringFocus: true) }
        guard let window else { return }
        if let editor = search.currentEditor(), editor === window.firstResponder {
            actionsFocus = .search(editor.selectedRange)
        } else if shelf.locatorHasFocus {
            actionsFocus = .locator(shelf.locatorSelection)
        } else if let responder = window.firstResponder {
            actionsFocus = .responder(responder)
        }
        popover.showActions(availableActions(), at: shelf.isHidden ? search : shelf.more, metrics: metrics)
        needsLayout = true
    }

    func handleActionKey(_ event: NSEvent) -> Bool {
        popover.updateActions(availableActions())
        return popover.handleActionKey(event)
    }

    private func availableActions() -> [PickerAction] {
        let membershipOwnsRemoval = canRun(.membership) && model.isSelectionStaged && model.selected == model.citationItem
        return PickerCommand.allCases.compactMap { command in
            guard canRun(command), command != .remove || !membershipOwnsRemoval else { return nil }
            return describe(command)
        }
    }

    private func canRun(_ command: PickerCommand) -> Bool {
        switch command {
        case .format: model.canChooseFormat
        case .typeCitation, .openSource: model.libraryReady
        case .peek: model.selection != nil
        case .membership: model.canChangeSelectionMembership
        case .page, .details, .focusCitation: model.citationItem != nil
        case .remove: model.hasDraft && model.citationItem != nil
        case .keepOrder: model.canKeepOrder
        case .undo: model.canUndo
        case .locateLibrary: model.catalog?.library?.condition == .missing
        case .reload: model.catalog?.library != nil && !model.isOpening
        case .chooseLibrary: !model.isDelivering && panel?.choosingLibrary != true
        case .largerText: model.textPercent < 200
        case .smallerText: model.textPercent > 100
        case .settings: true
        }
    }

    private func describe(_ command: PickerCommand) -> PickerAction {
        switch command {
        case .typeCitation: command.action(checked: model.shorthand)
        case .keepOrder: command.action(checked: model.keepOrder)
        case .locateLibrary: command.action(title: fixTitle(.locateLibrary))
        case .peek: command.action(reference: model.selected)
        case .page, .details, .focusCitation: command.action(reference: model.citationItem)
        case .membership:
            if let row = model.selection, model.isSelectionStaged {
                command.action(title: PickerWords.text(.removeToken, row.title ?? PickerWords.text(.untitled)), reference: row.id)
            } else {
                command.action(reference: model.selected)
            }
        case .remove:
            if let row = model.citationRows.first(where: { $0.id == model.citationItem }) {
                command.action(title: PickerWords.text(.removeToken, row.title ?? PickerWords.text(.untitled)), reference: row.id)
            } else {
                command.action()
            }
        default: command.action()
        }
    }

    @discardableResult
    func runCommand(_ command: PickerCommand, repeating: Bool = false) -> Bool {
        guard canRun(command) else { return false }
        if repeating, command != .largerText, command != .smallerText { return true }
        switch command {
        case .format: openFormat()
        case .peek: togglePeek()
        case .membership: toggleSelectionMembership()
        case .page: focusLocator()
        case .details: showDetails()
        case .focusCitation: focusTokens()
        case .remove: if let item = model.citationItem { removeToken(item) }
        case .typeCitation: model.toggleShorthand(); focusSearch()
        case .keepOrder: model.toggleKeepOrder()
        case .undo: model.undoChange()
        case .openSource: panel?.openSource()
        case .locateLibrary: panel?.chooseLibrary(locate: true)
        case .chooseLibrary: panel?.chooseLibrary()
        case .reload: model.reload()
        case .largerText: model.changeSize(10)
        case .smallerText: model.changeSize(-10)
        case .settings: panel?.openSettings?()
        }
        return true
    }

    private func open(_ menu: NSMenu, at button: NSView) {
        menu.autoenablesItems = false
        activeMenu = menu
        menu.popUp(positioning: nil, at: NSPoint(x: button.bounds.minX, y: button.bounds.maxY), in: button)
        activeMenu = nil
    }

    private func menuItem(_ command: PickerCommand) -> NSMenuItem {
        let item = PickerMenuItem(describe(command).title) { [weak self] in self?.runCommand(command) }
        item.isEnabled = canRun(command)
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
        case unreadable(String)
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

    func configure(state: State?, actionsOpen: Bool, metrics m: PickerMetrics) {
        isHidden = state == nil
        guard let state else { return }
        let content: (title: String, hint: PickerWord, primary: String?, secondary: String?, key: String)
        switch state {
        case .newLibrary:
            content = (PickerWords.text(.emptyLibrary), .emptyLibraryHint, nil, nil, "")
        case .noMatches:
            content = (PickerWords.text(.noMatches), .noMatchesHint, PickerWords.text(.clear), nil, "")
        case .choose:
            content = (PickerWords.text(.chooseTitle), .chooseHint, PickerWords.text(.choose), PickerWords.text(.settings), "↩")
        case let .missing(title, name):
            content = (title, .missingHint, PickerWords.text(.findLibrary, name), PickerWords.text(.choose), "↩")
        case let .unreadable(problem):
            content = (problem, .chooseHint, PickerWords.text(.choose), nil, "↩")
        }
        titleLabel.stringValue = content.title
        titleLabel.toolTip = content.title
        hint.stringValue = PickerWords.text(content.hint)
        titleLabel.font = m.font(15, .semibold)
        hint.font = m.font(13)
        primary.metrics = m
        secondary.metrics = m
        primary.text = content.primary ?? ""
        secondary.text = content.secondary ?? ""
        primary.key = actionsOpen ? "" : content.key
        primary.isEnabled = !actionsOpen
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
