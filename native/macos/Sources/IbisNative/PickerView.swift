import AppKit
import SwiftUI

// The picker's content: search, results and the delivery bar, laid out with
// the system's own controls. Every size is a multiple of the picker's scale.

private struct PickerScaleKey: EnvironmentKey { static let defaultValue: CGFloat = 1 }

extension EnvironmentValues {
    var pickerScale: CGFloat {
        get { self[PickerScaleKey.self] }
        set { self[PickerScaleKey.self] = newValue }
    }
}

/// The picker's column grid: the lead column holds glyphs, text starts after it.
struct PickerGrid {
    let s: CGFloat
    var lead: CGFloat { 48 * s }
    var trailing: CGFloat { 16 * s }
    var header: CGFloat { 56 * s }
    var heading: CGFloat { 28 * s }
    func row(_ density: Density) -> CGFloat { (density == .compact ? 38 : 50) * s }
    func font(_ size: CGFloat, _ weight: Font.Weight = .regular) -> Font { .system(size: size * s, weight: weight) }
}

struct PickerView: View {
    @ObservedObject var model: PickerModel
    @ObservedObject var controller: PickerController

    var body: some View {
        let grid = PickerGrid(s: model.scale)
        GeometryReader { window in
            let stacked = window.size.width - 560 * grid.s < 150 * grid.s
            VStack(spacing: 0) {
                SearchHeader(model: model, controller: controller, grid: grid)
                Divider()
                HeadingLine(model: model, controller: controller, grid: grid)
                ZStack {
                    ResultsList(model: model, controller: controller, grid: grid)
                    if let state = controller.emptyState {
                        EmptyStateView(state: state, controller: controller, grid: grid)
                    }
                }
                .frame(maxHeight: .infinity)
                if model.libraryReady && !model.isOpening {
                    Divider()
                    DeliveryBar(model: model, controller: controller, grid: grid, stacked: stacked)
                }
            }
            .overlay(alignment: .bottomTrailing) {
                if controller.actionsOpen {
                    ActionsPanel(controller: controller, grid: grid)
                        .padding(.trailing, 12 * grid.s)
                        .padding(.bottom, (stacked ? 92 : 52) * grid.s + 6 * grid.s)
                        .transition(.opacity)
                }
            }
        }
        .environment(\.pickerScale, grid.s)
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
                began: { controller.fieldBegan(.search) },
                change: { model.query = $0 }
            )
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

// MARK: Heading

private struct HeadingLine: View {
    @ObservedObject var model: PickerModel
    @ObservedObject var controller: PickerController
    let grid: PickerGrid

    var body: some View {
        let heading = controller.heading
        let library = model.libraryReady && !model.isOpening ? model.catalog?.library : nil
        if heading != nil || library != nil {
            HStack(spacing: 6 * grid.s) {
                Group {
                    if model.isOpening {
                        ProgressView().controlSize(.small).scaleEffect(grid.s)
                    } else if heading?.isProblem == true {
                        Image(systemName: heading?.status?.kind == .citation ? "xmark.octagon.fill" : "exclamationmark.triangle.fill")
                            .font(grid.font(12))
                            .symbolRenderingMode(.multicolor)
                    }
                }
                .frame(width: grid.lead - 6 * grid.s)
                if let heading {
                    Text(heading.text)
                        .font(grid.font(12, .medium))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .help(heading.isProblem ? heading.text : "")
                    if let count = heading.count {
                        Text(count).font(grid.font(11)).foregroundStyle(.tertiary).monospacedDigit()
                    }
                }
                Spacer(minLength: 8 * grid.s)
                if let status = heading?.status, let fix = status.fix {
                    Button(controller.fixTitle(fix)) { controller.runFix(status) }
                        .buttonStyle(.link)
                        .font(grid.font(12))
                }
                if let library {
                    LibraryButton(library: library, controller: controller, grid: grid,
                                  showsName: heading?.isProblem != true)
                }
            }
            .padding(.trailing, grid.trailing)
            .frame(height: grid.heading)
        }
    }
}

private struct LibraryButton: View {
    let library: PickerLibrary
    let controller: PickerController
    let grid: PickerGrid
    let showsName: Bool

