import CIbisNative
import Combine
import Foundation

struct CitationDetails: Equatable {
    var form: CitationForm?
    var term: LocatorTerm = .page
    var locator = ""
    var prefix = ""
    var suffix = ""

    init(segment: PickerSegment? = nil) {
        if let locator = segment?.locator {
            term = locator.label
            self.locator = locator.text
        }
        prefix = segment?.prefix ?? ""
    }
}

@MainActor
final class PickerModel: ObservableObject {
    struct Entry: Equatable {
        let row: PickerRow
        var details: CitationDetails
    }

    enum Drawer: Equatable {
        case citation(ItemRef)
        case peek(ItemRef)

        var item: ItemRef {
            switch self {
            case let .citation(item), let .peek(item): item
            }
        }
    }

    @Published var query = "" {
        didSet { if query != oldValue, !applyingReply { search(afterTyping: true) } }
    }
    @Published var format = "" {
        didSet {
            guard format != oldValue else { return }
            for id in detailsByItem.keys {
                if let form = detailsByItem[id]?.form, !forms.contains(where: { $0.form == form }) {
                    detailsByItem[id]?.form = nil
                }
            }
            prepare()
        }
    }
    @Published var appearance: Appearance = .system
    @Published var density: Density = .compact
    @Published var textPercent = 100
    @Published private(set) var selected: ItemRef?
    @Published private(set) var catalog: PickerOpenResult?
    @Published private(set) var sections: [PickerSection] = []
    @Published private(set) var searchReply: PickerSearchResult?
    private(set) var searchRevision = 0
    @Published private(set) var preview: PickerPreviewResult?
    @Published private(set) var previewEntries: [Entry] = []
    @Published private(set) var citations: [Entry] = []
    @Published private(set) var drawer: Drawer?
    @Published private(set) var formExamples: [FormOption] = []
    @Published private(set) var isOpening = false
    @Published private(set) var isFormatting = false
    @Published private(set) var isDelivering = false
    @Published private(set) var shorthand = false
    @Published private(set) var keepOrder = false
    @Published private(set) var pasteExpected = false
    @Published private(set) var targetName: String?
    @Published private(set) var notice: String?

    var file: URL?
    private let engine: EngineClient
    private var openingTask: Task<Void, Never>?
    private var searchTask: Task<Void, Never>?
    private var previewTask: Task<Void, Never>?
    private var formsTask: Task<Void, Never>?
    private var formatDelay: Task<Void, Never>?
    private var opening: PickerOpenRequest?
    private var detailsByItem: [ItemRef: CitationDetails] = [:]
    private var answeredQuery: String?
    private var applyingReply = false
    private var prepared = false
    private var delivery: UUID?
    private var queuedDelivery: (@MainActor () -> Void)?
    private var lastDelivery: String?
    private var lastShown: Date?
    private var undo: (entries: [Entry], keepOrder: Bool)?
    private var preferenceOverrides = Set<Preference>()

    enum Preference { case appearance, density, text }

    init(engine: EngineClient, file: URL? = nil) {
        self.engine = engine
        self.file = file
    }

    var scale: CGFloat { CGFloat(textPercent) / 100 }
    var rows: [PickerRow] { sections.flatMap(\.rows) }
    var selection: PickerRow? { rows.first { $0.id == selected } }
    var drawerRow: PickerRow? {
        guard let id = drawer?.item else { return nil }
        return citations.first(where: { $0.row.id == id })?.row ?? rows.first { $0.id == id }
    }
    var libraryReady: Bool { catalog?.library?.condition == .ready }
    var forms: [PickerForm] {
        guard let catalog, let choice = catalog.formats.first(where: { $0.id == format }) else { return [] }
        return catalog.forms.filter { choice.forms.contains($0.form) }
    }
    var details: CitationDetails {
        get {
            guard let item = drawer?.item ?? selected else { return CitationDetails() }
            return detailsByItem[item] ?? CitationDetails(segment: item == selected ? activeSegment : nil)
        }
        set {
            guard let item = drawer?.item ?? selected, newValue != details else { return }
            detailsByItem[item] = newValue
            prepare()
        }
    }

