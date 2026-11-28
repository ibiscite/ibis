import SwiftUI

/// The picker's own search line and two of its rows, one selected and one
/// plain, so the preview shows exactly what the settings above it change.
struct AppearancePreview: View {
    let appearance: Appearance
    let density: Density
    let largerText: Bool
    let searchGlyph: SearchGlyph

    var body: some View {
        let grid = PickerGrid(s: largerText ? 1.25 : 0.9)
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 0) {
                if searchGlyph != .none {
                    SearchFieldGlyph(kind: searchGlyph, grid: grid).padding(.trailing, grid.title - grid.edge - grid.icon)
                }
                Text(verbatim: "shoreline").font(.system(size: grid.query))
                Spacer(minLength: 0)
            }
            .padding(.leading, grid.edge)
            .frame(height: grid.header * 0.8)
            ForEach(Array(Self.rows.enumerated()), id: \.offset) { index, row in
                ResultRow(row: row, highlighted: index == 0, emphasized: index == 0, added: false,
                          note: index == 1 ? "p. 417" : nil, number: nil, density: density, grid: grid)
            }
        }
        .padding(.vertical, 8 * grid.s)
        .systemGlass(corner: 20)
        .environment(\.colorScheme, scheme ?? systemScheme)
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }

    @Environment(\.colorScheme) private var systemScheme

    private var scheme: ColorScheme? {
        switch appearance {
        case .system: nil
        case .light: .light
        case .dark: .dark
        }
    }

    // Invented sample records; nothing here comes from a library.
    private static let rows: [PickerRow] = [
        sample("shore", "Shorelines in Motion", "Okafor & Lin · Coastal Studies", "2019", .article),
        sample("tide", "Reading the Tide Line", "Vale · Harbour Press", "2021", .book),
    ]

    private static func sample(_ key: String, _ title: String, _ byline: String, _ year: String?,
                               _ kind: PickerRowKind) -> PickerRow {
        PickerRow(authorCount: 1, authors: [], bylines: [byline], citationKey: key, cite: key, firstAuthor: nil,
                  fullByline: byline, id: ItemRef(key: key, library: "sample"), itemType: nil, journal: nil,
                  kind: kind, kindLabel: "", matchReasons: [], title: title, year: year)
    }
}