    var body: some View {
        Button { controller.openLibraryMenu() } label: {
            ViewThatFits(in: .horizontal) {
                if showsName {
                    HStack(spacing: 4 * grid.s) {
                        Image(systemName: "books.vertical")
                        Text(library.name).lineLimit(1).truncationMode(.middle).frame(maxWidth: 160 * grid.s)
                        Image(systemName: "chevron.down").font(grid.font(8, .semibold))
                    }
                }
                Image(systemName: "books.vertical")
            }
            .font(grid.font(12))
            .foregroundStyle(.secondary)
        }
        .buttonStyle(.borderless)
        .help(library.name)
        .accessibilityLabel(PickerWords.text(.scopeLabel, library.name))
        .anchor(.library)
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
    @ObservedObject var model: PickerModel
    @ObservedObject var controller: PickerController
    let grid: PickerGrid

    var body: some View {
        let cited = Set(model.citations.map(\.row.id))
        GeometryReader { viewport in
            ScrollViewReader { scroll in
                ScrollView(.vertical) {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        ForEach(Array(model.sections.enumerated()), id: \.offset) { index, section in
                            if index > 0 {
                                Text(section.kind.label)
                                    .font(grid.font(12, .medium))
                                    .foregroundStyle(.secondary)
                                    .padding(.leading, grid.lead)
                                    .frame(height: grid.heading, alignment: .bottom)
                                    .accessibilityAddTraits(.isHeader)
                            }
                            ForEach(section.rows, id: \.id) { row in
                                rowView(row, cited: cited.contains(row.id))
                            }
                        }
                    }
                    .padding(.vertical, 4 * grid.s)
                }
                .coordinateSpace(name: "results")
                .onPreferenceChange(RowFrames.self) { frames in
                    controller.visibleRows = frames
                        .filter { $0.value.minY >= -0.5 && $0.value.maxY <= viewport.size.height + 0.5 }
                        .sorted { $0.value.minY < $1.value.minY }
                        .map(\.key)
                }
                .onChange(of: controller.revealedRow) { _ in
                    if let selected = model.selected { scroll.scrollTo(selected) }
                }
            }
        }
        .accessibilityLabel(PickerWords.text(.references))
    }

