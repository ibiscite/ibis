import AppKit
import SwiftUI

@MainActor
struct AppearancePreview: View {
    let appearance: Appearance
    let density: Density
    let largerText: Bool

    var body: some View {
        let metrics = PickerMetrics(scale: (largerText ? 1.5 : 1) * 0.85, density: density)
        AppearanceSample(appearance: appearance, metrics: metrics)
            .frame(height: AppearanceSampleView.height(for: metrics))
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
    private let heading = pickerLabel(PickerSectionKind.best.label)
    private let rule = PickerRule()
    private let searchFocus = PickerRule()
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
        search.isBezeled = false
        search.isBordered = false
        search.drawsBackground = false
        search.focusRingType = .none
        search.controlSize = .large
        if let cell = search.cell as? NSSearchFieldCell {
            cell.searchButtonCell = nil
            cell.cancelButtonCell = nil
        }
        searchFocus.color = .keyboardFocusIndicatorColor
        heading.textColor = .secondaryLabelColor
        results.scroll.hasVerticalScroller = false
        [mark, search, searchFocus, rule, heading, results].forEach(addSubview)
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
        let header = metrics.header
        let markSize = metrics.pt(32)
        mark.frame = NSRect(x: metrics.tileCenter - markSize / 2, y: (header - markSize) / 2,
                            width: markSize, height: markSize)
        let contentWidth = max(0, width - metrics.inset - metrics.column)
        search.font = metrics.searchFont
        search.frame = NSRect(x: metrics.column, y: (header - metrics.searchHeight) / 2,
                              width: contentWidth, height: metrics.searchHeight)
        searchFocus.frame = NSRect(x: metrics.column, y: search.frame.maxY - metrics.line,
                                   width: contentWidth, height: metrics.line)
        rule.frame = NSRect(x: 0, y: header, width: width, height: metrics.line)
        let headingY = header + metrics.line
        heading.font = metrics.font(12, .medium)
        pickerLine(heading, x: metrics.column, y: headingY + metrics.pt(2), width: contentWidth, height: metrics.section)
        results.frame = NSRect(x: 0, y: headingY + metrics.section, width: width,
                               height: metrics.row * CGFloat(Self.records.count))
        results.configure(sections: [PickerSection(kind: .best, rows: Self.records)],
                          metrics: metrics, recent: false, selected: Self.records.first?.id, cited: [], numbers: false,
                          quickDelivery: true)
    }

    static func height(for metrics: PickerMetrics) -> CGFloat {
        metrics.header + metrics.line + metrics.section + metrics.row * CGFloat(records.count) + metrics.pt(6)
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
                         kind: .article, kindLabel: "", matchReasons: [], title: title, year: year)
    }
}
