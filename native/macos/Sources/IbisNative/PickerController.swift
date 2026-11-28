import AppKit
import Combine
import SwiftUI

/// The picker's shell state: focus, the open layer, the action list and every
/// command. The engine and `PickerModel` own the citation; this owns the window.
@MainActor
final class PickerController: ObservableObject {
    enum Layer: Equatable { case details(ItemRef), peek(ItemRef) }
    enum Focus: Equatable { case search, locator, token(ItemRef), actions }
    enum Anchor: Hashable { case format, library, more }

    enum EmptyState: Equatable {
        case newLibrary, noMatches, choose
        case missing(title: String, name: String)
        case unreadable(String)
    }

    private enum ReturnFocus {
        case search(NSRange)
        case locator(NSRange?)
        case token(ItemRef)
    }

    let model: PickerModel
    @Published var layer: Layer?
    @Published private(set) var actionsOpen = false
    @Published private(set) var actions: [PickerAction] = []
    @Published private(set) var highlighted: PickerAction?
    @Published var actionsQuery = "" {
        didSet { if actionsQuery != oldValue { refreshActions(newQuery: true) } }
    }
    @Published private(set) var numbers = false
    @Published private(set) var focus: Focus = .search
    @Published private(set) var isKey = false
    /// Changes when the keyboard moves the highlight, so only then the list scrolls.
    @Published private(set) var revealedRow = 0
    var visibleRows: [ItemRef] = []
    var anchors: [Anchor: CGRect] = [:]
    var choosingLibrary = false

    weak var panel: PickerPanel?
    weak var searchField: NSTextField?
    weak var locatorField: NSTextField?
    weak var actionsField: NSTextField?
    private var returnFocus: ReturnFocus?
    private var changes: AnyCancellable?
    private var announcedRevision = 0
    private var announcement: Task<Void, Never>?

    init(model: PickerModel) {
        self.model = model
        changes = model.objectWillChange.sink { [weak self] in
            DispatchQueue.main.async { self?.modelChanged() }
        }
    }

    private func modelChanged() {
        switch layer {
        case let .details(item) where model.editingCitation != item: layer = nil
        case let .peek(item) where item != model.selected: layer = nil
        default: break
        }
        if case let .token(item) = focus, !tokenOrder.contains(item) { focusSearch() }
        if actionsOpen { refreshActions(newQuery: false) }
        announceCount()
    }

