import Foundation

struct SearchQuery {
    let normalized: String
    private let words: [Substring]
    private let locale: Locale

    var isEmpty: Bool { words.isEmpty }

    init(_ text: String, locale: Locale = Locale(identifier: PickerWords.language.rawValue)) {
        self.locale = locale
        words = text.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: locale)
            .split(whereSeparator: \.isWhitespace)
        normalized = words.joined(separator: " ")
    }

    func ranked<Item>(_ items: [Item], title: (Item) -> String, aliases: (Item) -> String) -> [Item] {
        guard !isEmpty else { return items }
        return items.compactMap { item -> (item: Item, rank: Int)? in
            let title = folded(title(item))
            if title == normalized { return (item, 0) }
            if title.hasPrefix(normalized) { return (item, 1) }
            if words.allSatisfy({ title.contains($0) }) { return (item, 2) }
            let searchable = title + " " + folded(aliases(item))
            return words.allSatisfy({ searchable.contains($0) }) ? (item, 3) : nil
        }.sorted { $0.rank < $1.rank }.map(\.item)
    }

    private func folded(_ value: String) -> String {
        value.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: locale)
    }
}
