import AppKit

struct PickerAction: Equatable {
    let command: PickerCommand
    let title: String
    let group: String
    let aliases: String
    let shortcut: String
    let checked: Bool?
    var reference: ItemRef?
}

enum PickerCommand: CaseIterable {
    case format, peek, membership, page, details, focusCitation, remove, typeCitation, keepOrder, undo
    case openSource, locateLibrary, chooseLibrary, reload
    case largerText, smallerText, settings

    var title: PickerWord {
        switch self {
        case .format: .changeFormat
        case .peek: .peek
        case .membership: .add
        case .page: .editPage
        case .details: .details
        case .focusCitation: .goToCitation
        case .remove: .remove
        case .typeCitation: .typeCitation
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

    private var group: PickerWord {
        switch self {
        case .openSource, .locateLibrary, .chooseLibrary, .reload: .actionsLibrary
        case .largerText, .smallerText, .settings: .actionsIbis
        default: .actionsCitation
        }
    }

    private var aliases: PickerWord? {
        switch self {
        case .format: .actionWordsFormat
        case .peek: .actionWordsPeek
        case .page, .details: .actionWordsDetails
        case .focusCitation: .actionWordsFocus
        case .membership, .remove: .actionWordsRemove
        case .keepOrder: .actionWordsOrder
        case .openSource: .actionWordsSource
        case .locateLibrary, .chooseLibrary: .actionWordsChoose
        case .largerText, .smallerText: .actionWordsText
        case .settings: .actionWordsSettings
        default: nil
        }
    }

    private var binding: (key: String, modifiers: NSEvent.ModifierFlags, label: String)? {
        switch self {
        case .format: ("f", [.shift, .command], "⇧⌘F")
        case .peek: ("y", [.command], "⌘Y")
        case .membership: ("\r", [.shift], "⇧↩")
        case .page: ("l", [.command], "⌘L")
        case .details: ("i", [.command], "⌘I")
        case .focusCitation: ("\u{f701}", [.command], "⌘↓")
        case .remove: nil
        case .typeCitation: ("t", [.command], "⌘T")
        case .undo: ("z", [.command], "⌘Z")
        case .openSource: ("o", [.command], "⌘O")
        case .chooseLibrary: ("o", [.shift, .command], "⇧⌘O")
        case .largerText: ("=", [.command], "⌘+")
        case .smallerText: ("-", [.command], "⌘−")
        case .settings: (",", [.command], "⌘,")
        case .locateLibrary, .reload, .keepOrder: nil
        }
    }

    static func matching(_ key: String?, modifiers: NSEvent.ModifierFlags) -> Self? {
        guard let key else { return nil }
        if (key == "+" || key == "="), modifiers == [.command] || modifiers == [.command, .shift] {
            return .largerText
        }
        return allCases.first { $0.binding?.key == key && $0.binding?.modifiers == modifiers }
    }

    func action(title: String? = nil, checked: Bool? = nil, reference: ItemRef? = nil) -> PickerAction {
        PickerAction(command: self, title: title ?? PickerWords.text(self.title), group: PickerWords.text(group),
                     aliases: aliases.map { PickerWords.text($0) } ?? "", shortcut: binding?.label ?? "",
                     checked: checked, reference: reference)
    }
}
