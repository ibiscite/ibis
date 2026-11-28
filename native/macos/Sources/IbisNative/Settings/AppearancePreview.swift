import SwiftUI

/// A small picker drawn with the picker's own row, so the preview cannot drift.
struct AppearancePreview: View {
    let appearance: Appearance
    let density: Density
    let largerText: Bool

    var body: some View {
        let grid = PickerGrid(s: largerText ? 1.25 : 0.9)
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 8 * grid.s) {
                BrandMark().frame(width: 22 * grid.s, height: 22 * grid.s).frame(width: grid.lead - 8 * grid.s)
                Text(verbatim: "shoreline").font(grid.font(20))
                Spacer()
            }
            .frame(height: grid.header * 0.85)
            Divider()
            Text(PickerSectionKind.best.label)
                .font(grid.font(11, .semibold)).foregroundStyle(.secondary)
                .padding(.leading, 18 * grid.s)
                .frame(height: grid.heading, alignment: .bottom)
            ForEach(Array(Self.rows.enumerated()), id: \.offset) { index, row in
                ResultRow(row: row, highlighted: index == 0, emphasized: index == 0, added: index == 1,
                          note: index == 1 ? "p. 417" : nil, number: nil, density: density, grid: grid)
            }
            Spacer(minLength: 6 * grid.s)
            Divider()
            HStack(spacing: 12 * grid.s) {
                Text(verbatim: "APA").foregroundStyle(.primary)
                Text(verbatim: "(Okafor & Lin, 2019; Vale, 2021, p. 417)").foregroundStyle(.secondary).lineLimit(1)
                Spacer(minLength: 0)
                KeyCap("↩", grid: grid)
            }
            .font(grid.font(12))
            .padding(.horizontal, 18 * grid.s)
            .frame(height: grid.footer * 0.85)
        }
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
        sample("salt", "Salt Marsh Field Notes", "Aster et al.", nil, .report),
    ]

    private static func sample(_ key: String, _ title: String, _ byline: String, _ year: String?,
                               _ kind: PickerRowKind) -> PickerRow {
        PickerRow(authorCount: 1, authors: [], bylines: [byline], citationKey: key, cite: key, firstAuthor: nil,
                  fullByline: byline, id: ItemRef(key: key, library: "sample"), itemType: nil, journal: nil,
                  kind: kind, kindLabel: "", matchReasons: [], title: title, year: year)
    }
}