    private func rowView(_ row: PickerRow, cited: Bool) -> some View {
        let selected = row.id == model.selected
        return ResultRow(row: row, selected: selected, emphasized: selected && controller.quickDelivery,
                         cited: cited, number: controller.number(for: row.id), density: model.density, grid: grid)
            .id(row.id)
            .background(GeometryReader { geometry in
                Color.clear.preference(key: RowFrames.self, value: [row.id: geometry.frame(in: .named("results"))])
            })
            .onTapGesture { click(row) }
            .popover(isPresented: Binding(
                get: { controller.layer == .peek(row.id) },
                set: { if !$0, controller.layer == .peek(row.id) { controller.layerClosed() } }
            ), arrowEdge: .trailing) {
                PeekView(row: row)
            }
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

struct ResultRow: View {
    let row: PickerRow
    let selected: Bool
    let emphasized: Bool
    let cited: Bool
    let number: Int?
    let density: Density
    let grid: PickerGrid
    @State private var hovered = false

    var body: some View {
        let ink = emphasized ? Color(nsColor: .alternateSelectedControlTextColor) : Color(nsColor: .labelColor)
        let quiet = emphasized ? ink.opacity(0.85) : Color(nsColor: .secondaryLabelColor)
        HStack(spacing: 0) {
            Group {
                if let number {
                    Text("\(number)").font(grid.font(13, .medium)).monospacedDigit().foregroundStyle(quiet)
                } else {
                    Image(systemName: cited ? "checkmark.circle.fill" : row.kind.symbol)
                        .font(grid.font(15))
                        .foregroundStyle(cited && !emphasized ? Color.accentColor : quiet)
                }
            }
            .frame(width: grid.lead)
            VStack(alignment: .leading, spacing: (density == .compact ? 1 : 3) * grid.s) {
                HStack(alignment: .firstTextBaseline, spacing: 12 * grid.s) {
                    Text(row.title ?? PickerWords.text(.untitled))
                        .font(grid.font(14))
                        .foregroundStyle(ink)
                        .lineLimit(1)
                    Spacer(minLength: 0)
                    Text(row.year ?? PickerWords.text(.noDate))
                        .font(grid.font(12))
                        .monospacedDigit()
                        .foregroundStyle(quiet)
                        .fixedSize()
                }
                HStack(spacing: 8 * grid.s) {
                    Byline(options: row.bylines)
                    if !row.matchReasons.isEmpty {
                        Text(row.matchReasons.joined(separator: " · "))
                            .italic()
                            .lineLimit(1)
                            .layoutPriority(1)
                    }
                }
                .font(grid.font(12))
                .foregroundStyle(quiet)
            }
            .padding(.trailing, grid.trailing)
        }
        .frame(height: grid.row(density))
        .background(
            RoundedRectangle(cornerRadius: 8 * grid.s, style: .continuous)
                .fill(fill)
                .padding(.horizontal, 8 * grid.s)
                .padding(.vertical, 1 * grid.s)
        )
        .contentShape(Rectangle())
        .onHover { hovered = $0 }
        .help(([row.title ?? "", row.fullByline] + row.matchReasons + [row.citationKey ?? ""])
            .filter { !$0.isEmpty }.joined(separator: "\n"))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(([row.title ?? PickerWords.text(.untitled), row.fullByline, row.kindLabel]
            + row.matchReasons + (cited ? [PickerWords.text(.cited)] : []) + [row.citationKey].compactMap { $0 })
            .joined(separator: ", "))
        .accessibilityHint(number.map { "⌘\($0)" } ?? "")
        .accessibilityAddTraits(selected ? [.isSelected, .isButton] : .isButton)
    }

    private var fill: Color {
        if emphasized { return Color(nsColor: .selectedContentBackgroundColor) }
        if selected { return Color(nsColor: .unemphasizedSelectedContentBackgroundColor) }
        return hovered ? Color.primary.opacity(0.05) : .clear
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
        VStack(alignment: .leading, spacing: 5) {
            Text(row.title ?? PickerWords.text(.untitled)).font(.headline)
            Text(row.fullByline).foregroundStyle(.secondary)
            Text([row.kindLabel, row.citationKey].compactMap { $0 }.joined(separator: " · "))
                .foregroundStyle(.secondary)
        }
        .textSelection(.enabled)
        .fixedSize(horizontal: false, vertical: true)
        .frame(width: 320, alignment: .leading)
        .padding(14)
    }
}

// MARK: Empty states

private struct EmptyStateView: View {
    let state: PickerController.EmptyState
    let controller: PickerController
    let grid: PickerGrid

    var body: some View {
        let content = self.content
        VStack(spacing: 6 * grid.s) {
            Image(systemName: content.symbol)
                .font(grid.font(30, .light))
                .foregroundStyle(.tertiary)
                .padding(.bottom, 4 * grid.s)
            Text(content.title).font(grid.font(15, .semibold)).multilineTextAlignment(.center).lineLimit(2)
            Text(PickerWords.text(content.hint)).font(grid.font(13)).foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
            if content.primary != nil || content.secondary != nil {
                HStack(spacing: 12 * grid.s) {
                    if let primary = content.primary {
                        Button { controller.emptyAction(primary: true) } label: {
                            HStack(spacing: 6 * grid.s) {
                                Text(primary)
                                if content.returnKey { KeyHint("↩") }
                            }
                        }
                        .buttonStyle(.borderedProminent)
                    }
                    if let secondary = content.secondary {
                        Button(secondary) { controller.emptyAction(primary: false) }.buttonStyle(.link)
                    }
                }
                .font(grid.font(13))
                .controlSize(grid.s >= 1.4 ? .large : .regular)
                .padding(.top, 8 * grid.s)
            }
        }
        .padding(.horizontal, 32 * grid.s)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var content: (symbol: String, title: String, hint: PickerWord, primary: String?, secondary: String?, returnKey: Bool) {
        switch state {
        case .newLibrary:
            ("books.vertical", PickerWords.text(.emptyLibrary), .emptyLibraryHint, nil, nil, false)
        case .noMatches:
            ("magnifyingglass", PickerWords.text(.noMatches), .noMatchesHint, PickerWords.text(.clear), nil, false)
        case .choose:
            ("books.vertical", PickerWords.text(.chooseTitle), .chooseHint,
             PickerWords.text(.choose), PickerWords.text(.settings), true)
        case let .missing(title, name):
            ("questionmark.folder", title, .missingHint, PickerWords.text(.findLibrary, name), PickerWords.text(.choose), true)
        case let .unreadable(problem):
            ("exclamationmark.triangle", problem, .chooseHint, PickerWords.text(.choose), nil, true)
        }
    }
}

// MARK: Delivery bar

private struct DeliveryBar: View {
    @ObservedObject var model: PickerModel
    @ObservedObject var controller: PickerController
    let grid: PickerGrid
    let stacked: Bool

    var body: some View {
        Group {
            if stacked {
                VStack(spacing: 6 * grid.s) {
                    citation
                    HStack { Spacer(); commands }
                }
            } else {
                HStack(spacing: 8 * grid.s) {
                    citation
                    commands
                }
            }
        }
        .padding(.trailing, grid.trailing)
        .padding(.vertical, 10 * grid.s)
        .frame(height: (stacked ? 92 : 52) * grid.s)
        .font(grid.font(13))
        .controlSize(grid.s >= 1.4 ? .large : .regular)
    }

    private var citation: some View {
        HStack(spacing: 8 * grid.s) {
            Group {
                if let beak = PickerArt.image("ibis-quote-beak") {
                    Image(nsImage: beak).resizable().aspectRatio(contentMode: .fit)
                        .frame(width: 18 * grid.s, height: 18 * grid.s)
                        .foregroundStyle(Color.accentColor)
                        .accessibilityHidden(true)
                }
            }
            .frame(width: grid.lead - 8 * grid.s)
            CitationTokens(model: model, controller: controller, grid: grid)
            LocatorField(model: model, controller: controller, grid: grid)
            Button { controller.openFormatMenu() } label: {
                HStack(spacing: 3 * grid.s) {
                    Text(model.formatName).lineLimit(1)
                    Image(systemName: "chevron.up.chevron.down").font(grid.font(9, .semibold))
                }
                .frame(maxWidth: 120 * grid.s)
            }
            .buttonStyle(.borderless)
            .disabled(!model.canChooseFormat || controller.actionsOpen)
            .help(model.formatContext.isEmpty ? model.formatName : model.formatContext)
            .accessibilityLabel(PickerWords.text(.formatValue, model.formatName))
            .accessibilityHint(model.formatContext)
            .anchor(.format)
        }
    }

    private var commands: some View {
        HStack(spacing: 8 * grid.s) {
            let staged = model.isSelectionStaged
            Button { controller.runCommand(.membership) } label: {
                Image(systemName: staged ? "minus.circle" : "plus.circle").font(grid.font(15))
            }
            .buttonStyle(.borderless)
            .disabled(!model.canChangeSelectionMembership || controller.actionsOpen)
            .help(PickerWords.text(staged ? .remove : .add) + "  ⇧↩")
            .accessibilityLabel(PickerWords.text(staged ? .remove : .add))

            deliver
                .disabled(!model.canDeliver || controller.actionsOpen)
                .help(model.deliveryTitle)
                .accessibilityLabel(model.deliveryTitle)
                .accessibilityHint(PickerWords.text(.returnKey))

            Button { controller.toggleActions() } label: {
                HStack(spacing: 4 * grid.s) {
                    Text(PickerWords.text(.more))
                    KeyHint("⌘K")
                }
            }
            .buttonStyle(.borderless)
            .accessibilityLabel(PickerWords.text(.more))
            .anchor(.more)
        }
        .fixedSize()
    }

    @ViewBuilder private var deliver: some View {
        let button = Button { controller.panel?.submit() } label: {
            HStack(spacing: 6 * grid.s) {
                Text(model.deliveryTitle).lineLimit(1)
                KeyHint("↩")
            }
        }
        // A draft moves the emphasis from the highlighted row to the delivery.
        if model.hasDraft { button.buttonStyle(.borderedProminent) } else { button.buttonStyle(.bordered) }
    }
}

private struct KeyHint: View {
    let key: String
    init(_ key: String) { self.key = key }
    var body: some View { Text(key).opacity(0.6).accessibilityHidden(true) }
}

private struct CitationTokens: View {
    @ObservedObject var model: PickerModel
    @ObservedObject var controller: PickerController
    let grid: PickerGrid

    var body: some View {
        let parts = CitationLine.parts(model, scale: grid.s)
        let active = model.hasDraft ? model.citationItem : nil
        ScrollViewReader { scroll in
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 0) {
                    if parts.isEmpty, model.isFormatting {
                        Text(PickerWords.text(.formatting)).foregroundStyle(.secondary)
                    }
                    ForEach(parts) { part in
                        if model.hasDraft, let item = part.item {
                            let first = parts.first { $0.item == item }?.id == part.id
                            CitationToken(part: part, item: item, first: first, active: item == active,
                                          model: model, controller: controller, grid: grid)
                                .id(first ? AnyHashable(item) : AnyHashable(part.id))
                        } else {
                            Text(part.text)
                                .foregroundStyle(model.hasDraft ? Color.secondary : Color.primary)
                                .lineLimit(1)
                                .fixedSize()
                        }
                    }
                }
                .frame(minHeight: 24 * grid.s)
            }
            .opacity(model.canDeliver || model.editingCitation != nil ? 1 : 0.55)
            .onChange(of: active) { item in
                if let item { withAnimation(.easeOut(duration: 0.15)) { scroll.scrollTo(item) } }
            }
        }
        .frame(minWidth: 80 * grid.s, maxWidth: .infinity, alignment: .leading)
    }
}

private struct CitationToken: View {
    let part: CitationLine.Part
    let item: ItemRef
    let first: Bool
    let active: Bool
    @ObservedObject var model: PickerModel
    @ObservedObject var controller: PickerController
    let grid: PickerGrid
    @State private var hovered = false

