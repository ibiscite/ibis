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
                .font(grid.font(12, .medium)).foregroundStyle(.secondary)
                .padding(.leading, grid.lead)
                .frame(height: grid.heading)
            ForEach(Array(Self.rows.enumerated()), id: \.offset) { index, row in
                ResultRow(row: row, selected: index == 0, emphasized: index == 0, cited: index == 1,
                          number: nil, density: density, grid: grid)
            }
            Spacer(minLength: 6 * grid.s)
        }
        .background(SystemMaterial(material: .popover))
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous)
            .strokeBorder(Color(nsColor: .separatorColor), lineWidth: 0.5))
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