    /// VoiceOver hears the result count once typing settles.
    private func announceCount() {
        guard model.searchRevision != announcedRevision else { return }
        announcedRevision = model.searchRevision
        announcement?.cancel()
        guard let label = model.searchReply?.countLabel else { return }
        announcement = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(500))
            guard !Task.isCancelled, let self, isKey, let field = searchField else { return }
            NSAccessibility.post(element: field, notification: .announcementRequested, userInfo: [
                .announcement: label, .priority: NSAccessibilityPriorityLevel.medium.rawValue,
            ])
        }
    }

    // MARK: State the view reads

    var hasOpenLayer: Bool { layer != nil || actionsOpen }
    /// With no draft, Return delivers the highlighted row, so the row takes the accent.
    var quickDelivery: Bool { !model.hasDraft && isKey && focus == .search && !hasOpenLayer }
    var shownRows: [PickerRow] { model.rows }

    var emptyState: EmptyState? {
        guard model.rows.isEmpty, !model.isOpening else { return nil }
        guard let catalog = model.catalog else { return model.notice == nil ? nil : .choose }
        guard let library = catalog.library else { return .choose }
        switch library.condition {
        case .missing: return .missing(title: catalog.status.first?.text ?? library.name, name: library.name)
        case .unreadable: return .unreadable(catalog.status.first?.text ?? library.name)
        case .ready: return model.searchReply?.total == nil ? .newLibrary : .noMatches
        }
    }

    /// One line above the list: a notice, a problem, or the section's name.
    var heading: (text: String, count: String?, status: PickerStatus?, isProblem: Bool)? {
        if model.isOpening { return (PickerWords.text(.opening), nil, nil, true) }
        if model.formatScope?.requiresBibliography == true { return (model.formatContext, nil, nil, true) }
        if let notice = model.notice { return (notice, nil, nil, true) }
        let status = model.libraryReady
            ? (model.preview?.problem ?? model.searchReply?.status.first ?? model.catalog?.status.first) : nil
        if let status { return (status.text, nil, status, true) }
        guard let first = model.sections.first else { return nil }
        return (first.kind.label, model.searchReply?.countLabel, nil, false)
    }

    var tokenOrder: [ItemRef] {
        var seen: [ItemRef] = []
        for part in CitationLine.parts(model) {
            if let item = part.item, !seen.contains(item) { seen.append(item) }
        }
        return seen
    }

    func number(for item: ItemRef) -> Int? {
        guard numbers, !hasOpenLayer, let index = visibleRows.prefix(9).firstIndex(of: item) else { return nil }
        return index + 1
    }

    // MARK: Focus

    func panelKeyChanged(_ key: Bool) { isKey = key }

    func fieldBegan(_ focus: Focus) {
        if self.focus != focus { self.focus = focus }
    }

    func focusSearch(caretAtEnd: Bool = false) {
        focus = .search
        searchField?.focus(caretAtEnd: caretAtEnd)
    }

    func focusLocator(_ item: ItemRef? = nil, restoring selection: NSRange? = nil) {
        layer = nil
        if let item { model.selectCitation(item) }
        if panel?.isKeyWindow == false { panel?.makeKey() }
        guard let field = locatorField, field.isEnabled else { return }
        focus = .locator
        field.focus(restoring: selection)
    }

    func focusToken(_ item: ItemRef) {
        guard tokenOrder.contains(item) else { return }
        model.selectCitation(item)
        focus = .token(item)
        // The window itself takes the keyboard; the panel routes token keys.
        panel?.makeFirstResponder(nil)
    }

    // MARK: Keys

    func showNumbers(_ show: Bool) { if numbers != show { numbers = show } }

    func selectNumber(_ number: Int) {
        guard !hasOpenLayer, visibleRows.indices.contains(number - 1) else { return }
        model.select(visibleRows[number - 1])
    }

    func move(_ step: Int) {
        model.move(step)
        revealedRow += 1
    }

    func tokenKey(_ key: NSEvent.SpecialKey?, shift: Bool) -> Bool {
        guard case let .token(token) = focus else { return false }
        let order = tokenOrder
        guard let index = order.firstIndex(of: token) else { return false }
        switch key {
        case .leftArrow, .rightArrow:
            let step = key == .leftArrow ? -1 : 1
            if shift { model.reorder(token, by: step, displayed: order) }
            else if order.indices.contains(index + step) { focusToken(order[index + step]) }
        case .delete, .deleteForward: removeToken(token)
        case .downArrow: showDetails(token)
        case .upArrow: focusSearch()
        default: return false
        }
        return true
    }

    func escape() -> Bool {
        if actionsOpen { closeActions(restoring: true); return true }
        if layer != nil { closeLayer(restoring: true); return true }
        if case .token = focus { focusSearch(); return true }
        let handled = model.escape()
        if handled { focusSearch() }
        return handled
    }

    // MARK: Layers

    func showDetails(_ id: ItemRef? = nil) {
        guard let item = id ?? model.citationItem else { return }
        if layer == .details(item) { closeLayer(restoring: true); return }
        closeActions(restoring: false)
        model.beginCitationEdit(item)
        guard model.editingCitation == item else { return }
        layer = .details(item)
    }

    func togglePeek() {
        guard let row = model.selection else { return }
        layer = layer == .peek(row.id) ? nil : .peek(row.id)
    }

    /// The popover closed itself, by Escape or a click elsewhere.
    func layerClosed() {
        let closing = layer
        layer = nil
        if case .details = closing { model.endCitationEdit() }
        DispatchQueue.main.async { [weak self] in
            guard let self, let panel, panel.pickerIsOpen, layer == nil else { return }
            if panel.isKeyWindow { focusSearch() }
            else if !choosingLibrary { panel.dismiss() }
        }
    }

    func closeLayer(restoring: Bool) {
        guard let closing = layer else { return }
        layer = nil
        if case let .details(item) = closing {
            model.endCitationEdit()
            if restoring, tokenOrder.contains(item) { focusToken(item); return }
        }
        if restoring { focusSearch() }
    }

    // MARK: Actions

    func toggleActions() { actionsOpen ? closeActions(restoring: true) : showActions() }

    func showActions() {
        layer = nil
        if let field = searchField, field.hasFocus, let editor = field.currentEditor() {
            returnFocus = .search(editor.selectedRange)
        } else if let field = locatorField, field.hasFocus {
            returnFocus = .locator(field.currentEditor()?.selectedRange)
        } else if case let .token(item) = focus {
            returnFocus = .token(item)
        } else {
            returnFocus = nil
        }
        actionsQuery = ""
        actionsOpen = true
        focus = .actions
        refreshActions(newQuery: true)
        DispatchQueue.main.async { [weak self] in self?.actionsField?.focus() }
    }

    func closeActions(restoring: Bool, toSearch: Bool = false) {
        guard actionsOpen else { return }
        actionsOpen = false
        let target = returnFocus
        returnFocus = nil
        guard restoring else { return }
        switch toSearch ? nil : target {
        case let .search(selection):
            focus = .search
            searchField?.focus(restoring: selection)
        case let .locator(selection): focusLocator(restoring: selection)
        case let .token(item) where tokenOrder.contains(item): focusToken(item)
        default: focusSearch()
        }
    }

    func refreshActions(newQuery: Bool) {
        let all = PickerCommand.allCases.compactMap { command -> PickerAction? in
            guard canRun(command), command != .remove || !membershipOwnsRemoval else { return nil }
            return describe(command)
        }
        let ranked = SearchQuery(actionsQuery).ranked(all, title: \.title, aliases: \.aliases)
        actions = ranked
        if newQuery || !(highlighted.map(ranked.contains) ?? false) {
            highlighted = newQuery ? ranked.first : nil
        }
    }

    func moveAction(_ step: Int) {
        guard !actions.isEmpty else { return }
        let current = highlighted.flatMap(actions.firstIndex) ?? (step > 0 ? -1 : actions.count)
        highlighted = actions[min(max(current + step, 0), actions.count - 1)]
    }

    func highlight(_ action: PickerAction) { highlighted = action }

    func runHighlighted() {
        guard let action = highlighted else { return }
        run(action)
    }

    func run(_ action: PickerAction) {
        guard describe(action.command) == action else { return }
        closeActions(restoring: true)
        runCommand(action.command)
    }

    /// Keys while the action list is open. Text editing keeps everything else.
    func actionKey(_ event: NSEvent) -> Bool {
        let flags = event.modifierFlags.intersection([.command, .option, .control, .shift])
        let key = event.charactersIgnoringModifiers?.lowercased()
        if (flags.isEmpty && key == "\u{1b}") || (flags == [.command] && key == "k") {
            if !event.isARepeat { closeActions(restoring: true) }
            return true
        }
        if flags.isEmpty || flags == [.shift], key == "\t" || key == "\u{19}" {
            closeActions(restoring: true, toSearch: true)
            return true
        }
        guard flags.isEmpty else { return false }
        switch event.specialKey {
        case .upArrow: moveAction(-1)
        case .downArrow: moveAction(1)
        case .carriageReturn, .enter: if !event.isARepeat { runHighlighted() }
        case .home where actionsQuery.isEmpty: highlighted = actions.first
        case .end where actionsQuery.isEmpty: highlighted = actions.last
        default: return false
        }
        return true
    }

    private var membershipOwnsRemoval: Bool {
        canRun(.membership) && model.isSelectionStaged && model.selected == model.citationItem
    }

    // MARK: Commands

    func canRun(_ command: PickerCommand) -> Bool {
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
        case .chooseLibrary: !model.isDelivering && !choosingLibrary
        case .largerText: model.textPercent < 200
        case .smallerText: model.textPercent > 100
        case .settings: true
        }
    }

    func describe(_ command: PickerCommand) -> PickerAction {
        switch command {
        case .typeCitation: return command.action(checked: model.shorthand)
        case .keepOrder: return command.action(checked: model.keepOrder)
        case .locateLibrary: return command.action(title: fixTitle(.locateLibrary))
        case .peek: return command.action(reference: model.selected)
        case .page, .details, .focusCitation: return command.action(reference: model.citationItem)
        case .membership:
            if let row = model.selection, model.isSelectionStaged {
                return command.action(title: PickerWords.text(.removeToken, row.title ?? PickerWords.text(.untitled)),
                                      reference: row.id)
            }
            return command.action(reference: model.selected)
        case .remove:
            if let row = model.citationRows.first(where: { $0.id == model.citationItem }) {
                return command.action(title: PickerWords.text(.removeToken, row.title ?? PickerWords.text(.untitled)),
                                      reference: row.id)
            }
            return command.action()
        default: return command.action()
        }
    }

    @discardableResult
    func runCommand(_ command: PickerCommand, repeating: Bool = false) -> Bool {
        guard canRun(command) else { return false }
        if repeating, command != .largerText, command != .smallerText { return true }
        switch command {
        case .format: openFormatMenu()
        case .peek: togglePeek()
        case .membership: if let row = model.selection { model.toggleCitation(row) }
        case .page: focusLocator()
        case .details: showDetails()
        case .focusCitation: if let item = model.citationItem { focusToken(item) }
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

    func removeToken(_ item: ItemRef) {
        model.remove(item)
        focusSearch()
    }

    func fixTitle(_ fix: PickerFix) -> String {
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

    func runFix(_ status: PickerStatus) {
        switch status.fix {
        case .chooseLibrary: panel?.chooseLibrary()
        case .locateLibrary: panel?.chooseLibrary(locate: true)
        case .reloadLibrary: model.reload()
        case .clearSearch: model.query = ""; focusSearch()
        case .chooseFormat: openFormatMenu()
        case .editLocator: focusLocator(status.item)
        case .editCitation: showDetails(status.item)
        case nil: break
        }
    }

    func emptyAction(primary: Bool) {
        switch emptyState {
        case .choose:
            if primary { panel?.chooseLibrary() } else { panel?.openSettings?() }
        case .unreadable: panel?.chooseLibrary()
        case .missing: panel?.chooseLibrary(locate: primary)
        case .noMatches: if primary { model.query = ""; focusSearch() }
        default: break
        }
    }

    /// Return resolves the source first when there is nothing to cite yet.
    func resolveLibraryIfNeeded() -> Bool {
        switch emptyState {
        case .choose, .missing, .unreadable:
            emptyAction(primary: true)
            return true
        default: return false
        }
    }

    // MARK: Menus (native NSMenu, opened at the control that owns them)

    func openFormatMenu() {
        guard model.canChooseFormat, let scope = model.formatScope, let formats = model.catalog?.formats else { return }
        let menu = NSMenu()
        menu.addItem(heading(model.formatContext))
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
            addFormats(formats, to: once, remembering: false)
            let item = NSMenuItem(title: PickerWords.text(.formatForOne), action: nil, keyEquivalent: "")
            item.submenu = once
            menu.addItem(item)
            menu.addItem(.separator())
            menu.addItem(heading(PickerWords.text(.formatRememberFor, owner)))
        } else {
            menu.addItem(.separator())
            menu.addItem(heading(PickerWords.text(.formatForOne)))
        }
        addFormats(formats, to: menu, remembering: remember)
        panel?.popUp(menu, at: anchors[.format])
        focusSearch()
    }

    private func addFormats(_ formats: [PickerFormat], to menu: NSMenu, remembering: Bool) {
        for styles in [false, true] {
            let group = formats.filter { $0.canKeepOrder == styles }
            guard !group.isEmpty else { continue }
            menu.addItem(heading(PickerWords.text(styles ? .formatStyleGroup : .formatKeyGroup)))
            for option in group {
                let item = PickerMenuItem(option.name) { [weak model] in
                    if remembering { model?.rememberFormat(option.id) } else { model?.useFormatOnce(option.id) }
                }
                let current = remembering ? !model.formatIsHeld : model.formatIsHeld || model.formatOwner == nil
                item.state = model.format == option.id && current ? .on : .off
                menu.addItem(item)
            }
        }
    }

    func openLibraryMenu() {
        let menu = NSMenu()
        menu.autoenablesItems = false
        if let name = model.catalog?.library?.name {
            menu.addItem(heading(name))
            for command in [PickerCommand.openSource, .locateLibrary, .reload] where canRun(command) {
                menu.addItem(menuItem(command))
            }
            menu.addItem(.separator())
        }
        menu.addItem(menuItem(.chooseLibrary))
        panel?.popUp(menu, at: anchors[.library])
        if panel?.pickerIsOpen == true, !choosingLibrary { focusSearch() }
    }

    func tokenMenu(_ item: ItemRef) -> NSMenu {
        let menu = NSMenu()
        menu.addItem(PickerMenuItem(PickerWords.text(.details)) { [weak self] in self?.showDetails(item) })
        let remove = PickerMenuItem(PickerWords.text(.remove)) { [weak self] in self?.removeToken(item) }
        menu.addItem(remove)
        return menu
    }

    private func heading(_ title: String) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        item.isEnabled = false
        return item
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

/// The citation exactly as Return delivers it, split into its references.
enum CitationLine {
    struct Part: Identifiable {
        let id: Int
        let text: AttributedString
        let plain: String
        let item: ItemRef?
        let row: PickerRow?
        let notes: [String]
    }

    @MainActor
    static func parts(_ model: PickerModel, scale: CGFloat = 1) -> [Part] {
        let draft = model.citationRows
        let entries = model.previewEntries
        if let preview = model.preview, !preview.display.nodes.isEmpty,
           Set(entries.map(\.row.id)) == Set(draft.map(\.id)) {
            return preview.display.nodes.enumerated().compactMap { index, node in
                let row = node.item.flatMap(Int.init(exactly:)).flatMap { entries.indices.contains($0) ? entries[$0].row : nil }
                let notes = preview.warnings.filter { node.item != nil && $0.item == node.item }.map(\.text)
                let plain = node.content.map(\.text).joined().replacingOccurrences(of: "\n", with: " ")
                guard !plain.isEmpty else { return nil }
                return Part(id: index, text: styled(node.content, scale: scale), plain: plain,
                            item: row?.id, row: row, notes: notes)
            }
        }
        // Keep an editable reference in place while a preview is refused or rebuilt.
        return draft.enumerated().map { index, row in
            let text = draft.count == 1 && !(model.preview?.text ?? "").isEmpty && entries.first?.row.id == row.id
                ? model.preview?.text ?? "" : row.title ?? PickerWords.text(.untitled)
            var attributed = AttributedString(text)
            attributed[AttributeScopes.SwiftUIAttributes.FontAttribute.self] = .system(size: 13 * scale)
            return Part(id: index, text: attributed, plain: text, item: row.id, row: row, notes: [])
        }
    }

    private static func styled(_ runs: [DisplayRun], scale: CGFloat) -> AttributedString {
        var text = AttributedString()
        for run in runs {
            let shift = run.script == .superscript ? 1 : (run.script == .`subscript` ? -1 : 0)
            var part = AttributedString(run.text.replacingOccurrences(of: "\n", with: " "))
            var font = Font.system(size: (shift == 0 ? 13 : 10) * scale, weight: run.bold == true ? .semibold : .regular)
            if run.italic == true { font = font.italic() }
            if run.smallCaps == true { font = font.lowercaseSmallCaps() }
            part[AttributeScopes.SwiftUIAttributes.FontAttribute.self] = font
            if shift != 0 {
                part[AttributeScopes.SwiftUIAttributes.BaselineOffsetAttribute.self] = CGFloat(shift * 3) * scale
            }
            if run.underline == true {
                part[AttributeScopes.SwiftUIAttributes.UnderlineStyleAttribute.self] = Text.LineStyle.single
            }
            text += part
        }
        return text
    }
}