    var body: some View {
        let focused = controller.focus == .token(item)
        HStack(spacing: 4 * grid.s) {
            Text(part.text).lineLimit(1).frame(maxWidth: 260 * grid.s, alignment: .leading)
            Button { controller.removeToken(item) } label: {
                Image(systemName: "xmark.circle.fill").font(grid.font(11))
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            .opacity(hovered || focused ? 1 : 0)
            .accessibilityLabel(PickerWords.text(.remove))
        }
        .padding(.leading, 7 * grid.s)
        .padding(.trailing, 4 * grid.s)
        .frame(height: 22 * grid.s)
        .background(RoundedRectangle(cornerRadius: 6 * grid.s, style: .continuous)
            .fill(Color.accentColor.opacity(active ? 0.24 : 0.12)))
        .overlay(RoundedRectangle(cornerRadius: 6 * grid.s, style: .continuous)
            .strokeBorder(Color(nsColor: .keyboardFocusIndicatorColor), lineWidth: focused ? 2.5 : 0))
        .contentShape(Rectangle())
        .onHover { hovered = $0 }
        .onTapGesture { controller.showDetails(item) }
        .contextMenu {
            Button(PickerWords.text(.details)) { controller.showDetails(item) }
            Button(PickerWords.text(.remove)) { controller.removeToken(item) }
        }
        .help(([part.plain] + part.notes).joined(separator: "\n"))
        .accessibilityElement(children: .combine)
        .accessibilityLabel(PickerWords.text(active ? .activeToken : .token, part.plain))
        .accessibilityHint(PickerWords.text(.removeToken, part.row?.title ?? part.plain))
        .accessibilityAddTraits(.isButton)
        .popover(isPresented: Binding(
            get: { first && controller.layer == .details(item) },
            set: { if !$0, controller.layer == .details(item) { controller.layerClosed() } }
        ), arrowEdge: .top) {
            CitationDetailsView(model: model, item: item)
        }
    }
}

private struct LocatorField: View {
    @ObservedObject var model: PickerModel
    @ObservedObject var controller: PickerController
    let grid: PickerGrid