    private var activeSegment: PickerSegment? { searchReply?.segments.last.flatMap { $0.complete ? nil : $0 } }
    var canDeliver: Bool {
        prepared && !isDelivering && preview?.problem == nil && preview?.text.isEmpty == false
    }
    var canUndo: Bool { undo != nil }

    func changeSize(_ offset: Int) {
        preferenceOverrides.insert(.text)
        textPercent = min(200, max(100, textPercent + offset))
    }

    func chooseAppearance(_ appearance: Appearance) {
        preferenceOverrides.insert(.appearance)
        self.appearance = appearance
    }

    func chooseDensity(_ density: Density) {
        preferenceOverrides.insert(.density)
        self.density = density
    }

    func present(app: String?, name: String? = nil) {
        dismiss()
        if lastShown.map({ Date().timeIntervalSince($0) >= 60 }) ?? true { query = "" }
        lastShown = Date()
        selected = nil
        notice = lastDelivery
        lastDelivery = nil
        targetName = name
        opening = PickerOpenRequest(app: app, file: file?.path, language: PickerWords.language, reload: nil)
        loadLibrary()
    }

    func reload() {
        guard let opening else { return }
        self.opening = PickerOpenRequest(app: opening.app, file: file?.path, language: opening.language, reload: true)
        loadLibrary()
    }

    private func loadLibrary() {
        guard let request = opening else { return }
        openingTask?.cancel()
        searchTask?.cancel()
        retirePreview()
        answeredQuery = nil
        isOpening = true
        openingTask = Task {
            do {
                let opened = try await engine.open(request)
                try Task.checkCancellation()
                catalog = opened
                if !preferenceOverrides.contains(.appearance) { appearance = opened.appearance }
                if !preferenceOverrides.contains(.density) { density = opened.density }
                if !preferenceOverrides.contains(.text) { textPercent = opened.largerText ? 150 : 100 }
                format = opened.format
                pasteExpected = ibis_native_paste_expected(opened.pasteEnabled) && targetName != nil
                isOpening = false
                if libraryReady {
                    search()
                } else {
                    sections = []
                    searchReply = nil
                }
            } catch {
                guard !Task.isCancelled else { return }
                isOpening = false
                notice = pickerError(error)
            }
        }
    }

    func dismiss(forDelivery: Bool = false) {
        if opening != nil { lastShown = Date() }
        opening = nil
        openingTask?.cancel()
        searchTask?.cancel()
        retirePreview()
        answeredQuery = nil
        preview = nil
        previewEntries = []
        drawer = nil
        citations = []
        detailsByItem = [:]
        applyingReply = true
        if shorthand { query = "" }
        shorthand = false
        applyingReply = false
        keepOrder = false
        undo = nil
        if !forDelivery {
            ibis_native_cancel()
            delivery = nil
            isDelivering = false
        }
    }

    func escape() -> Bool {
        if drawer != nil {
            closeDrawer()
        } else if shorthand {
            toggleShorthand()
        } else if !query.isEmpty {
            query = ""
        } else {
            return false
        }
        return true
    }

    func toggleShorthand() {
        shorthand.toggle()
        search()
    }

    private func search(afterTyping: Bool = false) {
        guard opening != nil, !isOpening, libraryReady else { return }
        searchTask?.cancel()
        retirePreview()
        answeredQuery = nil
        if afterTyping { notice = nil }
        let request = PickerSearchRequest(query: query, shorthand: shorthand)
        searchTask = Task {
            do {
                if afterTyping { try await Task.sleep(for: .milliseconds(60)) }
                let result = try await engine.search(request)
                try Task.checkCancellation()
                accept(result)
            } catch {
                guard !Task.isCancelled else { return }
                notice = pickerError(error)
            }
        }
    }

    private func accept(_ reply: PickerSearchResult) {
        let finished = reply.segments.filter(\.complete)
        if shorthand, !finished.isEmpty, finished.allSatisfy({ $0.row != nil }) {
            saveUndo()
            for segment in finished {
                guard let row = segment.row else { continue }
                add(row, details: CitationDetails(segment: segment))
            }
            applyingReply = true
            query = reply.segments.last.flatMap { $0.complete ? nil : $0.typed } ?? ""
            applyingReply = false
        }
        searchRevision += 1
        searchReply = reply
        sections = reply.sections
        if let current = selected, !rows.contains(where: { $0.id == current }) {
            selected = nil
        }
        if selected == nil { selected = rows.first?.id }
        if case let .peek(id) = drawer, id != selected { closeDrawer() }
        if case .citation = drawer, drawerRow == nil { closeDrawer() }
        answeredQuery = query
        prepare()
    }

