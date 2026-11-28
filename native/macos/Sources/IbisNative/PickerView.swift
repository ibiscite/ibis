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
    var heading: CGFloat { 26 * s }
    var footer: CGFloat { 40 * s }
    var status: CGFloat { 30 * s }
    var editor: CGFloat { 40 * s }
    var empty: CGFloat { 220 * s }
    func row(_ density: Density) -> CGFloat { (density == .compact ? 44 : 52) * s }
    func font(_ size: CGFloat, _ weight: Font.Weight = .regular) -> Font { .system(size: size * s, weight: weight) }
}

struct PickerView: View {
    @ObservedObject var model: PickerModel
    @ObservedObject var controller: PickerController

    var body: some View {
        let grid = PickerGrid(s: model.scale)
        VStack(spacing: 0) {
            SearchHeader(model: model, controller: controller, grid: grid)
            if let state = controller.emptyState {
                Divider()
                EmptyStateView(state: state, controller: controller, grid: grid).frame(height: grid.empty)
            } else if controller.showsList {
                Divider()
                ResultsList(model: model, controller: controller, grid: grid)
                    .frame(height: controller.listHeight(grid))
            }
            if let problem = controller.problem {
                StatusLine(text: problem.text, status: problem.status, controller: controller, grid: grid)
            }
            if controller.showsFooter {
                Divider()
                Footer(model: model, controller: controller, grid: grid)
            }
            Spacer(minLength: 0)
        }
        .onPreferenceChange(AnchorFrames.self) { controller.anchors = $0 }
    }
}

// MARK: Search

private struct SearchHeader: View {
    @ObservedObject var model: PickerModel
    @ObservedObject var controller: PickerController
    let grid: PickerGrid

    var body: some View {
        HStack(spacing: 8 * grid.s) {
            BrandMark()
                .frame(width: 24 * grid.s, height: 24 * grid.s)
                .frame(width: grid.lead - 8 * grid.s)
            if model.shorthand {
                Text(PickerWords.text(.citation))
                    .font(grid.font(13, .medium))
                    .foregroundStyle(Color.accentColor)
                    .padding(.horizontal, 8 * grid.s)
                    .padding(.vertical, 3 * grid.s)
                    .background(Capsule().fill(Color.accentColor.opacity(0.14)))
            }
            NativeTextField(
                text: model.query,
                placeholder: PickerWords.text(model.shorthand ? .shorthandExample : .search),
                label: PickerWords.text(.search),
                style: .plain,
                font: .systemFont(ofSize: 22 * grid.s),
                register: { controller.searchField = $0 },
                began: {},
                change: { model.query = $0 }
            )
            if model.isOpening {
                ProgressView().controlSize(.small)
            }
        }
        .padding(.trailing, grid.trailing)
        .frame(height: grid.header)
        .background(WindowDragArea())
    }
}

/// The bird, from the brand.
struct BrandMark: View {
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        if let image = PickerArt.image(scheme == .dark ? "pickerMark-dark" : "pickerMark") {
            Image(nsImage: image).resizable().interpolation(.high).aspectRatio(contentMode: .fit)
                .accessibilityHidden(true)
        }
    }
}

enum PickerArt {
    private static let images: [String: NSImage] = {
        guard let folder = Bundle.main.resourceURL?.appendingPathComponent("macos-picker"),
              let urls = try? FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil)
        else { return [:] }
        return Dictionary(uniqueKeysWithValues: urls.compactMap { url in
            guard url.pathExtension == "pdf", let image = NSImage(contentsOf: url) else { return nil }
            let name = url.deletingPathExtension().lastPathComponent
            image.isTemplate = !name.hasPrefix("pickerMark")
            return (name, image)
        })
    }()

    static func image(_ name: String) -> NSImage? { images[name] }
}

// MARK: Results

private struct RowFrames: PreferenceKey {
    static let defaultValue: [ItemRef: CGRect] = [:]
    static func reduce(value: inout [ItemRef: CGRect], nextValue: () -> [ItemRef: CGRect]) {
        value.merge(nextValue()) { $1 }
    }
}

private struct ResultsList: View {
    @ObservedObject var model: PickerModel
    @ObservedObject var controller: PickerController
    let grid: PickerGrid

