import AppKit
import CIbisNative
import Combine
import SwiftUI

/// The picker's one store. The engine's session owns the citation: the
/// results, the highlighted row, the references added and what Return
/// delivers arrive in `state` (decision 0066). The store keeps only what is
/// the shell's own: the field's text, the window, Peek, the details popover,
/// focus and the keys.
@MainActor
final class PickerStore: ObservableObject {
    enum EmptyState: Equatable {
        case newLibrary, noMatches, choose
        case missing(title: String, name: String)
        case unreadable(String)
    }

    enum Anchor: Hashable { case format, citation, actions }

    // From the engine.
    @Published private(set) var catalog: PickerOpenResult?
    @Published private(set) var state: PickerState?
    @Published private(set) var formExamples: [FormOption] = []

    // The shell's own.
    @Published private(set) var query = ""
    @Published private(set) var isOpening = false
    @Published private(set) var isDelivering = false
    @Published private(set) var isChangingFormat = false
    @Published private(set) var notice: String?
    /// The reference whose page and details are open in the popover.
    @Published private(set) var editing: ItemRef?
    @Published private(set) var peek: ItemRef?
    @Published private(set) var numbers = false
    @Published private(set) var isKey = false
    /// Changes when the keyboard moves the highlight, so only then the list scrolls.
    @Published private(set) var revealed = 0
    @Published var appearance: Appearance = .system
    @Published var density: Density = .compact
    @Published var textPercent = 100

    var anchors: [Anchor: CGRect] = [:]
    var visibleRows: [ItemRef] = []
    var choosingLibrary = false
    var file: URL?
    weak var panel: PickerPanel?
    weak var searchField: NSTextField?
    weak var locatorField: NSTextField?
    /// A paste was copied instead for want of access; the app asks once.
    var needsPasteAccess: (() -> Void)?
    /// A citation reached the app: the menu bar item nods.
    var didDeliver: (() -> Void)?
    /// The citation was copied rather than pasted; the text says what to do next.
    var didCopy: ((String) -> Void)?

    private let engine: EngineClient
    private var opening: PickerOpenRequest?
    private var openingTask: Task<Void, Never>?
    private var outbox: [PickerIntent] = []
    private var pump: Task<Void, Never>?
    private var deliverWhenSettled: Bool?
    private var delivery: UUID?
    private var targetName: String?
    private var targetPID: Int32?
    private var pasteDestination: String?
    private var lastShown: Date?
    private var selectionSearch: String?
    private var hasTemporaryTextSize = false
    private var announcement: Task<Void, Never>?
    private var formsTask: Task<Void, Never>?

    init(engine: EngineClient, file: URL? = nil) {
        self.engine = engine
        self.file = file
    }

    // MARK: What the views read

    var scale: CGFloat { CGFloat(textPercent) / 100 }
    var sections: [PickerSection] { state?.sections ?? [] }
    var rows: [PickerRow] { sections.flatMap(\.rows) }
    var highlight: ItemRef? { state?.highlight }
    var citation: [PickerEntry] { state?.citation ?? [] }
    var added: [PickerEntry] { citation.filter(\.added) }
    var libraryReady: Bool { catalog?.library?.condition == .ready }
    var settled: Bool { pump == nil && outbox.isEmpty && state?.query == query && !isOpening }

    func entry(_ item: ItemRef) -> PickerEntry? { citation.first { $0.row.id == item } }
    func isAdded(_ item: ItemRef) -> Bool { added.contains { $0.row.id == item } }

    var canDeliver: Bool {
        guard !isDelivering, !isOpening, let preview = state?.preview else { return false }
        return preview.problem == nil && !preview.ticket.isEmpty && !preview.text.isEmpty
    }

    /// One citation, however many references it holds; the footer counts those.
    var deliveryTitle: String {
        pasteDestination.map { PickerWords.text(.paste, $0) } ?? PickerWords.text(.copyCitation)
    }

    /// The app the citation goes to, for its icon beside the delivery button.
    var destinationIcon: NSImage? {
        guard pasteDestination != nil, let targetPID,
              let app = NSRunningApplication(processIdentifier: targetPID) else { return nil }
        return app.icon
    }