    private func retirePreview() {
        previewTask?.cancel()
        formsTask?.cancel()
        formatDelay?.cancel()
        queuedDelivery = nil
        prepared = false
        isFormatting = false
    }

    private func prepare() {
        retirePreview()
        guard opening != nil, answeredQuery == query, !isOpening, !format.isEmpty,
              searchReply?.status.contains(where: { $0.kind == .shorthand }) != true else { return }
        let entries = citations.isEmpty
            ? selection.map { [Entry(row: $0, details: detailsByItem[$0.id] ?? CitationDetails(segment: activeSegment))] } ?? []
            : citations.map { Entry(row: $0.row, details: detailsByItem[$0.row.id] ?? $0.details) }
        guard !entries.isEmpty else {
            preview = nil
            previewEntries = []
            return
        }
        let request = PickerPreviewRequest(entries: entries.map { entry in
            let details = entry.details
            return PickerPreviewEntry(
                cite: entry.row.cite, form: details.form, item: entry.row.id,
                locator: details.locator.isEmpty ? nil : LocatorInput(label: details.term, text: details.locator),
                prefix: details.prefix.isEmpty ? nil : details.prefix,
                suffix: details.suffix.isEmpty ? nil : details.suffix
            )
        }, format: format, keepOrder: keepOrder)
        formatDelay = Task {
            try? await Task.sleep(for: .milliseconds(150))
            if !Task.isCancelled { isFormatting = true }
        }
        previewTask = Task {
            do {
                let result = try await engine.preview(request)
                try Task.checkCancellation()
                formatDelay?.cancel()
                isFormatting = false
                previewEntries = entries
                prepared = result.problem == nil
                preview = result
                examples()
                let deliver = queuedDelivery
                queuedDelivery = nil
                if canDeliver { deliver?() }
            } catch {
                guard !Task.isCancelled else { return }
                formatDelay?.cancel()
                isFormatting = false
                queuedDelivery = nil
                notice = pickerError(error)
            }
        }
    }

    func select(_ item: ItemRef?) {
        guard answeredQuery == query, item != selected else { return }
        if case .peek = drawer { closeDrawer() }
        selected = item
        prepare()
    }

    func move(_ step: Int, within shown: [PickerRow]) {
        guard !shown.isEmpty, !isDelivering else { return }
        let current = shown.firstIndex { $0.id == selected } ?? 0
        select(shown[min(max(current + step, 0), shown.count - 1)].id)
    }

    func move(_ step: Int) { move(step, within: rows) }

    private func saveUndo() {
        let entries = citations.map { Entry(row: $0.row, details: detailsByItem[$0.row.id] ?? $0.details) }
        undo = (entries, keepOrder)
    }

    private func add(_ row: PickerRow, details: CitationDetails) {
        guard !citations.contains(where: { $0.row.id == row.id }) else { return }
        detailsByItem[row.id] = details
        citations.append(Entry(row: row, details: details))
    }

    func toggleCitation(_ row: PickerRow) {
        guard answeredQuery == query, rows.contains(where: { $0.id == row.id }) else { return }
        if citations.contains(where: { $0.row.id == row.id }) {
            remove(row.id)
        } else {
            saveUndo()
            add(row, details: detailsByItem[row.id] ?? CitationDetails(segment: row.id == selected ? activeSegment : nil))
            prepare()
        }
    }

    func remove(_ id: ItemRef) {
        if citations.isEmpty {
            // The suggested token can be dismissed until another row is chosen.
            select(nil)
            if drawer?.item == id { closeDrawer() }
            return
        }
        guard citations.contains(where: { $0.row.id == id }) else { return }
        saveUndo()
        citations.removeAll { $0.row.id == id }
        detailsByItem[id] = nil
        if drawer?.item == id { closeDrawer() }
        prepare()
    }