    var body: some View {
        let added = Set(controller.addedOrder)
        GeometryReader { viewport in
            ScrollViewReader { scroll in
                ScrollView(.vertical) {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        if model.hasDraft {
                            SectionHeading(title: PickerWords.text(.cited), grid: grid)
                            ForEach(model.citations, id: \.row.id) { entry in
                                addedRow(entry.row)
                            }
                        }
                        ForEach(Array(model.sections.enumerated()), id: \.offset) { _, section in
                            SectionHeading(title: section.kind.label, grid: grid)
                            ForEach(section.rows, id: \.id) { row in
                                resultRow(row, added: added.contains(row.id))
                            }
                        }
                    }
                    .padding(.vertical, 6 * grid.s)
                }
                .scrollIndicators(.automatic)
                .coordinateSpace(name: "results")
                .onPreferenceChange(RowFrames.self) { frames in
                    controller.visibleRows = frames
                        .filter { $0.value.minY >= -0.5 && $0.value.maxY <= viewport.size.height + 0.5 }
                        .sorted { $0.value.minY < $1.value.minY }
                        .map(\.key)
                }
                .onChange(of: controller.revealedRow) {
                    if let target = controller.pinned.map({ "added-\($0.key)-\($0.library)" }) {
                        scroll.scrollTo(target)
                    } else if let selected = model.selected {
                        scroll.scrollTo(selected)
                    }
                }
            }
        }
        .accessibilityLabel(PickerWords.text(.references))
    }

    private func addedRow(_ row: PickerRow) -> some View {
        let selected = controller.pinned == row.id
        return VStack(spacing: 0) {
            ResultRow(row: row, selected: selected, emphasized: selected && controller.isKey && controller.editing == nil,
                      added: true, note: controller.locatorLabel(for: row.id), number: nil,
                      density: model.density, grid: grid,
                      actions: RowActions(remove: { controller.remove(row.id) }))
                .onTapGesture { controller.edit(row.id) }
                .contextMenu { rowMenu(row, added: true) }
            if controller.editing == row.id { DetailsEditor(model: model, controller: controller, item: row.id, grid: grid) }
        }
        .id("added-\(row.id.key)-\(row.id.library)")
    }

    private func resultRow(_ row: PickerRow, added: Bool) -> some View {
        let selected = row.id == model.selected && controller.pinned == nil
        return VStack(spacing: 0) {
            ResultRow(row: row, selected: selected, emphasized: selected && controller.resultEmphasis,
                      added: added, note: nil, number: controller.number(for: row.id),
                      density: model.density, grid: grid,
                      actions: RowActions(add: added ? nil : { add(row) }, peek: { controller.togglePeek(row.id) }))
                .background(GeometryReader { geometry in
                    Color.clear.preference(key: RowFrames.self, value: [row.id: geometry.frame(in: .named("results"))])
                })
                .onTapGesture { click(row) }
                .contextMenu { rowMenu(row, added: added) }
                .popover(isPresented: Binding(
                    get: { controller.peek == row.id },
                    set: { if !$0, controller.peek == row.id { controller.peekClosed() } }
                ), arrowEdge: .trailing) {
                    PeekView(row: row)
                }
            if !model.hasDraft, controller.editing == row.id {
                DetailsEditor(model: model, controller: controller, item: row.id, grid: grid)
            }
        }
        .id(row.id)
    }

    @ViewBuilder private func rowMenu(_ row: PickerRow, added: Bool) -> some View {
        Button(model.deliveryTitle) {
            model.select(row.id)
            controller.panel?.submit(whenReady: true)
        }
        if added {
            Button(PickerWords.text(.page)) { controller.edit(row.id) }
            Button(PickerWords.text(.remove)) { controller.remove(row.id) }
        } else {
            Button(PickerWords.text(.add)) { add(row) }
        }
        Button(PickerWords.text(.peek)) { controller.togglePeek(row.id) }
    }

    private func add(_ row: PickerRow) {
        model.select(row.id)
        if !model.isSelectionStaged { model.toggleCitation(row) }
        controller.focusSearch(caretAtEnd: true)
    }

    private func click(_ row: PickerRow) {
        let event = NSApp.currentEvent
        if event?.modifierFlags.contains(.command) == true {
            model.select(row.id)
            model.toggleCitation(row)
        } else if (event?.clickCount ?? 1) >= 2, row.id == model.selected {
            controller.panel?.submit(whenReady: true)
            return
        } else {
            model.select(row.id)
        }
        controller.focusSearch(caretAtEnd: true)
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
    var add: (() -> Void)?
    var remove: (() -> Void)?
    var peek: (() -> Void)?
}

