import AppKit
import SwiftUI

// The picker's content: one field, one list, one footer line, laid out with
// the system's own controls. Every size is a multiple of the picker's scale.

/// The picker's grid: the lead column holds glyphs, text starts after it.
struct PickerGrid {
    let s: CGFloat
    var width: CGFloat { 640 * s }
    var lead: CGFloat { 48 * s }
    var trailing: CGFloat { 16 * s }
    var header: CGFloat { 56 * s }
    var hint: CGFloat { 22 * s }
    var heading: CGFloat { 26 * s }
    var footer: CGFloat { 44 * s }
    var status: CGFloat { 30 * s }
    /// Room for an empty state's symbol, two lines and a button, at any text size.
    var empty: CGFloat { 170 + 50 * s }
    func row(_ density: Density) -> CGFloat { (density == .compact ? 44 : 52) * s }
    func font(_ size: CGFloat, _ weight: Font.Weight = .regular) -> Font { .system(size: size * s, weight: weight) }
}

struct PickerView: View {
    @ObservedObject var store: PickerStore

    var body: some View {
        let grid = PickerGrid(s: store.scale)
        VStack(spacing: 0) {
            SearchHeader(store: store, grid: grid)
            if let readsAs = store.state?.readsAs {
                Text(readsAs)
                    .font(grid.font(12))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .padding(.leading, grid.lead)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .frame(height: grid.hint, alignment: .top)
                    .accessibilityLabel(readsAs)
            }
            if let empty = store.emptyState {
                Divider()
                EmptyStateView(state: empty, store: store, grid: grid).frame(height: grid.empty)
            } else if store.showsList {
                Divider()
                ResultsList(store: store, grid: grid).frame(height: store.listHeight(grid))
            }
            if let problem = store.problem {
                StatusLine(text: problem.text, status: problem.status, store: store, grid: grid)
            }
            if store.showsFooter {
                Divider()
                Footer(store: store, grid: grid)
            }
            Spacer(minLength: 0)
        }
        .onPreferenceChange(AnchorFrames.self) { store.anchors = $0 }
    }
}

// MARK: Search

private struct SearchHeader: View {
    @ObservedObject var store: PickerStore
    let grid: PickerGrid

    var body: some View {
        HStack(spacing: 8 * grid.s) {
            SearchGlyph(grid: grid)
            NativeTextField(
                text: store.query,
                placeholder: PickerWords.text(.search),
                label: PickerWords.text(.search),
                style: .plain,
                font: .systemFont(ofSize: 22 * grid.s),
                register: { store.searchField = $0 },
                began: {},
                change: { store.type($0) }
            )
            if store.isOpening {
                ProgressView().controlSize(.small)
            }
        }
        .padding(.trailing, grid.trailing)
        .frame(height: grid.header)
        .background(WindowDragArea())
    }
}

/// The search field's glyph, as in Spotlight: the brand stays in the menu bar.
struct SearchGlyph: View {
    let grid: PickerGrid

    var body: some View {
        Image(systemName: "magnifyingglass")
            .font(grid.font(20))
            .foregroundStyle(.secondary)
            .frame(width: grid.lead - 8 * grid.s)
            .accessibilityHidden(true)
    }
}

// MARK: Results

private struct RowFrames: PreferenceKey {
    static let defaultValue: [ItemRef: CGRect] = [:]
    static func reduce(value: inout [ItemRef: CGRect], nextValue: () -> [ItemRef: CGRect]) {
        value.merge(nextValue()) { $1 }
    }
}

private struct ResultsList: View {
    @ObservedObject var store: PickerStore
    let grid: PickerGrid