    var chosenFormat: PickerFormat? { catalog?.formats.first { $0.id == state?.format } }
    var formatName: String { chosenFormat?.name ?? PickerWords.text(.formatCustom) }
    var formatScope: PickerFormatScope? { state?.formatScope }
    var formatOwner: String? { name(for: formatScope?.remembersFor) }
    var rememberedFormatOwner: String? { name(for: formatScope?.rememberedFor) }
    var canChooseFormat: Bool {
        libraryReady && formatScope?.requiresBibliography == false && !isOpening && !isChangingFormat && !isDelivering
    }

    var formatContext: String {
        guard let scope = formatScope else { return "" }
        if state?.formatHeld == true { return PickerWords.text(.formatHeld, formatName) }
        if let project = scope.project {
            return PickerWords.text(scope.requiresBibliography ? .formatProjectBlocked : .formatProject, project)
        }
        switch scope.from {
        case .app: return rememberedFormatOwner.map { PickerWords.text(.formatRemembered, $0) } ?? ""
        case .suggestion:
            return formatOwner.map { PickerWords.text(.formatSuggestedFor, $0) } ?? PickerWords.text(.formatSuggestion)
        case .yours: return PickerWords.text(.formatDefault)
        case .writing: return ""
        }
    }

    private func name(for owner: String?) -> String? {
        guard let owner else { return nil }
        if owner.caseInsensitiveCompare(opening?.app ?? "") == .orderedSame { return targetName ?? owner }
        return owner
    }

    var showsList: Bool { !rows.isEmpty }
    var showsFooter: Bool { libraryReady && !isOpening && (showsList || !citation.isEmpty) }

    var emptyState: EmptyState? {
        guard rows.isEmpty, citation.isEmpty, !isOpening else { return nil }
        guard let catalog else { return notice == nil ? nil : .choose }
        guard let library = catalog.library else { return .choose }
        switch library.condition {
        case .missing: return .missing(title: catalog.status.first?.text ?? library.name, name: library.name)
        case .unreadable: return .unreadable(catalog.status.first?.text ?? library.name)
        case .ready: return query.trimmingCharacters(in: .whitespaces).isEmpty ? .newLibrary : .noMatches
        }
    }

    /// One line above the footer, only when there is something to say.
    var problem: (text: String, status: PickerStatus?)? {
        if formatScope?.requiresBibliography == true { return (formatContext, nil) }
        if let notice { return (notice, nil) }
        guard libraryReady, emptyState == nil else { return nil }
        let status = state?.preview?.problem ?? state?.status.first ?? catalog?.status.first
        return status.map { ($0.text, $0) }
    }

    /// The panel's height follows its content, as Spotlight's does.
    func height(_ grid: PickerGrid) -> CGFloat {
        var height = grid.header
        if state?.readsAs != nil { height += grid.hint }
        if emptyState != nil { height += 1 + grid.empty }
        else if showsList { height += 1 + listHeight(grid) }
        if problem != nil { height += grid.status }
        if showsFooter { height += 1 + grid.footer }
        return ceil(height)
    }

    func listHeight(_ grid: PickerGrid) -> CGFloat {
        let content = CGFloat(rows.count) * grid.row(density) + CGFloat(sections.count) * grid.heading + 12 * grid.s
        return min(content, grid.row(density) * 8 + grid.heading * 2 + 12 * grid.s)
    }

    func number(for item: ItemRef) -> Int? {
        guard numbers, let index = visibleRows.prefix(9).firstIndex(of: item) else { return nil }
        return index + 1
    }

    // MARK: Opening and closing

    /// Opens for the app in front. `capture` reads its site and, once, the
    /// short text selected there, which becomes the search (K5).
    func present(app: String?, name: String? = nil, pid: Int32? = nil,
                 capture: () -> (site: String?, selection: String?) = { (nil, nil) }) {
        dismiss()
        let (site, selection) = capture()
        selectionSearch = selection
        let keepQuery = lastShown.map { Date().timeIntervalSince($0) < 60 } ?? false
        lastShown = Date()
        notice = nil
        targetName = name
        targetPID = pid
        pasteDestination = nil
        open(PickerOpenRequest(app: app, file: file?.path, keepQuery: keepQuery, language: PickerWords.language,
                               reload: nil, site: site))
    }