struct ResultRow: View {
    let row: PickerRow
    let selected: Bool
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
                    Text("\(number)").font(grid.font(13, .medium)).monospacedDigit().foregroundStyle(quiet)
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
                    if let add = actions.add { HoverButton(symbol: "plus", label: .add, ink: ink, grid: grid, perform: add) }
                    if let peek = actions.peek { HoverButton(symbol: "eye", label: .peek, ink: ink, grid: grid, perform: peek) }
                    if let remove = actions.remove {
                        HoverButton(symbol: "xmark", label: .remove, ink: ink, grid: grid, perform: remove)
                    }
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
        .background(
            RoundedRectangle(cornerRadius: 10 * grid.s, style: .continuous)
                .fill(fill)
        )
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
        .accessibilityAddTraits(selected ? [.isSelected, .isButton] : .isButton)
        .accessibilityActions {
            if let add = actions.add { Button(PickerWords.text(.add), action: add) }
            if let remove = actions.remove { Button(PickerWords.text(.remove), action: remove) }
            if let peek = actions.peek { Button(PickerWords.text(.peek), action: peek) }
        }
    }

    private var fill: Color {
        if emphasized { return Color(nsColor: .selectedContentBackgroundColor) }
        if selected { return Color(nsColor: .unemphasizedSelectedContentBackgroundColor) }
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

private struct PeekView: View {
    let row: PickerRow

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Label(row.kindLabel, systemImage: row.kind.symbol).font(.caption).foregroundStyle(.secondary)
            Text(row.title ?? PickerWords.text(.untitled)).font(.title3.weight(.semibold))
            Text(row.fullByline).foregroundStyle(.secondary)
            if let journal = row.journal { Text(journal).italic().foregroundStyle(.secondary) }
            if let key = row.citationKey {
                Text(key).font(.callout.monospaced()).foregroundStyle(.secondary).padding(.top, 4)
            }
        }
        .textSelection(.enabled)
        .fixedSize(horizontal: false, vertical: true)
        .frame(width: 340, alignment: .leading)
        .padding(16)
    }
}

// MARK: Editing in place

/// The page, prefix and suffix of one reference, opened inside its row.
private struct DetailsEditor: View {
    @ObservedObject var model: PickerModel
    @ObservedObject var controller: PickerController
    let item: ItemRef
    let grid: PickerGrid

    var body: some View {
        let details = model.details(for: item)
        let options = model.catalog?.locators ?? []
        let term = options.first { $0.term == details.term }
        let name = term?.name ?? PickerWords.text(.page)
        let choices = self.choices(details)
        HStack(spacing: 8 * grid.s) {
            Menu {
                ForEach(options, id: \.term) { option in
                    Button {
                        model.changeDetails(for: item) { $0.term = option.term }
                    } label: {
                        if option.term == details.term { Label(option.name, systemImage: "checkmark") } else { Text(option.name) }
                    }
                }
            } label: {
                Text(term?.shortName ?? PickerWords.text(.pageShort))
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
            .disabled(options.isEmpty)
            .help(name)
            .accessibilityLabel(PickerWords.text(.locatorType))
            NativeTextField(
                text: details.locator, placeholder: name, label: name, style: .rounded,
                font: .systemFont(ofSize: 13 * grid.s),
                register: { controller.locatorField = $0 },
                began: {},
                change: { text in model.changeDetails(for: item) { $0.locator = text } }
            )
            .frame(width: 64 * grid.s)
            TextField(PickerWords.text(.before), text: binding(\.prefix), prompt: Text(PickerWords.text(.beforeExample)))
                .textFieldStyle(.roundedBorder)
                .frame(width: 96 * grid.s)
            TextField(PickerWords.text(.after), text: binding(\.suffix), prompt: Text(PickerWords.text(.after)))
                .textFieldStyle(.roundedBorder)
                .frame(width: 96 * grid.s)
            if choices.count > 1 {
                Picker(PickerWords.text(.showAs), selection: Binding(
                    get: { details.form ?? .normal },
                    set: { form in model.changeDetails(for: item) { $0.form = form } }
                )) {
                    ForEach(choices) { choice in Text(choice.form.name).tag(choice.form.form) }
                }
                .labelsHidden()
                .fixedSize()
                .help(PickerWords.text(.showAs))
            }
            Spacer(minLength: 0)
        }
        .font(grid.font(13))
        .controlSize(grid.s >= 1.4 ? .regular : .small)
        .padding(.leading, 58 * grid.s)
        .padding(.trailing, 20 * grid.s)
        .frame(height: grid.editor)
    }

    private func binding(_ field: WritableKeyPath<CitationDetails, String>) -> Binding<String> {
        Binding(get: { model.details(for: item)[keyPath: field] },
                set: { text in model.changeDetails(for: item) { $0[keyPath: field] = text } })
    }

    private struct Choice: Identifiable {
        let form: PickerForm
        var id: CitationForm { form.form }
    }

    private func choices(_ details: CitationDetails) -> [Choice] {
        let selected = details.form ?? .normal
        return [CitationForm.normal, .narrative, .yearOnly].compactMap { choice in
            guard let option = model.forms.first(where: { $0.form == choice }) else { return nil }
            let example = model.formExamples.first { $0.form == choice }
            let usable = example.map { $0.reason == nil && !($0.example ?? "").isEmpty } ?? false
            return usable || selected == choice ? Choice(form: option) : nil
        }
    }
}

// MARK: Status and empty states

private struct StatusLine: View {
    let text: String
    let status: PickerStatus?
    let controller: PickerController
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
                Button(controller.fixTitle(fix)) { controller.runFix(status) }
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
    let state: PickerController.EmptyState
    let controller: PickerController
    let grid: PickerGrid