    var body: some View {
        GeometryReader { viewport in
            ScrollViewReader { scroll in
                ScrollView(.vertical) {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        ForEach(Array(store.sections.enumerated()), id: \.offset) { _, section in
                            SectionHeading(title: section.kind.label, grid: grid)
                            ForEach(section.rows, id: \.id) { row in
                                resultRow(row)
                            }
                        }
                    }
                    .padding(.vertical, 6 * grid.s)
                }
                .scrollIndicators(.automatic)
                .coordinateSpace(name: "results")
                .onPreferenceChange(RowFrames.self) { frames in
                    store.visibleRows = frames
                        .filter { $0.value.minY >= -0.5 && $0.value.maxY <= viewport.size.height + 0.5 }
                        .sorted { $0.value.minY < $1.value.minY }
                        .map(\.key)
                }
                .onChange(of: store.revealed) {
                    if let highlight = store.highlight { scroll.scrollTo(highlight) }
                }
            }
        }
        .accessibilityLabel(PickerWords.text(.references))
    }

    private func resultRow(_ row: PickerRow) -> some View {
        let highlighted = row.id == store.highlight
        let added = store.isAdded(row.id)
        return ResultRow(row: row, highlighted: highlighted, emphasized: highlighted && store.isKey,
                         added: added, note: store.entry(row.id)?.note, number: store.number(for: row.id),
                         density: store.density, grid: grid,
                         actions: RowActions(toggle: { store.toggle(row.id) }, peek: { store.togglePeek(row.id) }))
            .background(GeometryReader { geometry in
                Color.clear.preference(key: RowFrames.self, value: [row.id: geometry.frame(in: .named("results"))])
            })
            .onTapGesture { click(row) }
            .contextMenu { rowMenu(row, added: added) }
            .id(row.id)
    }

    @ViewBuilder private func rowMenu(_ row: PickerRow, added: Bool) -> some View {
        Button(PickerWords.text(added ? .remove : .add)) { store.toggle(row.id) }
        if added { Button(PickerWords.text(.editPage)) { store.edit(row.id) } }
        Button(PickerWords.text(.peek)) { store.togglePeek(row.id) }
    }

    private func click(_ row: PickerRow) {
        let event = NSApp.currentEvent
        if event?.modifierFlags.contains(.command) == true {
            store.toggle(row.id)
        } else if (event?.clickCount ?? 1) >= 2, row.id == store.highlight {
            store.deliver()
            return
        } else {
            store.highlight(row.id)
        }
        store.focusSearch()
    }
}

private struct SectionHeading: View {
    let title: String
    let grid: PickerGrid

    var body: some View {
        Text(title)
            .font(grid.font(11, .semibold))
            .foregroundStyle(.secondary)
            .padding(.leading, 18 * grid.s)
            .padding(.bottom, 4 * grid.s)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottomLeading)
            .frame(height: grid.heading)
            .accessibilityAddTraits(.isHeader)
    }
}

struct RowActions {
    var toggle: (() -> Void)?
    var peek: (() -> Void)?
}

/// Two lines: the title, then who, when and any page. A reference in the
/// citation carries a check; the highlighted one takes the selection colour.
struct ResultRow: View {
    let row: PickerRow
    let highlighted: Bool
    let emphasized: Bool
    let added: Bool
    let note: String?
    let number: Int?
    let density: Density
    let grid: PickerGrid
    var actions = RowActions()
    @State private var hovered = false

