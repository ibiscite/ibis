import AppKit
import Combine
import SwiftUI

/// The picker's shell state: the highlighted added reference, the reference
/// open for editing, Peek and every command. The engine and `PickerModel` own
/// the citation; this owns the window.
@MainActor
final class PickerController: ObservableObject {
    enum Anchor: Hashable { case format, actions }

    enum EmptyState: Equatable {
        case newLibrary, noMatches, choose
        case missing(title: String, name: String)
        case unreadable(String)
    }

    let model: PickerModel
    /// The reference Peek shows beside its row.
    @Published var peek: ItemRef?
    /// The added reference the keyboard is on, in the "In this citation" section.
    @Published private(set) var pinned: ItemRef?
    /// The reference whose page, prefix and suffix are open in its row.
    @Published private(set) var editing: ItemRef?
    @Published private(set) var numbers = false
    @Published private(set) var isKey = false
    /// Changes when the keyboard moves the highlight, so only then the list scrolls.
    @Published private(set) var revealedRow = 0
    var visibleRows: [ItemRef] = []
    var anchors: [Anchor: CGRect] = [:]
    var choosingLibrary = false

    weak var panel: PickerPanel?
    weak var searchField: NSTextField?
    weak var locatorField: NSTextField?
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
        if let peek, peek != model.selected { self.peek = nil }
        if let pinned, !addedOrder.contains(pinned) { self.pinned = nil }
        if let editing, model.editingCitation != editing { self.editing = nil }
        announceCount()
        panel?.fitHeight()
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

    var addedOrder: [ItemRef] { model.citations.map(\.row.id) }
    /// The highlighted result takes the accent while it is what the keyboard acts on.
    var resultEmphasis: Bool { isKey && pinned == nil && editing == nil }
    var showsList: Bool { !model.rows.isEmpty || model.hasDraft }
    var showsFooter: Bool { model.libraryReady && !model.isOpening && showsList }

    var emptyState: EmptyState? {
        guard model.rows.isEmpty, !model.hasDraft, !model.isOpening else { return nil }
        guard let catalog = model.catalog else { return model.notice == nil ? nil : .choose }
        guard let library = catalog.library else { return .choose }
        switch library.condition {
        case .missing: return .missing(title: catalog.status.first?.text ?? library.name, name: library.name)
        case .unreadable: return .unreadable(catalog.status.first?.text ?? library.name)
        case .ready: return model.searchReply?.total == nil ? .newLibrary : .noMatches
        }
    }

    /// A problem or notice, shown on one line above the footer only when there is one.
    var problem: (text: String, status: PickerStatus?)? {
        if model.formatScope?.requiresBibliography == true { return (model.formatContext, nil) }
        if let notice = model.notice { return (notice, nil) }
        guard model.libraryReady, emptyState == nil else { return nil }
        let status = model.preview?.problem ?? model.searchReply?.status.first ?? model.catalog?.status.first
        return status.map { ($0.text, $0) }
    }

    /// The panel's height follows its content, as Spotlight's does.
    func height(_ grid: PickerGrid) -> CGFloat {
        var height = grid.header
        if emptyState != nil { height += 1 + grid.empty }
        else if showsList { height += 1 + listHeight(grid) }
        if problem != nil { height += grid.status }
        if showsFooter { height += 1 + grid.footer }
        return ceil(height)
    }

    func listHeight(_ grid: PickerGrid) -> CGFloat {
        let rows = CGFloat(model.rows.count + model.citations.count)
        let headings = CGFloat(model.sections.count + (model.hasDraft ? 1 : 0))
        let content = rows * grid.row(model.density) + headings * grid.heading + (editing == nil ? 0 : grid.editor)
            + 12 * grid.s
        return min(content, grid.row(model.density) * 8 + grid.heading * 2 + 12 * grid.s)
    }

    func number(for item: ItemRef) -> Int? {
        guard numbers, editing == nil, let index = visibleRows.prefix(9).firstIndex(of: item) else { return nil }
        return index + 1
    }

    func locatorLabel(for item: ItemRef) -> String? {
        let details = model.details(for: item)
        let locator = details.locator.trimmingCharacters(in: .whitespaces)
        guard !locator.isEmpty else { return nil }
        let term = model.catalog?.locators.first { $0.term == details.term }?.shortName ?? PickerWords.text(.pageShort)
        return "\(term) \(locator)"
    }

    // MARK: Focus

    func panelKeyChanged(_ key: Bool) { isKey = key }

    func focusSearch(caretAtEnd: Bool = false) {
        searchField?.focus(caretAtEnd: caretAtEnd)
    }

    func reset() {
        peek = nil
        pinned = nil
        if editing != nil { model.endCitationEdit() }
        editing = nil
        showNumbers(false)
    }

    // MARK: Keys

    func showNumbers(_ show: Bool) { if numbers != show { numbers = show } }

    func selectNumber(_ number: Int) {
        guard editing == nil, visibleRows.indices.contains(number - 1) else { return }
        pinned = nil
        model.select(visibleRows[number - 1])
    }

