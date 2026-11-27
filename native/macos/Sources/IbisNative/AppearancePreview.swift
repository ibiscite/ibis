import AppKit
import SwiftUI

@MainActor
struct AppearancePreview: View {
    let appearance: Appearance
    let density: Density
    let largerText: Bool

    var body: some View {
        let metrics = PickerMetrics(scale: largerText ? 1.5 : 1, density: density)
        AppearanceSample(appearance: appearance, metrics: metrics)
            .frame(height: max(40, metrics.pt(36)) + metrics.row * 2 + 12)
            .accessibilityHidden(true)
    }
}

private struct AppearanceSample: NSViewRepresentable {
    let appearance: Appearance
    let metrics: PickerMetrics

    func makeNSView(context: Context) -> AppearanceSampleView { AppearanceSampleView() }

    func updateNSView(_ view: AppearanceSampleView, context: Context) {
        view.configure(appearance: appearance, metrics: metrics)
    }
}

@MainActor
private final class AppearanceSampleView: NSView {
    private let mark = NSImageView()
    private let search = PickerSearchField()
    private let format = pickerLabel("APA")
    private let rule = PickerRule()
    private let results = PickerResults()
    private var metrics = PickerMetrics(scale: 1, density: .compact)

    override var isFlipped: Bool { true }

    init() {
        super.init(frame: .zero)
        wantsLayer = true
        layer?.cornerRadius = 12
        layer?.masksToBounds = true
        layer?.borderWidth = 1
        mark.imageScaling = .scaleProportionallyUpOrDown
        search.stringValue = "shoreline"
        search.isEditable = false
        search.isSelectable = false
        search.focusRingType = .none
        (search.cell as? NSSearchFieldCell)?.cancelButtonCell = nil
        format.alignment = .right
        format.textColor = .secondaryLabelColor
        results.scroll.hasVerticalScroller = false
        [mark, search, format, rule, results].forEach(addSubview)
        updateAppearance()
    }

    required init?(coder: NSCoder) { nil }

    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    func configure(appearance: Appearance, metrics: PickerMetrics) {
        self.metrics = metrics
        switch appearance {
        case .system: self.appearance = nil
        case .light: self.appearance = NSAppearance(named: .aqua)
        case .dark: self.appearance = NSAppearance(named: .darkAqua)
        }
        needsLayout = true
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        updateAppearance()
    }

    private func updateAppearance() {
        mark.image = PickerArt.image(PickerArt.isDark(effectiveAppearance) ? "pickerMark-dark" : "pickerMark")
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer?.backgroundColor = NSColor.windowBackgroundColor.cgColor
            layer?.borderColor = NSColor.separatorColor.cgColor
        }
    }

    override func layout() {
        super.layout()
        let width = bounds.width
        let header = max(40, metrics.pt(36))
        let markSize = metrics.pt(24)
        let fieldHeight = metrics.pt(25)
        format.font = metrics.font(11)
        let formatWidth = pickerWidth(format.stringValue, font: metrics.font(11))
        let trailing = width - metrics.inset - formatWidth
        mark.frame = NSRect(x: metrics.tileCenter - markSize / 2, y: (header - markSize) / 2,
                            width: markSize, height: markSize)
        search.font = metrics.font(13)
        search.frame = NSRect(x: metrics.column, y: (header - fieldHeight) / 2,
                              width: max(0, trailing - metrics.column - metrics.pt(12)), height: fieldHeight)
        pickerLine(format, x: trailing, y: 0, width: formatWidth, height: header)
        rule.frame = NSRect(x: 0, y: header, width: width, height: 1)
        results.frame = NSRect(x: 0, y: header + 6, width: width, height: metrics.row * CGFloat(Self.records.count))
        results.configure(sections: [PickerSection(kind: .best, rows: Self.records)],
                          metrics: metrics, recent: false, selected: Self.records.first?.id, cited: [], numbers: false)
    }

    private static let records = [
        row("measure", "Measuring a shifting shoreline", author: "Mira Vale", venue: "Tideline Review", year: "2024"),
        row("map", "Mapping the quiet shoreline", author: "Jonah Fen", venue: "Coastal Methods", year: "2022"),
    ]

    private static func row(_ key: String, _ title: String, author: String, venue: String, year: String) -> PickerRow {
        let byline = "\(author) · \(venue)"
        return PickerRow(authorCount: 1, authors: [author], bylines: [byline, author], citationKey: nil,
                         cite: key, firstAuthor: author, fullByline: byline,
                         id: ItemRef(key: key, library: "appearance-preview"), itemType: nil, journal: venue,
                         kind: .article, kindLabel: "", title: title, year: year)
    }
}