    var body: some View {
        let ink = emphasized ? Color(nsColor: .alternateSelectedControlTextColor) : Color(nsColor: .labelColor)
        let quiet = emphasized ? ink.opacity(0.82) : Color(nsColor: .secondaryLabelColor)
        HStack(spacing: 0) {
            Group {
                if let number {
                    Text(verbatim: "\(number)").font(grid.font(13, .medium)).monospacedDigit().foregroundStyle(quiet)
                } else if added {
                    Image(systemName: "checkmark.circle.fill")
                        .font(grid.font(16))
                        .foregroundStyle(emphasized ? ink : Color.accentColor)
                } else {
                    Image(systemName: row.kind.symbol).font(grid.font(15)).foregroundStyle(quiet)
                }
            }
            .frame(width: 30 * grid.s)
            .padding(.trailing, 10 * grid.s)
            VStack(alignment: .leading, spacing: (density == .compact ? 1 : 3) * grid.s) {
                Text(row.title ?? PickerWords.text(.untitled))
                    .font(grid.font(14, .medium))
                    .foregroundStyle(ink)
                    .lineLimit(1)
                HStack(spacing: 0) {
                    Byline(options: row.bylines)
                    Text(verbatim: " · " + (row.year ?? PickerWords.text(.noDate))).fixedSize()
                    if let note {
                        Text(verbatim: " · ").fixedSize()
                        Text(note).fontWeight(.medium).foregroundStyle(emphasized ? ink : Color.accentColor).fixedSize()
                    }
                    if !row.matchReasons.isEmpty {
                        Text(verbatim: "  " + row.matchReasons.joined(separator: " · ")).italic().lineLimit(1)
                    }
                }
                .font(grid.font(12))
                .foregroundStyle(quiet)
            }
            Spacer(minLength: 8 * grid.s)
            if hovered {
                HStack(spacing: 6 * grid.s) {
                    if let toggle = actions.toggle {
                        HoverButton(symbol: added ? "minus" : "plus", label: added ? .remove : .add, ink: ink,
                                    grid: grid, perform: toggle)
                    }
                    if let peek = actions.peek { HoverButton(symbol: "eye", label: .peek, ink: ink, grid: grid, perform: peek) }
                }
            }
            if emphasized {
                Text(verbatim: "↩").font(grid.font(12, .medium)).foregroundStyle(quiet).padding(.leading, 6 * grid.s)
                    .accessibilityHidden(true)
            }
        }
        .padding(.leading, 10 * grid.s)
        .padding(.trailing, 12 * grid.s)
        .frame(height: grid.row(density))
        .background(RoundedRectangle(cornerRadius: 10 * grid.s, style: .continuous).fill(fill))
        .padding(.horizontal, 8 * grid.s)
        .contentShape(Rectangle())
        .onHover { hovered = $0 }
        .help(([row.title ?? "", row.fullByline] + row.matchReasons + [row.citationKey ?? ""])
            .filter { !$0.isEmpty }.joined(separator: "\n"))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(([row.title ?? PickerWords.text(.untitled), row.fullByline, row.kindLabel]
            + row.matchReasons + (added ? [PickerWords.text(.cited)] : []) + [note, row.citationKey].compactMap { $0 })
            .joined(separator: ", "))
        .accessibilityHint(number.map { "⌘\($0)" } ?? "")
        .accessibilityAddTraits(highlighted ? [.isSelected, .isButton] : .isButton)
        .accessibilityActions {
            if let toggle = actions.toggle { Button(PickerWords.text(added ? .remove : .add), action: toggle) }
            if let peek = actions.peek { Button(PickerWords.text(.peek), action: peek) }
        }
    }

    private var fill: Color {
        if emphasized { return Color(nsColor: .selectedContentBackgroundColor) }
        if highlighted { return Color(nsColor: .unemphasizedSelectedContentBackgroundColor) }
        return hovered ? Color.primary.opacity(0.06) : .clear
    }
}

private struct HoverButton: View {
    let symbol: String
    let label: PickerWord
    let ink: Color
    let grid: PickerGrid
    let perform: () -> Void

    var body: some View {
        Button(action: perform) {
            Image(systemName: symbol)
                .font(grid.font(11, .semibold))
                .foregroundStyle(ink)
                .frame(width: 24 * grid.s, height: 24 * grid.s)
                .background(Circle().fill(Color.primary.opacity(0.08)))
        }
        .buttonStyle(.plain)
        .help(PickerWords.text(label))
        .accessibilityLabel(PickerWords.text(label))
    }
}

/// The engine sends bylines longest first; show the first one that fits whole.
private struct Byline: View {
    let options: [String]