    func reorder(_ id: ItemRef, by step: Int, displayed: [ItemRef]) {
        guard let index = displayed.firstIndex(of: id), displayed.indices.contains(index + step),
              citations.count > 1 else { return }
        saveUndo()
        // Once moved, the citation keeps the order the person actually saw.
        var ordered = displayed.compactMap { id in citations.first { $0.row.id == id } }
        guard ordered.count == citations.count else { return }
        ordered.swapAt(index, index + step)
        citations = ordered
        keepOrder = true
        prepare()
    }

    func toggleKeepOrder() {
        saveUndo()
        keepOrder.toggle()
        prepare()
    }

    func undoChange() {
        guard let undo else { return }
        citations = undo.entries
        keepOrder = undo.keepOrder
        for entry in citations { detailsByItem[entry.row.id] = entry.details }
        self.undo = nil
        if drawerRow == nil { closeDrawer() }
        prepare()
    }

    func showDrawer(for item: ItemRef? = nil) {
        guard let id = item ?? citations.last?.row.id ?? selected else { return }
        drawer = .citation(id)
        formExamples = []
        examples()
    }

    func togglePeek() {
        guard let selected else { return }
        if drawer == .peek(selected) { closeDrawer() } else { drawer = .peek(selected) }
    }

    func closeDrawer() {
        drawer = nil
        formsTask?.cancel()
        formExamples = []
    }

    private func examples() {
        formsTask?.cancel()
        guard case let .citation(id) = drawer, let preview, !preview.ticket.isEmpty,
              let index = previewEntries.firstIndex(where: { $0.row.id == id }) else { return }
        let request = PickerFormsRequest(contentHash: preview.contentHash, item: UInt64(index), ticket: preview.ticket)
        formsTask = Task {
            guard let reply = try? await engine.forms(request), !Task.isCancelled else { return }
            formExamples = reply.forms
        }
    }

    func deliver(whenReady: Bool = false, hide: @escaping @MainActor () -> Void) {
        guard canDeliver, let preview else {
            if whenReady, !prepared, previewTask != nil {
                queuedDelivery = { [weak self] in self?.deliver(hide: hide) }
            }
            return
        }
        let token = ibis_native_begin()
        guard token != 0 else { return }
        let attempt = UUID()
        delivery = attempt
        isDelivering = true
        let request = PickerDeliverRequest(contentHash: preview.contentHash, paste: pasteExpected, ticket: preview.ticket)
        Task { [self] in
            do {
                let result = try await engine.deliver(request) { [weak self] grant in
                    guard let self, self.delivery == attempt, self.opening != nil,
                          self.canWrite(request) else { return 0 }
                    let receipt = withUtf8(grant.text) { text in
                        withUtf8(grant.html) { html in
                            withUtf8(grant.rtf) { rtf in
                                ibis_native_write(token, text, html, rtf, grant.paste)
                            }
                        }
                    }
                    if receipt != 0 { hide() }
                    return receipt
                }
                guard delivery == attempt else { return }
                switch result {
                case IbisPasted: break
                case IbisPermissionRequired: lastDelivery = PickerWords.text(.copiedPermission)
                case nil: notice = PickerWords.text(.copyFailed)
                default: lastDelivery = PickerWords.text(.copied)
                }
            } catch {
                guard delivery == attempt else { return }
                notice = pickerError(error)
                search()
            }
            if delivery == attempt {
                delivery = nil
                isDelivering = false
            }
        }
    }

    private func canWrite(_ request: PickerDeliverRequest) -> Bool {
        prepared && preview?.ticket == request.ticket
    }
}

func pickerError(_ error: Error) -> String {
    (error as? DaemonError)?.errorDescription ?? PickerWords.text(.requestFailed)
}

/// Rust borrows this buffer only while body runs.
private func withUtf8<T>(_ text: String?, _ body: (IbisUtf8) -> T) -> T {
    guard let text else { return body(IbisUtf8(data: nil, len: 0)) }
    return Array(text.utf8).withUnsafeBufferPointer { bytes in
        body(IbisUtf8(data: bytes.baseAddress, len: bytes.count))
    }
}