    var body: some View {
        let item = model.citationItem
        let details = item.map(model.details(for:)) ?? CitationDetails()
        let options = model.catalog?.locators ?? []
        let term = options.first { $0.term == details.term }
        let name = term?.name ?? PickerWords.text(.page)
        HStack(spacing: 2 * grid.s) {
            Menu {
                ForEach(options, id: \.term) { option in
                    Button {
                        if let item { model.changeDetails(for: item) { $0.term = option.term } }
                    } label: {
                        if option.term == details.term { Label(option.name, systemImage: "checkmark") } else { Text(option.name) }
                    }
                }
            } label: {
                Text(term?.shortName ?? PickerWords.text(.pageShort))
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
            .disabled(item == nil || options.isEmpty)
            .help(name)
            .accessibilityLabel(PickerWords.text(.locatorType))
            NativeTextField(
                text: details.locator, placeholder: name, label: name, style: .rounded,
                font: .systemFont(ofSize: 13 * grid.s), isEnabled: item != nil,
                register: { controller.locatorField = $0 },
                began: { controller.fieldBegan(.locator) },
                change: { text in if let item { model.changeDetails(for: item) { $0.locator = text } } }
            )
            .frame(width: 64 * grid.s)
            .help(details.locator.isEmpty ? name : details.locator)
        }
    }
}

private struct CitationDetailsView: View {
    @ObservedObject var model: PickerModel
    let item: ItemRef