    var body: some View {
        ViewThatFits(in: .horizontal) {
            ForEach(options, id: \.self) { Text($0).lineLimit(1).fixedSize() }
            Text(options.last ?? "").lineLimit(1)
        }
    }
}

extension PickerRowKind {
    var symbol: String {
        switch self {
        case .article: "doc.text"
        case .book: "book.closed"
        case .chapter: "text.book.closed"
        case .thesis: "graduationcap"
        case .web: "globe"
        case .report: "doc.richtext"
        case .other: "doc"
        }
    }
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

// MARK: Status and empty states

private struct StatusLine: View {
    let text: String
    let status: PickerStatus?
    let store: PickerStore
    let grid: PickerGrid

    var body: some View {
        HStack(spacing: 8 * grid.s) {
            Image(systemName: status?.kind == .citation ? "xmark.octagon.fill" : "exclamationmark.triangle.fill")
                .symbolRenderingMode(.multicolor)
                .font(grid.font(12))
                .frame(width: 30 * grid.s)
            Text(text).font(grid.font(12)).foregroundStyle(.secondary).lineLimit(1).help(text)
            Spacer(minLength: 8 * grid.s)
            if let status, let fix = status.fix {
                Button(store.fixTitle(fix)) { store.runFix(status) }
                    .buttonStyle(.link)
                    .font(grid.font(12))
            }
        }
        .padding(.leading, 18 * grid.s)
        .padding(.trailing, grid.trailing)
        .frame(height: grid.status)
    }
}

private struct EmptyStateView: View {
    let state: PickerStore.EmptyState
    let store: PickerStore
    let grid: PickerGrid

    var body: some View {
        let content = self.content
        ContentUnavailableView {
            Label(content.title, systemImage: content.symbol).foregroundStyle(.primary)
        } description: {
            Text(PickerWords.text(content.hint))
        } actions: {
            if let primary = content.primary {
                Button(primary) { store.emptyAction(primary: true) }
                    .buttonStyle(.borderedProminent)
            }
            if let secondary = content.secondary {
                Button(secondary) { store.emptyAction(primary: false) }.buttonStyle(.link)
            }
        }
        .controlSize(grid.s >= 1.4 ? .large : .regular)
    }

    private var content: (symbol: String, title: String, hint: PickerWord, primary: String?, secondary: String?) {
        switch state {
        case .newLibrary:
            ("books.vertical", PickerWords.text(.emptyLibrary), .emptyLibraryHint, nil, nil)
        case .noMatches:
            ("magnifyingglass", PickerWords.text(.noMatches), .noMatchesHint, PickerWords.text(.clear), nil)
        case .choose:
            ("books.vertical", PickerWords.text(.chooseTitle), .chooseHint,
             PickerWords.text(.choose), PickerWords.text(.settings))
        case let .missing(title, name):
            ("questionmark.folder", title, .missingHint, PickerWords.text(.findLibrary, name), PickerWords.text(.choose))
        case let .unreadable(problem):
            ("exclamationmark.triangle", problem, .chooseHint, PickerWords.text(.choose), nil)
        }
    }
}

// MARK: Footer

/// One line: the format, exactly what Return delivers, and the two keys.
private struct Footer: View {
    @ObservedObject var store: PickerStore
    let grid: PickerGrid

