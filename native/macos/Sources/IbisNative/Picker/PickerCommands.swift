import AppKit

/// Every picker command, with its key. ⌘K lists those that can run now,
/// grouped, as a native menu; the same keys work without opening it.
enum PickerCommand: CaseIterable {
    case toggle, page, peek, format, keepOrder, undo
    case openSource, locateLibrary, chooseLibrary, reload
    case largerText, smallerText, settings

    var title: PickerWord {
        switch self {
        case .toggle: .add
        case .page: .editPage
        case .peek: .peek
        case .format: .changeFormat
        case .keepOrder: .keepOrder
        case .undo: .undo
        case .openSource: .openSource
        case .locateLibrary: .findSource
        case .chooseLibrary: .choose
        case .reload: .reload
        case .largerText: .textLarger
        case .smallerText: .textSmaller
        case .settings: .settings
        }
    }

    var group: PickerWord {
        switch self {
        case .openSource, .locateLibrary, .chooseLibrary, .reload: .actionsLibrary
        case .largerText, .smallerText, .settings: .actionsIbis
        default: .actionsCitation
        }
    }

    /// The key equivalent, as AppKit matches it and as its menu item shows it.
    var key: (key: String, modifiers: NSEvent.ModifierFlags)? {
        switch self {
        case .toggle: ("\r", [.shift])
        case .page: ("l", [.command])
        case .peek: ("y", [.command])
        case .format: ("f", [.shift, .command])
        case .undo: ("z", [.command])
        case .openSource: ("o", [.command])
        case .chooseLibrary: ("o", [.shift, .command])
        case .largerText: ("=", [.command])
        case .smallerText: ("-", [.command])
        case .settings: (",", [.command])
        case .keepOrder, .locateLibrary, .reload: nil
        }
    }

    static func matching(_ key: String?, modifiers: NSEvent.ModifierFlags) -> Self? {
        guard let key else { return nil }
        if key == "+" || key == "=", modifiers == [.command] || modifiers == [.command, .shift] { return .largerText }
        return allCases.first { $0.key?.key == key && $0.key?.modifiers == modifiers }
    }
}

extension PickerStore {
    func canRun(_ command: PickerCommand) -> Bool {
        switch command {
        case .toggle, .peek: highlight != nil && libraryReady
        case .page: !citation.isEmpty
        case .format: canChooseFormat
        case .keepOrder: added.count > 1 && state?.canKeepOrder == true
        case .undo: state?.canUndo == true
        case .openSource: libraryReady
        case .locateLibrary: catalog?.library?.condition == .missing
        case .reload: catalog?.library != nil && !isOpening
        case .chooseLibrary: !isDelivering && !choosingLibrary
        case .largerText: textPercent < 200
        case .smallerText: textPercent > 100
        case .settings: true
        }
    }

    private func title(_ command: PickerCommand) -> String {
        switch command {
        case .toggle:
            if let highlight, isAdded(highlight) {
                return PickerWords.text(.removeToken, entry(highlight)?.row.title ?? PickerWords.text(.untitled))
            }
            return PickerWords.text(.add)
        case .locateLibrary: return fixTitle(.locateLibrary)
        default: return PickerWords.text(command.title)
        }
    }

    @discardableResult
    func run(_ command: PickerCommand, repeating: Bool = false) -> Bool {
        guard canRun(command) else { return false }
        if repeating, command != .largerText, command != .smallerText { return true }
        switch command {
        case .toggle: if let highlight { toggle(highlight) }
        case .page: edit()
        case .peek: togglePeek()
        case .format: openFormatMenu()
        case .keepOrder: keepOrder(!(state?.keepOrder ?? false))
        case .undo: undo()
        case .openSource: panel?.openSource()
        case .locateLibrary: panel?.chooseLibrary(locate: true)
        case .chooseLibrary: panel?.chooseLibrary()
        case .reload: reload()
        case .largerText: changeSize(10)
        case .smallerText: changeSize(-10)
        case .settings: panel?.openSettings?()
        }
        return true
    }

    func fixTitle(_ fix: PickerFix) -> String {
        switch fix {
        case .chooseLibrary: PickerWords.text(.choose)
        case .locateLibrary: PickerWords.text(.findLibrary, catalog?.library?.name ?? "")
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
        case .reloadLibrary: reload()
        case .clearSearch: _ = escape()
        case .chooseFormat: openFormatMenu()
        case .editLocator, .editCitation: edit(status.item)
        case nil: break
        }
    }

    func emptyAction(primary: Bool) {
        switch emptyState {
        case .choose: panel?.chooseLibrary()
        case .unreadable: panel?.chooseLibrary()
        case .missing: panel?.chooseLibrary(locate: primary)
        case .noMatches: if primary { _ = escape() } else { panel?.openSource() }
        default: break
        }
    }

    /// Return finds the library first when there is nothing to cite yet.
    func resolveLibraryIfNeeded() -> Bool {
        switch emptyState {
        case .choose, .missing, .unreadable:
            emptyAction(primary: true)
            return true
        default: return false
        }
    }

    /// The popover closed itself, by Escape or a click elsewhere.
    func editingClosed() {
        guard editing != nil else { return }
        closeEditing(refocus: false)
        DispatchQueue.main.async { [weak self] in
            guard let self, let panel, panel.pickerIsOpen, editing == nil else { return }
            if panel.isKeyWindow || NSApp.keyWindow == nil { focusSearch() }
            else if !choosingLibrary { panel.dismiss() }
        }
    }