    var body: some View {
        let details = model.details(for: item)
        let choices = self.choices(details)
        Grid(alignment: .leading, horizontalSpacing: 10, verticalSpacing: 8) {
            GridRow {
                Text(PickerWords.text(.before)).foregroundStyle(.secondary).gridColumnAlignment(.trailing)
                TextField(PickerWords.text(.before), text: binding(\.prefix),
                          prompt: Text(PickerWords.text(.beforeExample)))
                    .labelsHidden()
                    .textFieldStyle(.roundedBorder)
            }
            GridRow {
                Text(PickerWords.text(.after)).foregroundStyle(.secondary)
                TextField(PickerWords.text(.after), text: binding(\.suffix)).labelsHidden().textFieldStyle(.roundedBorder)
            }
            GridRow {
                Text(PickerWords.text(.showAs)).foregroundStyle(.secondary)
                Picker(PickerWords.text(.showAs), selection: Binding(
                    get: { details.form ?? .normal },
                    set: { form in model.changeDetails(for: item) { $0.form = form } }
                )) {
                    ForEach(choices) { choice in
                        Text(choice.example.map { "\(choice.form.name)  ·  \($0)" } ?? choice.form.name)
                            .tag(choice.form.form)
                    }
                }
                .labelsHidden()
                .disabled(choices.count < 2)
            }
        }
        .frame(width: 320)
        .padding(14)
    }

    private func binding(_ field: WritableKeyPath<CitationDetails, String>) -> Binding<String> {
        Binding(get: { model.details(for: item)[keyPath: field] },
                set: { text in model.changeDetails(for: item) { $0[keyPath: field] = text } })
    }

    private struct Choice: Identifiable {
        let form: PickerForm
        let example: String?
        var id: CitationForm { form.form }
    }

    private func choices(_ details: CitationDetails) -> [Choice] {
        let selected = details.form ?? .normal
        let canReset = selected != .normal && model.preview?.problem?.fix == .editCitation
        return [CitationForm.normal, .narrative, .yearOnly].compactMap { choice in
            guard let option = model.forms.first(where: { $0.form == choice }) else { return nil }
            let example = model.formExamples.first { $0.form == choice }
            if let text = example?.example, example?.reason == nil, !text.isEmpty { return Choice(form: option, example: text) }
            return selected == choice || (canReset && choice == .normal) ? Choice(form: option, example: nil) : nil
        }
    }
}

// MARK: Actions

private struct ActionsPanel: View {
    @ObservedObject var controller: PickerController
    let grid: PickerGrid