    var body: some View {
        HStack(spacing: 12 * grid.s) {
            Button { store.openFormatMenu() } label: {
                HStack(spacing: 3 * grid.s) {
                    Text(store.formatName).lineLimit(1)
                    Image(systemName: "chevron.up.chevron.down").font(grid.font(8, .semibold))
                }
                .frame(maxWidth: 140 * grid.s)
                .fixedSize()
            }
            .buttonStyle(.borderless)
            .foregroundStyle(.primary)
            .disabled(!store.canChooseFormat)
            .help(store.formatContext.isEmpty ? store.formatName : store.formatContext)
            .accessibilityLabel(PickerWords.text(.formatValue, store.formatName))
            .accessibilityHint(store.formatContext)
            .anchor(.format)

            CitationPreview(store: store, grid: grid)
                .frame(maxWidth: .infinity, alignment: .leading)
                .anchor(.citation)
                .popover(isPresented: Binding(
                    get: { store.editing != nil },
                    set: { if !$0 { store.editingClosed() } }
                ), arrowEdge: .bottom) {
                    if let item = store.editing { DetailsPopover(store: store, item: item) }
                }

            Button { store.deliver() } label: {
                HStack(spacing: 5 * grid.s) {
                    if let icon = store.destinationIcon {
                        Image(nsImage: icon).resizable().frame(width: 16 * grid.s, height: 16 * grid.s)
                            .accessibilityHidden(true)
                    }
                    Text(store.deliveryTitle).lineLimit(1)
                    KeyCap("↩", grid: grid)
                }
            }
            .buttonStyle(.borderless)
            .foregroundStyle(.primary)
            .fontWeight(.medium)
            .fixedSize()
            .disabled(!store.canDeliver)
            .accessibilityLabel(store.deliveryTitle)
            .accessibilityHint(PickerWords.text(.returnKey))

            Button { store.openActionsMenu() } label: {
                KeyCap("⌘K", grid: grid)
            }
            .buttonStyle(.borderless)
            .foregroundStyle(.secondary)
            .fixedSize()
            .help(PickerWords.text(.actionsTitle))
            .accessibilityLabel(PickerWords.text(.actionsTitle))
            .anchor(.actions)
        }
        .font(grid.font(12))
        .padding(.leading, 18 * grid.s)
        .padding(.trailing, 14 * grid.s)
        .frame(height: grid.footer)
    }
}

/// The citation exactly as Return delivers it. With several references a
/// count opens their menu: page, order and removal live there, not in a tray.
private struct CitationPreview: View {
    @ObservedObject var store: PickerStore
    let grid: PickerGrid

    var body: some View {
        let citation = store.citation
        HStack(spacing: 8 * grid.s) {
            if let preview = store.state?.preview, !preview.text.isEmpty {
                Text(CitationText.styled(preview.display, fallback: preview.text, scale: grid.s * 0.92))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .opacity(store.canDeliver ? 1 : 0.6)
                    .help(preview.text)
                    .accessibilityLabel(PickerWords.text(.token, preview.text))
                    .onTapGesture { store.edit() }
            }
            if citation.count > 1 {
                Button { store.openCitationMenu() } label: {
                    HStack(spacing: 2 * grid.s) {
                        Text(PickerWords.text(.referenceCount, citation.count.formatted()))
                        Image(systemName: "chevron.up.chevron.down").font(grid.font(8, .semibold))
                    }
                    .fixedSize()
                }
                .buttonStyle(.borderless)
                .foregroundStyle(.secondary)
            }
        }
    }
}

/// The page, the words before and after, and the form of one reference.
private struct DetailsPopover: View {
    @ObservedObject var store: PickerStore
    let item: ItemRef