    // MARK: Native menus, opened at the control that owns them

    /// ⌘K: every command that can run now, grouped, with its key equivalent.
    func openActionsMenu() {
        let menu = NSMenu()
        var group: PickerWord?
        for command in PickerCommand.allCases where canRun(command) {
            if command.group != group {
                if group != nil { menu.addItem(.separator()) }
                menu.addItem(.sectionHeader(title: PickerWords.text(command.group)))
                group = command.group
            }
            let item = PickerMenuItem(title(command)) { [weak self] in self?.run(command) }
            if let key = command.key {
                item.keyEquivalent = key.key
                item.keyEquivalentModifierMask = key.modifiers
            }
            if command == .keepOrder { item.state = state?.keepOrder == true ? .on : .off }
            menu.addItem(item)
        }
        panel?.popUp(menu, at: anchors[.actions])
        if panel?.pickerIsOpen == true, !choosingLibrary { focusSearch() }
    }

    /// The references of a citation with several: each one's page, order and removal.
    func openCitationMenu() {
        let menu = NSMenu()
        let entries = citation
        for (index, entry) in entries.enumerated() {
            let title = [entry.row.title ?? PickerWords.text(.untitled), entry.note].compactMap { $0 }
                .joined(separator: " · ")
            let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
            let actions = NSMenu()
            let id = entry.row.id
            actions.addItem(PickerMenuItem(PickerWords.text(.editPage)) { [weak self] in self?.edit(id) })
            if entry.added {
                if index > 0 {
                    actions.addItem(PickerMenuItem(PickerWords.text(.moveEarlier)) { [weak self] in self?.reorder(id, by: -1) })
                }
                if index < entries.count - 1, entries[index + 1].added {
                    actions.addItem(PickerMenuItem(PickerWords.text(.moveLater)) { [weak self] in self?.reorder(id, by: 1) })
                }
                actions.addItem(.separator())
                actions.addItem(PickerMenuItem(PickerWords.text(.remove)) { [weak self] in self?.remove(id) })
            }
            item.submenu = actions
            item.image = NSImage(systemSymbolName: entry.added ? "checkmark.circle.fill" : entry.row.kind.symbol,
                                 accessibilityDescription: nil)
            menu.addItem(item)
        }
        if canRun(.keepOrder) {
            menu.addItem(.separator())
            let keep = PickerMenuItem(PickerWords.text(.keepOrder)) { [weak self] in self?.run(.keepOrder) }
            keep.state = state?.keepOrder == true ? .on : .off
            menu.addItem(keep)
        }
        if canRun(.undo) {
            let undo = PickerMenuItem(PickerWords.text(.undo)) { [weak self] in self?.undo() }
            undo.keyEquivalent = "z"
            undo.keyEquivalentModifierMask = .command
            menu.addItem(undo)
        }
        panel?.popUp(menu, at: anchors[.citation])
        if panel?.pickerIsOpen == true { focusSearch() }
    }

    func openFormatMenu() {
        guard canChooseFormat, let scope = formatScope, let formats = catalog?.formats else { return }
        let menu = NSMenu()
        if !formatContext.isEmpty { menu.addItem(.sectionHeader(title: formatContext)) }
        let held = state?.formatHeld == true
        let remember = scope.project == nil && formatOwner != nil
        if held {
            let usual = formats.first { $0.id == scope.format }?.name ?? PickerWords.text(.formatDefault)
            menu.addItem(PickerMenuItem(PickerWords.text(.formatLibraryUsual, usual)) { [weak self] in
                self?.useFormatOnce(nil)
            })
        }
        if let owner = rememberedFormatOwner {
            let forget = PickerMenuItem(PickerWords.text(.formatForget, owner)) { [weak self] in self?.forgetFormat() }
            if let usual = formats.first(where: { $0.id == scope.usualFormat }) {
                forget.toolTip = PickerWords.text(.formatLibraryUsual, usual.name)
            }
            menu.addItem(forget)
        }
        if remember, let owner = formatOwner {
            let once = NSMenu()
            addFormats(formats, to: once, remembering: false)
            let item = NSMenuItem(title: PickerWords.text(.formatForOne), action: nil, keyEquivalent: "")
            item.submenu = once
            menu.addItem(item)
            menu.addItem(.separator())
            menu.addItem(.sectionHeader(title: PickerWords.text(.formatRememberFor, owner)))
        } else {
            menu.addItem(.separator())
            menu.addItem(.sectionHeader(title: PickerWords.text(.formatForOne)))
        }
        addFormats(formats, to: menu, remembering: remember)
        panel?.popUp(menu, at: anchors[.format])
        if panel?.pickerIsOpen == true { focusSearch() }
    }

    private func addFormats(_ formats: [PickerFormat], to menu: NSMenu, remembering: Bool) {
        let held = state?.formatHeld == true
        for styles in [false, true] {
            let group = formats.filter { $0.canKeepOrder == styles }
            guard !group.isEmpty else { continue }
            menu.addItem(.sectionHeader(title: PickerWords.text(styles ? .formatStyleGroup : .formatKeyGroup)))
            for option in group {
                let item = PickerMenuItem(option.name) { [weak self] in
                    if remembering { self?.rememberFormat(option.id) } else { self?.useFormatOnce(option.id) }
                }
                let current = remembering ? !held : held || formatOwner == nil
                item.state = state?.format == option.id && current ? .on : .off
                menu.addItem(item)
            }
        }
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