    var body: some View {
        let actions = controller.actions
        let grouped = controller.actionsQuery.allSatisfy(\.isWhitespace)
        VStack(spacing: 0) {
            ScrollViewReader { scroll in
                ScrollView(.vertical) {
                    VStack(alignment: .leading, spacing: 0) {
                        if actions.isEmpty {
                            Text(PickerWords.text(.noActions))
                                .foregroundStyle(.secondary)
                                .frame(maxWidth: .infinity)
                                .padding(.vertical, 18 * grid.s)
                        }
                        ForEach(Array(actions.enumerated()), id: \.offset) { index, action in
                            if grouped, index == 0 || actions[index - 1].group != action.group {
                                Text(action.group)
                                    .font(grid.font(11, .semibold))
                                    .foregroundStyle(.secondary)
                                    .padding(.horizontal, 10 * grid.s)
                                    .padding(.top, (index == 0 ? 6 : 10) * grid.s)
                                    .padding(.bottom, 3 * grid.s)
                            }
                            ActionRow(action: action, highlighted: action == controller.highlighted,
                                      showsGroup: !grouped, grid: grid)
                                .id(index)
                                .onHover { if $0 { controller.highlight(action) } }
                                .onTapGesture { controller.run(action) }
                        }
                    }
                    .padding(5 * grid.s)
                }
                .frame(height: min(300, listHeight(actions, grouped: grouped)) * grid.s)
                .onChange(of: controller.highlighted) { action in
                    if let action, let index = actions.firstIndex(of: action) { scroll.scrollTo(index) }
                }
            }
            Divider()
            NativeTextField(
                text: controller.actionsQuery, placeholder: PickerWords.text(.searchActions),
                style: .plain, font: .systemFont(ofSize: 13 * grid.s),
                register: { controller.actionsField = $0 },
                began: { controller.fieldBegan(.actions) },
                change: { controller.actionsQuery = $0 }
            )
            .padding(.horizontal, 12 * grid.s)
            .frame(height: 36 * grid.s)
        }
        .font(grid.font(13))
        .frame(width: 320 * grid.s)
        .background(SystemMaterial(material: .menu))
        .clipShape(RoundedRectangle(cornerRadius: 10 * grid.s, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 10 * grid.s, style: .continuous)
            .strokeBorder(Color(nsColor: .separatorColor), lineWidth: 0.5))
        .shadow(color: .black.opacity(0.2), radius: 16, y: 6)
        .accessibilityElement(children: .contain)
        .accessibilityLabel(PickerWords.text(.actionsTitle))
    }
}

private func listHeight(_ actions: [PickerAction], grouped: Bool) -> CGFloat {
    guard !actions.isEmpty else { return 56 }
    let groups = grouped ? Set(actions.map(\.group)).count : 0
    return CGFloat(actions.count) * 26 + CGFloat(groups) * 24 + 10
}

private struct ActionRow: View {
    let action: PickerAction
    let highlighted: Bool
    let showsGroup: Bool
    let grid: PickerGrid

    var body: some View {
        let ink = highlighted ? Color(nsColor: .alternateSelectedControlTextColor) : Color(nsColor: .labelColor)
        HStack(spacing: 6 * grid.s) {
            if let checked = action.checked {
                Image(systemName: "checkmark").font(grid.font(11, .semibold)).opacity(checked ? 1 : 0)
                    .frame(width: 14 * grid.s)
            }
            Text(action.title).lineLimit(1)
            Spacer(minLength: 12 * grid.s)
            if showsGroup { Text(action.group).opacity(0.6).lineLimit(1) }
            if !action.shortcut.isEmpty { Text(action.shortcut).opacity(0.6).fixedSize() }
        }
        .foregroundStyle(ink)
        .padding(.horizontal, 10 * grid.s)
        .frame(height: 26 * grid.s)
        .background(RoundedRectangle(cornerRadius: 5 * grid.s, style: .continuous)
            .fill(highlighted ? Color(nsColor: .selectedContentBackgroundColor) : .clear))
        .contentShape(Rectangle())
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(action.checked.map {
            [action.title, PickerWords.text($0 ? .actionStateOn : .actionStateOff)].joined(separator: ". ")
        } ?? action.title)
        .accessibilityHint(action.shortcut)
        .accessibilityAddTraits(highlighted ? [.isSelected, .isButton] : .isButton)
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
    /// Records where a control sits, so its native menu opens beneath it.
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