    var body: some View {
        let entry = store.entry(item)
        let details = entry?.details ?? .empty
        let options = store.catalog?.locators ?? []
        let term = details.locator?.term ?? .page
        let name = options.first { $0.term == term }?.name ?? PickerWords.text(.page)
        Form {
            if let title = entry?.row.title { Text(title).font(.headline).lineLimit(2) }
            LabeledContent {
                NativeTextField(
                    text: details.locator?.value ?? "", placeholder: "", label: name, style: .rounded,
                    font: .systemFont(ofSize: NSFont.systemFontSize),
                    register: { store.locatorField = $0 },
                    began: {},
                    change: { value in
                        store.changeDetails(item, to: details.with(locator: Locator(term: term, value: value)))
                    }
                )
                .frame(width: 120)
            } label: {
                // The locator's kind is the row's label, so "Page" is said once.
                Picker(PickerWords.text(.locatorType), selection: Binding(
                    get: { term },
                    set: { next in
                        store.changeDetails(item, to: details.with(locator: Locator(term: next, value: details.locator?.value ?? "")))
                    }
                )) {
                    ForEach(options, id: \.term) { Text($0.name).tag($0.term) }
                }
                .labelsHidden()
                .fixedSize()
            }
            TextField(PickerWords.text(.before), text: Binding(
                get: { details.prefix ?? "" },
                set: { store.changeDetails(item, to: details.with(prefix: $0)) }
            ), prompt: Text(PickerWords.text(.beforeExample)))
            TextField(PickerWords.text(.after), text: Binding(
                get: { details.suffix ?? "" },
                set: { store.changeDetails(item, to: details.with(suffix: $0)) }
            ), prompt: Text(PickerWords.text(.afterExample)))
            let forms = formChoices(details)
            if forms.count > 1 {
                Picker(PickerWords.text(.showAs), selection: Binding(
                    get: { details.form ?? .normal },
                    set: { store.changeDetails(item, to: details.with(form: $0 == .normal ? nil : $0)) }
                )) {
                    ForEach(forms, id: \.form) { option in Text(option.name).tag(option.form) }
                }
            }
        }
        // One box: the popover is the group, so the form draws none of its own.
        .formStyle(.columns)
        .padding(16)
        .frame(width: 340)
        .fixedSize(horizontal: false, vertical: true)
        .onSubmit { store.closeEditing() }
    }

    /// Only forms this format writes for this reference, shown with the engine's names.
    private func formChoices(_ details: PickerDetails) -> [PickerForm] {
        let offered = store.state?.forms ?? []
        return (store.catalog?.forms ?? []).filter { option in
            guard offered.contains(option.form) else { return false }
            if option.form == (details.form ?? .normal) { return true }
            let example = store.formExamples.first { $0.form == option.form }
            return example.map { $0.reason == nil && !($0.example ?? "").isEmpty } ?? false
        }
    }
}

struct KeyCap: View {
    let key: String
    let grid: PickerGrid
    init(_ key: String, grid: PickerGrid) {
        self.key = key
        self.grid = grid
    }

    var body: some View {
        Text(key)
            .font(grid.font(11, .medium))
            .padding(.horizontal, 5 * grid.s)
            .padding(.vertical, 2 * grid.s)
            .background(RoundedRectangle(cornerRadius: 5 * grid.s, style: .continuous).fill(Color.primary.opacity(0.07)))
            .accessibilityHidden(true)
    }
}

/// The engine's rich runs as text: italics, small caps and superscripts kept.
enum CitationText {
    static func styled(_ display: DisplayTree, fallback: String, scale: CGFloat) -> AttributedString {
        let runs = display.nodes.flatMap(\.content)
        guard !runs.isEmpty else { return AttributedString(fallback) }
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

// MARK: Helpers

struct AnchorFrames: PreferenceKey {
    static let defaultValue: [PickerStore.Anchor: CGRect] = [:]
    static func reduce(value: inout [PickerStore.Anchor: CGRect], nextValue: () -> [PickerStore.Anchor: CGRect]) {
        value.merge(nextValue()) { $1 }
    }
}

private extension View {
    /// Records where a control sits, so its native menu opens at it.
    func anchor(_ anchor: PickerStore.Anchor) -> some View {
        background(GeometryReader { geometry in
            Color.clear.preference(key: AnchorFrames.self, value: [anchor: geometry.frame(in: .global)])
        })
    }
}

/// Lets the header move the window, as a title bar would.
private struct WindowDragArea: NSViewRepresentable {
    final class Area: NSView {
        override var mouseDownCanMoveWindow: Bool { false }
        override func mouseDown(with event: NSEvent) { window?.performDrag(with: event) }
    }

    func makeNSView(context: Context) -> Area { Area() }
    func updateNSView(_ view: Area, context: Context) {}
}