    var body: some View {
        let content = self.content
        ContentUnavailableView {
            Label(content.title, systemImage: content.symbol)
        } description: {
            Text(PickerWords.text(content.hint))
        } actions: {
            if let primary = content.primary {
                Button(primary) { controller.emptyAction(primary: true) }
                    .buttonStyle(.borderedProminent)
            }
            if let secondary = content.secondary {
                Button(secondary) { controller.emptyAction(primary: false) }.buttonStyle(.link)
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
    @ObservedObject var model: PickerModel
    @ObservedObject var controller: PickerController
    let grid: PickerGrid

    var body: some View {
        HStack(spacing: 12 * grid.s) {
            Button { controller.openFormatMenu() } label: {
                HStack(spacing: 3 * grid.s) {
                    Text(model.formatName).lineLimit(1)
                    Image(systemName: "chevron.up.chevron.down").font(grid.font(8, .semibold))
                }
                .frame(maxWidth: 140 * grid.s)
                .fixedSize()
            }
            .buttonStyle(.borderless)
            .foregroundStyle(.primary)
            .disabled(!model.canChooseFormat)
            .help(model.formatContext.isEmpty ? model.formatName : model.formatContext)
            .accessibilityLabel(PickerWords.text(.formatValue, model.formatName))
            .accessibilityHint(model.formatContext)
            .anchor(.format)

            preview
                .frame(maxWidth: .infinity, alignment: .leading)

            Button { controller.panel?.submit(whenReady: true) } label: {
                HStack(spacing: 5 * grid.s) {
                    Text(model.deliveryTitle).lineLimit(1)
                    KeyCap("↩", grid: grid)
                }
            }
            .buttonStyle(.borderless)
            .foregroundStyle(.primary)
            .fontWeight(.medium)
            .fixedSize()
            .disabled(!model.canDeliver)
            .accessibilityLabel(model.deliveryTitle)
            .accessibilityHint(PickerWords.text(.returnKey))

            Button { controller.openActionsMenu() } label: {
                HStack(spacing: 5 * grid.s) {
                    Text(PickerWords.text(.more))
                    KeyCap("⌘K", grid: grid)
                }
            }
            .buttonStyle(.borderless)
            .foregroundStyle(.secondary)
            .fixedSize()
            .accessibilityLabel(PickerWords.text(.actionsTitle))
            .anchor(.actions)
        }
        .font(grid.font(12))
        .padding(.leading, 18 * grid.s)
        .padding(.trailing, 14 * grid.s)
        .frame(height: grid.footer)
    }

    @ViewBuilder private var preview: some View {
        let parts = CitationLine.parts(model, scale: grid.s * 0.92)
        if parts.isEmpty {
            Text(model.isFormatting ? PickerWords.text(.formatting) : "").foregroundStyle(.tertiary)
        } else {
            Text(parts.reduce(into: AttributedString()) { line, part in line += part.text })
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
                .opacity(model.canDeliver ? 1 : 0.6)
                .help(parts.map(\.plain).joined())
                .accessibilityLabel(PickerWords.text(.token, parts.map(\.plain).joined()))
        }
    }
}

private struct KeyCap: View {
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

// MARK: Helpers

struct AnchorFrames: PreferenceKey {
    static let defaultValue: [PickerController.Anchor: CGRect] = [:]
    static func reduce(value: inout [PickerController.Anchor: CGRect],
                       nextValue: () -> [PickerController.Anchor: CGRect]) {
        value.merge(nextValue()) { $1 }
    }
}

private extension View {
    /// Records where a control sits, so its native menu opens at it.
    func anchor(_ anchor: PickerController.Anchor) -> some View {
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