    /// Up and Down walk one list: the added references first, then the results.
    func move(_ step: Int) {
        let added = addedOrder
        if let pinned, let index = added.firstIndex(of: pinned) {
            let next = index + step
            if added.indices.contains(next) { self.pinned = added[next] }
            else if next >= added.count, !model.rows.isEmpty {
                self.pinned = nil
                if let first = model.rows.first { model.select(first.id) }
            }
        } else if step < 0, !added.isEmpty, model.rows.isEmpty || model.selected == model.rows.first?.id {
            pinned = added.last
        } else {
            model.move(step)
        }
        revealedRow += 1
    }

    /// Delete in an empty field reaches the last added reference, then removes it.
    func deleteInEmptyField() -> Bool {
        if let pinned {
            let added = addedOrder
            let index = added.firstIndex(of: pinned) ?? 0
            model.remove(pinned)
            let rest = addedOrder
            self.pinned = rest.isEmpty ? nil : rest[max(0, min(index, rest.count) - 1)]
            return true
        }
        guard let last = addedOrder.last else { return false }
        pinned = last
        return true
    }

    func reorderPinned(_ step: Int) -> Bool {
        guard let pinned else { return false }
        model.reorder(pinned, by: step, displayed: addedOrder)
        return true
    }

    /// Tab adds the highlighted result and clears the field for the next search.
    func addSelection() {
        guard editing == nil, let row = model.selection, model.canChangeSelectionMembership else { return }
        if !model.isSelectionStaged { model.toggleCitation(row) }
        pinned = nil
        model.query = ""
        focusSearch()
    }

    func returnKey() {
        if editing != nil { closeEditing(); return }
        if let pinned { edit(pinned); return }
        panel?.submit(whenReady: true)
    }

    func escape() -> Bool {
        if editing != nil { closeEditing(); return true }
        if peek != nil { peek = nil; return true }
        if pinned != nil { pinned = nil; return true }
        let handled = model.escape()
        if handled { focusSearch() }
        return handled
    }

    // MARK: Editing in place

    func edit(_ id: ItemRef? = nil) {
        guard let item = id ?? pinned ?? model.citationItem else { return }
        if editing == item { closeEditing(); return }
        peek = nil
        model.beginCitationEdit(item)
        guard model.editingCitation == item else { return }
        editing = item
        pinned = model.hasDraft ? item : nil
        DispatchQueue.main.async { [weak self] in self?.locatorField?.focus(caretAtEnd: true) }
    }

    func closeEditing() {
        guard editing != nil else { return }
        model.endCitationEdit()
        editing = nil
        focusSearch(caretAtEnd: true)
    }

    func togglePeek(_ id: ItemRef? = nil) {
        guard let item = id ?? model.selected else { return }
        if id != nil { model.select(item) }
        peek = peek == item ? nil : item
    }

    /// The popover closed itself, by Escape or a click elsewhere.
    func peekClosed() {
        peek = nil
        DispatchQueue.main.async { [weak self] in
            guard let self, let panel, panel.pickerIsOpen, peek == nil else { return }
            if panel.isKeyWindow { focusSearch(caretAtEnd: true) }
            else if !choosingLibrary { panel.dismiss() }
        }
    }

    // MARK: Commands

    private var membershipOwnsRemoval: Bool {
        canRun(.membership) && model.isSelectionStaged && model.selected == model.citationItem
    }

    func canRun(_ command: PickerCommand) -> Bool {
        switch command {
        case .format: model.canChooseFormat
        case .typeCitation, .openSource: model.libraryReady
        case .peek: model.selection != nil
        case .membership: model.canChangeSelectionMembership
        case .page, .details: model.citationItem != nil
        case .focusCitation: model.hasDraft
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
            if let row = model.citationRows.first(where: { $0.id == (pinned ?? model.citationItem) }) {
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
        case .page, .details: edit()
        case .focusCitation: pinned = pinned == nil ? (model.citationItem ?? addedOrder.last) : nil
        case .remove: if let item = pinned ?? model.citationItem { model.remove(item); pinned = nil }
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

    func remove(_ item: ItemRef) {
        model.remove(item)
        if pinned == item { pinned = nil }
        focusSearch(caretAtEnd: true)
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
        case .editLocator, .editCitation: edit(status.item)
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

    /// ⌘K: every command that can run now, grouped, with its key equivalent.
    func openActionsMenu() {
        let menu = NSMenu()
        var group: String?
        for command in PickerCommand.allCases where canRun(command) {
            guard command != .remove || !membershipOwnsRemoval else { continue }
            let action = describe(command)
            if action.group != group {
                if group != nil { menu.addItem(.separator()) }
                menu.addItem(heading(action.group))
                group = action.group
            }
            let item = PickerMenuItem(action.title) { [weak self] in self?.runCommand(command) }
            if let key = command.menuKey {
                item.keyEquivalent = key.key
                item.keyEquivalentModifierMask = key.modifiers
            }
            if let checked = action.checked { item.state = checked ? .on : .off }
            menu.addItem(item)
        }
        panel?.popUp(menu, at: anchors[.actions])
        if panel?.pickerIsOpen == true, !choosingLibrary { focusSearch(caretAtEnd: true) }
    }

    func openFormatMenu() {
        guard model.canChooseFormat, let scope = model.formatScope, let formats = model.catalog?.formats else { return }
        let menu = NSMenu()
        if !model.formatContext.isEmpty { menu.addItem(heading(model.formatContext)) }
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
        if panel?.pickerIsOpen == true { focusSearch(caretAtEnd: true) }
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

    private func heading(_ title: String) -> NSMenuItem { .sectionHeader(title: title) }
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