    func reload() {
        guard let opening else { return }
        open(PickerOpenRequest(app: opening.app, file: file?.path, keepQuery: true, language: opening.language,
                               reload: true, site: opening.site))
    }

    func useSavedLibrary() {
        file = nil
        guard let opening else { return }
        catalog = nil
        state = nil
        notice = nil
        open(PickerOpenRequest(app: opening.app, file: nil, keepQuery: true, language: opening.language,
                               reload: nil, site: opening.site))
    }

    func reportLibraryChoice(_ error: Error) { notice = pickerError(error) }

    private func open(_ request: PickerOpenRequest) {
        opening = request
        openingTask?.cancel()
        pump?.cancel()
        pump = nil
        outbox.removeAll()
        isOpening = true
        // Text typed while the library opens is the person's and must survive the reply.
        let queryAtOpen = query
        openingTask = Task {
            do {
                let opened = try await engine.open(request)
                try Task.checkCancellation()
                catalog = opened
                appearance = opened.appearance
                density = opened.density
                if !hasTemporaryTextSize { textPercent = opened.largerText ? 150 : 100 }
                let destination = ibis_native_paste_destination(opened.pasteEnabled)
                pasteDestination = destination > 0 && destination == targetPID ? targetName : nil
                isOpening = false
                let typed = query != queryAtOpen ? query : nil
                query = opened.state.query
                apply(opened.state)
                // What the person typed wins over the selection read from the app in front.
                let selection = selectionSearch.take()
                if let text = typed ?? selection { type(text) }
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
        pump?.cancel()
        pump = nil
        outbox.removeAll()
        deliverWhenSettled = nil
        isChangingFormat = false
        closeEditing(refocus: false)
        peek = nil
        showNumbers(false)
        if !forDelivery {
            ibis_native_cancel()
            delivery = nil
            isDelivering = false
        }
    }

    func applySettings(_ settings: SettingsResult, resetTextSize: Bool) {
        appearance = settings.appearance
        density = settings.density
        if resetTextSize { hasTemporaryTextSize = false }
        if !hasTemporaryTextSize { textPercent = settings.largerText ? 150 : 100 }
    }

    func changeSize(_ offset: Int) {
        hasTemporaryTextSize = true
        textPercent = min(200, max(100, textPercent + offset))
    }

    // MARK: Intents

    func type(_ text: String) {
        guard text != query else { return }
        query = text
        notice = nil
        send(.type(query: text))
    }

    /// Intents go to the engine one at a time, in order. Typing coalesces:
    /// only the newest text waits behind the request in flight.
    func send(_ intent: PickerIntent) {
        guard opening != nil, catalog != nil, libraryReady, !isDelivering else { return }
        if let last = outbox.last, intent.replaces(last) {
            outbox[outbox.count - 1] = intent
        } else {
            outbox.append(intent)
        }
        guard pump == nil else { return }
        pump = Task { [weak self] in await self?.drain() }
    }

    private func drain() async {
        while !outbox.isEmpty, !Task.isCancelled {
            let intent = outbox.removeFirst()
            do {
                let next = try await engine.act(intent)
                guard !Task.isCancelled else { return }
                apply(next)
            } catch {
                guard !Task.isCancelled else { return }
                notice = pickerError(error)
            }
        }
        pump = nil
        if let copyOnly = deliverWhenSettled {
            deliverWhenSettled = nil
            deliver(copyOnly: copyOnly)
        }
    }

    private func apply(_ next: PickerState) {
        let countChanged = next.countLabel != state?.countLabel || next.query != state?.query
        state = next
        if let editing, entry(editing) == nil { closeEditing(refocus: false) }
        if let peek, !rows.contains(where: { $0.id == peek }) { self.peek = nil }
        if let editing { examples(for: editing) }
        if countChanged { announceCount() }
        panel?.fitHeight()
    }

    func move(_ step: Int) {
        send(.move(step: Int64(step)))
        revealed += 1
    }

    func highlight(_ item: ItemRef) {
        guard item != highlight else { return }
        send(.highlight(item: item))
    }

    /// Tab: what is typed and highlighted joins the citation; the field clears.
    func add() {
        guard !rows.isEmpty, editing == nil else { return }
        query = ""
        send(.add)
        NSHapticFeedbackManager.defaultPerformer.perform(.alignment, performanceTime: .now)
        focusSearch()
    }

    func toggle(_ item: ItemRef) {
        let adding = !isAdded(item)
        send(.toggle(item: item))
        if adding { NSHapticFeedbackManager.defaultPerformer.perform(.alignment, performanceTime: .now) }
    }

    func remove(_ item: ItemRef) {
        send(.remove(item: item))
        focusSearch()
    }

    func reorder(_ item: ItemRef, by step: Int) { send(.reorder(item: item, step: Int64(step))) }
    func undo() { send(.undo) }
    func keepOrder(_ on: Bool) { send(.keepOrder(on: on)) }

    func changeDetails(_ item: ItemRef, to details: PickerDetails) {
        guard let entry = entry(item), details != entry.details else { return }
        // Changing a typed or highlighted reference adds it, and the engine clears the field.
        if !entry.added { query = "" }
        send(.details(details: details, item: item))
    }

    /// Delete in an empty field takes back the last added reference; ⌘Z returns it.
    func deleteInEmptyField() -> Bool {
        guard query.isEmpty, let last = added.last else { return false }
        remove(last.row.id)
        return true
    }

    /// Escape: the popover, then Peek, then the field's text; else the picker closes.
    func escape() -> Bool {
        if editing != nil { closeEditing(); return true }
        if peek != nil { peek = nil; return true }
        guard !query.isEmpty else { return false }
        query = ""
        send(.clearSearch)
        focusSearch()
        return true
    }

    // MARK: Delivery

    /// Return delivers what the footer shows, once the engine has caught up.
    /// ⌥Return copies it instead of pasting.
    func deliver(copyOnly: Bool = false) {
        guard opening != nil, !isDelivering else { return }
        if !settled {
            deliverWhenSettled = copyOnly
            if pump == nil, !isOpening { send(.type(query: query)) }
            return
        }
        guard canDeliver, let preview = state?.preview else { return }
        let token = ibis_native_begin()
        guard token != 0 else { return }
        let attempt = UUID()
        delivery = attempt
        isDelivering = true
        let request = PickerDeliverRequest(contentHash: preview.contentHash, paste: !copyOnly && pasteDestination != nil,
                                           ticket: preview.ticket)
        Task { [self] in
            do {
                let result = try await engine.deliver(request) { [weak self] grant in
                    guard let self, self.delivery == attempt, self.opening != nil,
                          self.state?.preview?.ticket == request.ticket else { return 0 }
                    let receipt = withUtf8(grant.text) { text in
                        withUtf8(grant.html) { html in
                            withUtf8(grant.rtf) { rtf in ibis_native_write(token, text, html, rtf, grant.paste) }
                        }
                    }
                    if receipt != 0 { self.panel?.dismiss(forDelivery: true) }
                    return receipt
                }
                guard delivery == attempt else { return }
                switch result {
                case IbisPasted: didDeliver?()
                case IbisPermissionRequired:
                    didCopy?(PickerWords.text(.copied))
                    needsPasteAccess?()
                case nil: notice = PickerWords.text(.copyFailed)
                default:
                    // 8B: say so only when the citation was copied, not pasted.
                    didCopy?(PickerWords.text(.copied))
                    didDeliver?()
                }
                if result != nil { query = "" }
            } catch {
                guard delivery == attempt else { return }
                notice = pickerError(error)
                if let opening, let saved = try? await engine.changeFormat(.readOnly, for: opening),
                   delivery == attempt {
                    apply(saved)
                }
            }
            if delivery == attempt {
                delivery = nil
                isDelivering = false
            }
        }
    }

    // MARK: Formats

    func useFormatOnce(_ id: String?) {
        guard canChooseFormat else { return }
        send(.useFormat(id: id))
    }

    func rememberFormat(_ id: String) { changeFormat(remember: id, forget: nil) }
    func forgetFormat() { changeFormat(remember: nil, forget: true) }

    private func changeFormat(remember: String?, forget: Bool?) {
        guard canChooseFormat, let opening, let scope = formatScope, scope.project == nil,
              scope.remembersFor != nil else { return }
        isChangingFormat = true
        let request = PickerChangeFormatRequest(expected: scope, forget: forget, remember: remember)
        Task {
            do {
                let next = try await engine.changeFormat(request, for: opening)
                isChangingFormat = false
                apply(next)
            } catch {
                isChangingFormat = false
                notice = pickerError(error)
                if let saved = try? await engine.changeFormat(.readOnly, for: opening) { apply(saved) }
            }
        }
    }

    // MARK: Popover, Peek and focus

    func panelKeyChanged(_ key: Bool) { isKey = key }

    func focusSearch(caretAtEnd: Bool = true) { searchField?.focus(caretAtEnd: caretAtEnd) }

    func showNumbers(_ show: Bool) { if numbers != show { numbers = show } }

    func selectNumber(_ number: Int) -> Bool {
        guard visibleRows.indices.contains(number - 1) else { return false }
        highlight(visibleRows[number - 1])
        return true
    }

    /// ⌘L: the page and details of the reference Return would cite last.
    func edit(_ item: ItemRef? = nil) {
        guard let item = item ?? citation.last?.row.id else { return }
        if editing == item { closeEditing(); return }
        peek = nil
        editing = item
        formExamples = []
        examples(for: item)
        DispatchQueue.main.async { [weak self] in self?.locatorField?.focus(caretAtEnd: true) }
    }

    func closeEditing(refocus: Bool = true) {
        guard editing != nil else { return }
        editing = nil
        formsTask?.cancel()
        formExamples = []
        if refocus { focusSearch() }
    }

    func togglePeek(_ item: ItemRef? = nil) {
        guard let item = item ?? highlight else { return }
        if item != highlight { highlight(item) }
        peek = peek == item ? nil : item
    }

    private func examples(for item: ItemRef) {
        formsTask?.cancel()
        guard let preview = state?.preview, !preview.ticket.isEmpty,
              let index = citation.firstIndex(where: { $0.row.id == item }) else { return }
        let request = PickerFormsRequest(contentHash: preview.contentHash, item: UInt64(index), ticket: preview.ticket)
        formsTask = Task {
            guard let reply = try? await engine.forms(request), !Task.isCancelled else { return }
            formExamples = reply.forms
        }
    }

    /// VoiceOver hears the result count once typing settles.
    private func announceCount() {
        announcement?.cancel()
        guard let label = state?.countLabel else { return }
        announcement = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(500))
            guard !Task.isCancelled, let self, isKey, let field = searchField else { return }
            NSAccessibility.post(element: field, notification: .announcementRequested, userInfo: [
                .announcement: label, .priority: NSAccessibilityPriorityLevel.medium.rawValue,
            ])
        }
    }
}

private extension PickerIntent {
    /// Typing, and typing in one reference's details, keep only the newest text.
    func replaces(_ earlier: PickerIntent) -> Bool {
        switch (self, earlier) {
        case (.type, .type): true
        case let (.details(_, item), .details(_, previous)): item == previous
        default: false
        }
    }
}

extension PickerDetails {
    static let empty = PickerDetails(form: nil, locator: nil, prefix: nil, suffix: nil)
    func with(form: CitationForm?) -> Self { Self(form: form, locator: locator, prefix: prefix, suffix: suffix) }
    func with(locator: Locator?) -> Self { Self(form: form, locator: locator, prefix: prefix, suffix: suffix) }
    func with(prefix: String) -> Self { Self(form: form, locator: locator, prefix: prefix, suffix: suffix) }
    func with(suffix: String) -> Self { Self(form: form, locator: locator, prefix: prefix, suffix: suffix) }
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

private extension Optional {
    /// The value, leaving nil behind.
    mutating func take() -> Wrapped? {
        defer { self = nil }
        return self
    }
}
