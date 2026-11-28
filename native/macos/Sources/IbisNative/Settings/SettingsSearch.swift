enum SettingsPane: CaseIterable {
    case general, appearance, library

    var title: String {
        switch self {
        case .general: PickerWords.text(.generalSettings)
        case .appearance: PickerWords.text(.appearanceSettings)
        case .library: PickerWords.text(.library)
        }
    }
}

enum SettingsSearchRow: String, CaseIterable, Identifiable {
    case library, shortcut, paste, appearance, searchGlyph, density, largerText

    var id: Self { self }

    var pane: SettingsPane {
        switch self {
        case .shortcut, .paste: .general
        case .library: .library
        case .appearance, .searchGlyph, .density, .largerText: .appearance
        }
    }

    var word: PickerWord {
        switch self {
        case .library: .library
        case .shortcut: .open
        case .paste: .automaticPaste
        case .appearance: .appearance
        case .searchGlyph: .searchGlyph
        case .density: .density
        case .largerText: .largerText
        }
    }

    var title: String { PickerWords.text(word) }

    private var searchWords: [PickerWord] {
        switch self {
        case .library: [.choose, .findSource, .actionWordsChoose]
        case .shortcut: [.shortcuts, .settingsShortcutWords, .restoreShortcut]
        case .paste: [.pasteSettings, .settingsPasteWords, .pasteNeedsAccess]
        case .appearance: [.settingsAppearanceWords]
        case .searchGlyph: [.searchGlyphBird, .searchGlyphMagnifier, .settingsSearchGlyphWords]
        case .density: [.compact, .comfortable]
        case .largerText: [.settingsTextSize, .actionWordsText]
        }
    }

    fileprivate var aliases: String {
        ([pane.title] + searchWords.map { PickerWords.text($0) }).joined(separator: " ")
    }
}

struct SettingsSearch {
    private(set) var query = ""
    private(set) var matches: [SettingsSearchRow] = []
    private(set) var highlighted: SettingsSearchRow?

    var isSearching: Bool { !query.allSatisfy(\.isWhitespace) }

    mutating func update(_ text: String) {
        guard query != text else { return }
        query = text
        let terms = SearchQuery(text)
        matches = terms.isEmpty ? [] : terms.ranked(SettingsSearchRow.allCases, title: \.title, aliases: \.aliases)
        highlighted = matches.first
    }

    mutating func move(_ step: Int) {
        guard let highlighted, let index = matches.firstIndex(of: highlighted) else { return }
        self.highlighted = matches[min(max(index + step, 0), matches.count - 1)]
    }
}
