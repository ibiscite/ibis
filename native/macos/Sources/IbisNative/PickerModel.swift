import CIbisNative
import Combine
import Foundation

struct CitationDetails: Equatable {
    var form: CitationForm?
    var term: LocatorTerm = .page
    var locator = ""
    var prefix = ""
    var suffix = ""
}

@MainActor
final class PickerModel: ObservableObject {
    @Published var query = "" {
        didSet { if query != oldValue { search(afterTyping: true) } }
    }
    @Published var selected: ItemRef? {
        didSet { if selected != oldValue { prepare() } }
    }
    @Published var format = "" {
        didSet {
            if format != oldValue {
                if let form = details.form, !forms.contains(where: { $0.form == form }) {
                    details.form = nil
                } else {
                    prepare()
                }
            }
        }
    }
    @Published var details = CitationDetails() {
        didSet { if details != oldValue { prepare() } }
    }
    @Published var drawerOpen = false
    @Published private(set) var catalog: PickerOpenResult?
    @Published private(set) var rows: [PickerRow] = []
    @Published private(set) var preview: PickerPreviewResult?
    @Published private(set) var heading: String?
    @Published private(set) var emptyMessage: String?
    @Published private(set) var isDelivering = false
    @Published private(set) var pasteExpected = false

    var file: URL?
    private let engine: EngineClient
    private var openingTask: Task<Void, Never>?
    private var searchTask: Task<Void, Never>?
    private var previewTask: Task<Void, Never>?
    private var answeredQuery: String?
    private var isPresented = false
    private var delivery: UUID?
    private var lastDelivery: String?
    private var lastShown: Date?

    init(engine: EngineClient, file: URL? = nil) {
        self.engine = engine
        self.file = file
    }

    var forms: [PickerForm] {
        guard let catalog, let choice = catalog.formats.first(where: { $0.id == format }) else { return [] }
        return catalog.forms.filter { choice.forms.contains($0.form) }
    }

    var canDeliver: Bool {
        guard let preview else { return false }
        return !isDelivering && preview.problem == nil && !preview.text.isEmpty
    }

    func present(app: String?) {
        dismiss()
        catalog = nil
        selected = nil
        rows = []
        details = CitationDetails()
        pasteExpected = false
        if lastShown.map({ Date().timeIntervalSince($0) >= 60 }) ?? true { query = "" }
        lastShown = Date()
        isPresented = true
        heading = lastDelivery
        lastDelivery = nil
        emptyMessage = "Opening library…"
        let request = PickerOpenRequest(app: app, file: file?.path)
        openingTask = Task {
            do {
                let opened = try await engine.open(request)
                try Task.checkCancellation()
                catalog = opened
                format = opened.format
                pasteExpected = ibis_native_paste_expected(opened.pasteEnabled)
                if opened.libraryOpen {
                    search()
                } else {
                    emptyMessage = "Choose a library to search."
                }
            } catch {
                guard !Task.isCancelled else { return }
                emptyMessage = error.localizedDescription
            }
        }
    }

    func dismiss(forDelivery: Bool = false) {
        if isPresented { lastShown = Date() }
        isPresented = false
        openingTask?.cancel()
        searchTask?.cancel()
        previewTask?.cancel()
        answeredQuery = nil
        preview = nil
        drawerOpen = false
        if !forDelivery {
            ibis_native_cancel()
            delivery = nil
            isDelivering = false
        }
    }

    func escape() -> Bool {
        if drawerOpen {
            drawerOpen = false
        } else if !query.isEmpty {
            query = ""
        } else {
            return false
        }
        return true
    }

    private func search(afterTyping: Bool = false) {
        guard isPresented, catalog?.libraryOpen == true else { return }
        searchTask?.cancel()
        previewTask?.cancel()
        answeredQuery = nil
        preview = nil
        if afterTyping { heading = nil }
        let request = PickerSearchRequest(query: query)
        searchTask = Task {
            do {
                if afterTyping { try await Task.sleep(nanoseconds: 60_000_000) }
                let result = try await engine.search(request)
                try Task.checkCancellation()
                rows = result.rows
                if let status = result.heading { heading = status }
                emptyMessage = request.query.isEmpty ? "Type to find a reference." : "No matching references."
                let next = rows.contains { $0.id == selected } ? selected : rows.first?.id
                changedSelection(to: next)
            } catch {
                guard !Task.isCancelled else { return }
                rows = []
                selected = nil
                emptyMessage = error.localizedDescription
            }
        }
    }

    private func changedSelection(to next: ItemRef?) {
        // Reset edits before admitting a preview for the newly answered query.
        answeredQuery = nil
        if next != selected {
            details = CitationDetails()
            selected = next
        }
        answeredQuery = query
        prepare()
    }

    private func prepare() {
        previewTask?.cancel()
        preview = nil
        guard isPresented, answeredQuery == query, let row = rows.first(where: { $0.id == selected }),
              !format.isEmpty else { return }
        let locator = details.locator.isEmpty ? nil : LocatorInput(label: details.term, text: details.locator)
        let request = PickerPreviewRequest(
            cite: row.cite, form: details.form, format: format, item: row.id, locator: locator,
            prefix: details.prefix.isEmpty ? nil : details.prefix, suffix: details.suffix.isEmpty ? nil : details.suffix
        )
        previewTask = Task {
            do {
                let result = try await engine.preview(request)
                try Task.checkCancellation()
                preview = result
            } catch {
                guard !Task.isCancelled else { return }
                heading = error.localizedDescription
            }
        }
    }

    func select(_ item: ItemRef?) {
        guard answeredQuery == query, item != selected else { return }
        changedSelection(to: item)
    }

    func move(_ step: Int) {
        guard answeredQuery == query, !rows.isEmpty, !isDelivering else { return }
        let current = rows.firstIndex(where: { $0.id == selected }) ?? 0
        select(rows[min(max(current + step, 0), rows.count - 1)].id)
    }

    func deliver(hide: @escaping @MainActor () -> Void) {
        guard canDeliver, let preview else { return }
        let token = ibis_native_begin()
        guard token != 0 else { return }
        let attempt = UUID()
        delivery = attempt
        isDelivering = true
        let request = PickerDeliverRequest(contentHash: preview.contentHash, paste: pasteExpected, ticket: preview.ticket)
        Task { [self] in
            do {
                let result = try await engine.deliver(request) { [weak self] grant in
                    guard let self, self.delivery == attempt, self.isPresented,
                          self.preview?.ticket == request.ticket else { return 0 }
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
                case IbisPermissionRequired: lastDelivery = "Copied. Allow Paste in the Ibis menu to paste automatically."
                case nil: heading = "The clipboard changed or could not be written."
                default: lastDelivery = "Copied. Press ⌘V to paste."
                }
            } catch {
                guard delivery == attempt else { return }
                heading = error.localizedDescription
                search()
            }
            if delivery == attempt {
                delivery = nil
                isDelivering = false
            }
        }
    }
}

/// Rust borrows this buffer only while body runs.
private func withUtf8<T>(_ text: String?, _ body: (IbisUtf8) -> T) -> T {
    guard let text else { return body(IbisUtf8(data: nil, len: 0)) }
    return Array(text.utf8).withUnsafeBufferPointer { bytes in
        body(IbisUtf8(data: bytes.baseAddress, len: bytes.count))
    }
}
