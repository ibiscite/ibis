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
            term = locator.term
            self.locator = locator.value
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
    @Published private var activeToken: ItemRef?
    @Published private(set) var editingCitation: ItemRef?
    @Published private(set) var formExamples: [FormOption] = []
    @Published private(set) var isOpening = false
    @Published private(set) var isFormatting = false
    @Published private(set) var isDelivering = false
    @Published private(set) var shorthand = false
    @Published private(set) var keepOrder = false
    @Published private var pasteDestination: String?
    @Published private(set) var notice: String?

    var file: URL?
    private let engine: EngineClient
    private var openingTask: Task<Void, Never>?
    private var searchTask: Task<Void, Never>?
    private var previewTask: Task<Void, Never>?
    private var formsTask: Task<Void, Never>?
    private var formatDelay: Task<Void, Never>?
    private var opening: PickerOpenRequest?
    private var targetName: String?
    private var targetPID: Int32?
    private var detailsByItem: [ItemRef: CitationDetails] = [:]
    private var answeredQuery: String?
    private var applyingReply = false
    private var prepared = false
    private var previewSession: UInt64?
    private var delivery: UUID?
    private var queuedDelivery: (@MainActor () -> Void)?
    private var lastDelivery: String?
    private var lastShown: Date?
    private var undo: (entries: [Entry], keepOrder: Bool)?
    private var hasTemporaryTextSize = false

    init(engine: EngineClient, file: URL? = nil) {
        self.engine = engine
        self.file = file
    }

    var scale: CGFloat { CGFloat(textPercent) / 100 }
    var rows: [PickerRow] { sections.flatMap(\.rows) }
    var selection: PickerRow? { rows.first { $0.id == selected } }
    var hasDraft: Bool { !citations.isEmpty }
    var canChangeSelectionMembership: Bool { answeredQuery == query && !isOpening && !isDelivering && selection != nil }
    var isSelectionStaged: Bool { citations.contains { $0.row.id == selected } }
    var citationRows: [PickerRow] {
        hasDraft ? citations.map(\.row) : selection.map { [$0] } ?? []
    }
    var citationItem: ItemRef? {
        if let activeToken, containsCitation(activeToken) { return activeToken }
        return citationRows.last?.id
    }
    var libraryReady: Bool { catalog?.library?.condition == .ready }
    var forms: [PickerForm] {
        guard let catalog, let choice = catalog.formats.first(where: { $0.id == format }) else { return [] }
        return catalog.forms.filter { choice.forms.contains($0.form) }
    }
    var deliveryTitle: String {
        let count = citationRows.count
        if count < 2 {
            return pasteDestination.map { PickerWords.text(.paste, $0) } ?? PickerWords.text(.copyCitation)
        }
        let number = count.formatted(.number.locale(Locale(identifier: PickerWords.language.rawValue)))
        guard let pasteDestination else { return PickerWords.text(.copyCitations, number) }
        return PickerWords.text(.pasteCitations)
            .replacingOccurrences(of: "{IBIS_COUNT}", with: number)
            .replacingOccurrences(of: "{IBIS_VALUE}", with: pasteDestination)
    }

    func details(for item: ItemRef) -> CitationDetails {
        detailsByItem[item] ?? CitationDetails(segment: item == selected ? activeSegment : nil)
    }

    func changeDetails(for item: ItemRef, _ edit: (inout CitationDetails) -> Void) {
        guard containsCitation(item) else { return }
        let previous = details(for: item)
        var updated = previous
        edit(&updated)
        guard updated != previous else { return }
        detailsByItem[item] = updated
        prepare()
    }

    private func containsCitation(_ item: ItemRef) -> Bool {
        citations.isEmpty ? selected == item : citations.contains(where: { $0.row.id == item })
    }

    func selectCitation(_ item: ItemRef) {
        guard containsCitation(item), activeToken != item, !citations.isEmpty else { return }
        activeToken = item
    }

    private var activeSegment: PickerSegment? { searchReply?.segments.last.flatMap { $0.complete ? nil : $0 } }
    var canDeliver: Bool {
        prepared && !isDelivering && preview?.problem == nil && preview?.text.isEmpty == false
    }
    var canUndo: Bool { undo != nil }

    func changeSize(_ offset: Int) {
        hasTemporaryTextSize = true
        textPercent = min(200, max(100, textPercent + offset))
    }

    func applySettings(_ settings: SettingsResult, resetTextSize: Bool) {
        appearance = settings.appearance
        density = settings.density
        if resetTextSize { hasTemporaryTextSize = false }
        if !hasTemporaryTextSize { textPercent = settings.largerText ? 150 : 100 }
    }

    func present(app: String?, name: String? = nil, pid: Int32? = nil) {
        dismiss()
        if lastShown.map({ Date().timeIntervalSince($0) >= 60 }) ?? true { query = "" }
        lastShown = Date()
        selected = nil
        sections = []
        searchReply = nil
        notice = lastDelivery
        lastDelivery = nil
        targetName = name
        targetPID = pid
        pasteDestination = nil
        opening = PickerOpenRequest(app: app, file: file?.path, language: PickerWords.language, reload: nil)
        loadLibrary()
    }

    func reload() {
        guard let opening else { return }
        self.opening = PickerOpenRequest(app: opening.app, file: file?.path, language: opening.language, reload: true)
        loadLibrary()
    }

    func useSavedLibrary() {
        file = nil
        guard let opening else { return }
        clearDraft()
        catalog = nil
        sections = []
        searchReply = nil
        selected = nil
        notice = nil
        self.opening = PickerOpenRequest(app: opening.app, file: nil, language: opening.language, reload: nil)
        loadLibrary()
    }

    func reportLibraryChoice(_ error: Error) { notice = pickerError(error) }

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
                appearance = opened.appearance
                density = opened.density
                if !hasTemporaryTextSize { textPercent = opened.largerText ? 150 : 100 }
                format = opened.format
                let destination = ibis_native_paste_destination(opened.pasteEnabled)
                pasteDestination = destination > 0 && destination == targetPID ? targetName : nil
                isOpening = false
                if libraryReady {
                    search(refreshDraft: true)
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
        answeredQuery = nil
        clearDraft()
        applyingReply = true
        if shorthand { query = "" }
        shorthand = false
        applyingReply = false
        if !forDelivery {
            ibis_native_cancel()
            delivery = nil
            isDelivering = false
        }
    }

    private func clearDraft() {
        retirePreview()
        endCitationEdit()
        preview = nil
        previewSession = nil
        previewEntries = []
        citations = []
        activeToken = nil
        detailsByItem = [:]
        undo = nil
        keepOrder = false
    }

    func escape() -> Bool {
        if shorthand {
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
        search(refreshDraft: true)
    }

    private func search(afterTyping: Bool = false, refreshDraft: Bool = false) {
        guard opening != nil, !isOpening, libraryReady else { return }
        searchTask?.cancel()
        if refreshDraft || shorthand || !hasDraft { retirePreview() }
        answeredQuery = nil
        if afterTyping { notice = nil }
        let request = PickerSearchRequest(query: query, shorthand: shorthand)
        searchTask = Task {
            do {
                if afterTyping { try await Task.sleep(for: .milliseconds(60)) }
                let answer = try await engine.search(request)
                try Task.checkCancellation()
                let sessionChanged = previewSession.map { $0 != answer.session } ?? false
                accept(answer.result, refreshDraft: refreshDraft || sessionChanged)
            } catch {
                guard !Task.isCancelled else { return }
                retirePreview()
                notice = pickerError(error)
            }
        }
    }

    private func accept(_ reply: PickerSearchResult, refreshDraft: Bool) {
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
        answeredQuery = query
        if refreshDraft || shorthand || !hasDraft || (!prepared && previewTask == nil) { prepare() }
    }

    private func retirePreview() {
        previewTask?.cancel()
        previewTask = nil
        formsTask?.cancel()
        formatDelay?.cancel()
        queuedDelivery = nil
        prepared = false
        isFormatting = false
    }

    private func prepare() {
        retirePreview()
        if let editingCitation, !containsCitation(editingCitation) { endCitationEdit() }
        guard opening != nil, libraryReady, !isOpening, !format.isEmpty else { return }
        // In normal search the draft has its own inputs. Shorthand still waits
        // for the query, which can itself add references and locator details.
        if !hasDraft || shorthand {
            guard answeredQuery == query,
                  searchReply?.status.contains(where: { $0.kind == .shorthand }) != true else { return }
        }
        let entries = citationRows.map { Entry(row: $0, details: details(for: $0.id)) }
        guard !entries.isEmpty else {
            preview = nil
            previewEntries = []
            return
        }
        let request = PickerPreviewRequest(entries: entries.map { entry in
            let details = entry.details
            let hasLocator = !details.locator.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            return PickerPreviewEntry(
                cite: entry.row.cite, form: details.form, item: entry.row.id,
                locator: hasLocator ? Locator(term: details.term, value: details.locator) : nil,
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
                let answer = try await engine.preview(request)
                try Task.checkCancellation()
                previewTask = nil
                formatDelay?.cancel()
                isFormatting = false
                previewEntries = entries
                prepared = answer.result.problem == nil
                preview = answer.result
                previewSession = answer.session
                examples()
                let deliver = queuedDelivery
                queuedDelivery = nil
                if canDeliver { deliver?() }
            } catch {
                guard !Task.isCancelled else { return }
                previewTask = nil
                formatDelay?.cancel()
                isFormatting = false
                queuedDelivery = nil
                notice = pickerError(error)
            }
        }
    }

    func select(_ item: ItemRef?) {
        guard answeredQuery == query, item != selected else { return }
        selected = item
        if !hasDraft || shorthand { prepare() }
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
        activeToken = row.id
    }

    func toggleCitation(_ row: PickerRow) {
        guard answeredQuery == query, !isOpening, !isDelivering,
              rows.contains(where: { $0.id == row.id }) else { return }
        if citations.contains(where: { $0.row.id == row.id }) {
            remove(row.id)
        } else {
            stage(row)
        }
    }

    private func stage(_ row: PickerRow) {
        saveUndo()
        add(row, details: details(for: row.id))
        prepare()
    }

    func remove(_ id: ItemRef) {
        if citations.isEmpty {
            // The suggested token can be dismissed until another row is chosen.
            select(nil)
            return
        }
        guard citations.contains(where: { $0.row.id == id }) else { return }
        saveUndo()
        citations.removeAll { $0.row.id == id }
        detailsByItem[id] = nil
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
        prepare()
    }

    func beginCitationEdit(_ item: ItemRef) {
        guard containsCitation(item) else { return }
        selectCitation(item)
        editingCitation = item
        formExamples = []
        examples()
    }

    func endCitationEdit() {
        editingCitation = nil
        formsTask?.cancel()
        formExamples = []
    }

    private func examples() {
        formsTask?.cancel()
        guard let id = editingCitation, let preview, prepared, !preview.ticket.isEmpty,
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
        let request = PickerDeliverRequest(contentHash: preview.contentHash, paste: pasteDestination != nil, ticket: preview.ticket)
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
                search(refreshDraft: true)
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
