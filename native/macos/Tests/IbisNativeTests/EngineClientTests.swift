import Darwin
import Foundation
import XCTest
@testable import IbisNative

final class EngineClientTests: XCTestCase {
    @MainActor
    func testSavedLibrarySurvivesNewClientAndRejectedReplacement() async throws {
        let fixture = try Fixture()
        try fixture.identifyDaemon()
        let settings = fixture.client()
        let opening = PickerOpenRequest(app: nil, file: nil, keepQuery: nil, language: .enUS, reload: nil, site: nil)
        let empty = try await settings.open(opening)
        XCTAssertNil(empty.library)
        let saved = try await settings.chooseLibrary(fixture.library)
        XCTAssertEqual(saved.library?.name, fixture.library.lastPathComponent)
        XCTAssertEqual(saved.library?.missing, false)

        let picker = fixture.client()
        let opened = try await picker.open(opening)
        XCTAssertEqual(opened.library?.name, fixture.library.lastPathComponent)
        XCTAssertEqual(opened.library?.condition, .ready)
        XCTAssertEqual(opened.formats.first { $0.id == "apa" }?.canKeepOrder, true)
        XCTAssertEqual(opened.formats.first { $0.id == "pandoc" }?.canKeepOrder, false)
        _ = try await picker.act(.useFormat(id: "apa"))
        let reply = try await picker.typed("north")
        XCTAssertNotNil(reply.sections.first?.rows.first)
        let preview = try XCTUnwrap(reply.preview)
        let missing = fixture.root.appendingPathComponent("Missing.csljson")
        do {
            _ = try await picker.chooseLibrary(missing)
            XCTFail("A missing replacement must not become the saved library.")
        } catch {
            let after = try await picker.settings(.readOnly)
            XCTAssertEqual(after.library, saved.library)
        }
        let request = PickerDeliverRequest(contentHash: preview.contentHash, paste: false, ticket: preview.ticket)
        let delivered = try await picker.deliver(request) { grant in
            XCTAssertEqual(grant.text, preview.text)
            return 1
        }
        XCTAssertNotNil(delivered)

        try FileManager.default.moveItem(at: fixture.library, to: missing)
        let moved = try await settings.settings(.readOnly)
        XCTAssertEqual(moved.library?.missing, true)
        // The picker reads the moved file as missing from the open error, so it offers to find it.
        let lost = try await fixture.client().open(opening)
        XCTAssertEqual(lost.library?.condition, .missing)
        let recovered = try await settings.chooseLibrary(missing)
        XCTAssertEqual(recovered.library?.name, missing.lastPathComponent)
        let reopened = try await fixture.client().open(opening)
        XCTAssertEqual(reopened.library?.condition, .ready)
    }

    @MainActor
    func testFormatChoicesStayWithTheCapturedAppOrSite() async throws {
        let fixture = try Fixture()
        try fixture.identifyDaemon()
        let client = fixture.client()
        let text = fixture.opening
        let initial = try await client.open(text).formatScope
        let storedReply = try await client.changeFormat(
            PickerChangeFormatRequest(expected: initial, forget: nil, remember: "modern-language-association"), for: text
        )
        let stored = try XCTUnwrap(storedReply.formatScope)
        XCTAssertEqual(stored.format, "modern-language-association")
        XCTAssertEqual(stored.from, .app)
        XCTAssertEqual(stored.rememberedFor, text.app)

        func browser(_ site: String) -> PickerOpenRequest {
            PickerOpenRequest(app: "com.apple.Safari", file: fixture.library.path, keepQuery: nil, language: .enUS, reload: nil, site: site)
        }
        let overleaf = browser("overleaf.com")
        let googleDocs = browser("docs.google.com")
        let latex = try await client.open(overleaf).formatScope
        XCTAssertEqual(latex.remembersFor, "overleaf.com")
        XCTAssertNil(latex.rememberedFor)
        let savedSiteReply = try await client.changeFormat(
            PickerChangeFormatRequest(expected: latex, forget: nil, remember: "pandoc"), for: overleaf
        )
        let savedSite = try XCTUnwrap(savedSiteReply.formatScope)
        XCTAssertEqual(savedSite.rememberedFor, "overleaf.com")
        let docs = try await client.open(googleDocs).formatScope
        XCTAssertEqual(docs.format, "apa")
        XCTAssertNil(docs.rememberedFor)
        do {
            _ = try await client.changeFormat(
                PickerChangeFormatRequest(expected: savedSite, forget: true, remember: nil), for: overleaf
            )
            XCTFail("An earlier opening must never change the new destination.")
        } catch { }

        let unknown = try await client.open(browser("unlisted.invalid")).formatScope
        XCTAssertEqual(unknown.remembersFor, "com.apple.Safari")
        let otherClient = fixture.client()
        let reopened = try await otherClient.open(overleaf).formatScope
        XCTAssertEqual(reopened, savedSite)
        let resetReply = try await otherClient.changeFormat(
            PickerChangeFormatRequest(expected: reopened, forget: true, remember: nil), for: overleaf
        )
        let reset = try XCTUnwrap(resetReply.formatScope)
        XCTAssertEqual(reset, latex)
        let originalApp = try await otherClient.open(text).formatScope
        XCTAssertEqual(originalApp, stored)
        _ = try await client.open(text)
        let search = try await client.typed("north")
        XCTAssertEqual(search.format, originalApp.format)
        let preview = try XCTUnwrap(search.preview)
        XCTAssertNil(preview.problem)
        let changedReply = try await otherClient.changeFormat(
            PickerChangeFormatRequest(expected: originalApp, forget: nil, remember: "apa"), for: text
        )
        let changed = try XCTUnwrap(changedReply.formatScope)
        do {
            _ = try await client.deliver(
                PickerDeliverRequest(contentHash: preview.contentHash, paste: false, ticket: preview.ticket)
            ) { _ in
                XCTFail("An outdated saved format must never receive a delivery grant.")
                return 0
            }
            XCTFail("The daemon must refuse an outdated preview.")
        } catch let error as DaemonError {
            XCTAssertEqual(error, .changed)
        }
        let refreshed = try await client.changeFormat(.readOnly, for: text).formatScope
        XCTAssertEqual(refreshed, changed)
    }

    @MainActor
    func testSettingsPersistThroughTheOwnerAndReachThePicker() async throws {
        let fixture = try Fixture()
        try fixture.identifyDaemon()
        let client = fixture.client()
        let initial = try await client.settings(.readOnly)
        XCTAssertEqual(initial.density, .compact)
        XCTAssertTrue(initial.paste)
        XCTAssertNil(initial.shortcut)
        let shortcut = "Control+Alt+Shift+Command+F18"
        let saved = try await client.settings(
            .patch(appearance: .dark, density: .comfortable, largerText: true, paste: false,
                   shortcut: ShortcutSetting(binding: shortcut))
        )
        XCTAssertEqual(saved.density, .comfortable)
        XCTAssertFalse(saved.paste)
        XCTAssertFalse(saved.version.isEmpty)
        let reader = fixture.client()
        let reloaded = try await reader.settings(.readOnly)
        XCTAssertEqual(reloaded, saved)
        XCTAssertEqual(reloaded.shortcut, shortcut)
        let opened = try await reader.open(fixture.opening)
        XCTAssertEqual(opened.appearance, .dark)
        XCTAssertEqual(opened.density, .comfortable)
        XCTAssertTrue(opened.largerText)
        XCTAssertFalse(opened.pasteEnabled)
        _ = try await reader.settings(.patch(shortcut: ShortcutSetting(binding: nil)))
        let reset = try await client.settings(.readOnly)
        XCTAssertNil(reset.shortcut)
        XCTAssertEqual(reset.density, .comfortable)
        XCTAssertFalse(reset.paste)
    }

    @MainActor
    func testGuardedDeliveryConsumesPreviewAndObservesRevocation() async throws {
        let fixture = try Fixture()
        try fixture.identifyDaemon()
        let client = fixture.client()
        let opened = try await client.open(fixture.opening)
        XCTAssertTrue(opened.library?.condition == .ready)
        XCTAssertTrue(opened.pasteEnabled)
        let initial = opened.state
        XCTAssertEqual(initial.sections.first?.kind, .recentlyAdded)
        let row = try XCTUnwrap(initial.sections.first?.rows.first)
        XCTAssertEqual(initial.highlight, row.id)
        XCTAssertEqual(row.title, "Invented lanterns of North Quay")
        XCTAssertEqual(row.authors, ["Mira Vale"])
        XCTAssertEqual(row.year, "2024")
        XCTAssertEqual(row.journal, "Journal of Imaginary Cities")
        let details = PickerDetails(form: .narrative, locator: Locator(term: .page, value: "chapter 12"),
                                    prefix: nil, suffix: nil)
        _ = try await client.act(.useFormat(id: "apa"))
        let edited = try await client.act(.details(details: details, item: row.id))
        XCTAssertEqual(edited.citation.map(\.added), [true])
        let preview = try XCTUnwrap(edited.preview)
        XCTAssertNil(preview.problem)
        XCTAssertTrue(preview.text.contains("chapter 12"), preview.text)
        // Browsing beside an added reference keeps the citation; the newest
        // render carries the ticket that delivers it.
        let browse = try await client.typed("bridges")
        XCTAssertNotEqual(browse.sections.first?.rows.first?.id, row.id)
        let browsed = try await client.typed("")
        XCTAssertEqual(browsed.citation.map(\.row.id), [row.id])
        let current = try XCTUnwrap(browsed.preview)
        XCTAssertEqual(current.text, preview.text)

        let preferences = fixture.client()
        let disabled = try await preferences.settings(
            .patch(paste: false)
        )
        XCTAssertFalse(disabled.paste)

        let request = PickerDeliverRequest(contentHash: current.contentHash, paste: true, ticket: current.ticket)
        let settingsLock = fixture.settings.deletingPathExtension().appendingPathExtension("json.lock").path
        let copied = try await client.deliver(request) { grant in
            XCTAssertFalse(grant.paste)
            XCTAssertEqual(grant.text, current.text)
            XCTAssertNotNil(grant.html)
            XCTAssertNotNil(grant.rtf)
            let fd = open(settingsLock, O_RDONLY)
            XCTAssertGreaterThanOrEqual(fd, 0)
            if fd >= 0 {
                XCTAssertEqual(flock(fd, LOCK_EX | LOCK_NB), -1)
                XCTAssertEqual(errno, EWOULDBLOCK)
                close(fd)
            }
            return 1 // The test acknowledges a copy without changing the system clipboard.
        }
        XCTAssertNotNil(copied)
        let recents = try await client.open(fixture.opening).state
        XCTAssertEqual(recents.sections.map(\.kind), [.citedHere, .recentlyAdded])
        XCTAssertEqual(recents.sections.first?.rows.map(\.id), [row.id])

        do {
            _ = try await client.deliver(request) { _ in
                XCTFail("A spent preview must not publish clipboard content.")
                return 0
            }
            XCTFail("The engine accepted a spent preview.")
        } catch {
            XCTAssertEqual(error as? DaemonError, .changed)
        }

        let copyOnly = try await client.open(fixture.opening)
        XCTAssertFalse(copyOnly.pasteEnabled)
        _ = try await client.act(.useFormat(id: "apa"))
        let copyPreviewReply = try await client.act(.details(details: details, item: row.id))
        let copyPreview = try XCTUnwrap(copyPreviewReply.preview)
        let enabled = try await preferences.settings(
            .patch(paste: true)
        )
        XCTAssertTrue(enabled.paste)
        let copyRequest = PickerDeliverRequest(contentHash: copyPreview.contentHash, paste: true, ticket: copyPreview.ticket)
        let result = try await client.deliver(copyRequest) { grant in
            XCTAssertFalse(grant.paste, "A new preference must not enable paste for an old opening.")
            return 1
        }
        XCTAssertNotNil(result)
        let reopened = try await client.open(fixture.opening)
        XCTAssertTrue(reopened.pasteEnabled)
    }

    @MainActor
    func testChangedReferenceRefusesTheShownCitation() async throws {
        let fixture = try Fixture()
        try fixture.identifyDaemon()
        let client = fixture.client()
        _ = try await client.open(fixture.opening)
        _ = try await client.act(.useFormat(id: "apa"))
        let original = try await client.typed("north")
        let row = try XCTUnwrap(original.sections.first?.rows.first)
        let preview = try XCTUnwrap(original.preview)
        XCTAssertNil(preview.problem)
        let old = try String(contentsOf: fixture.library, encoding: .utf8)
        let changed = old.replacingOccurrences(of: "North Quay", with: "North Quay after rain")
        try changed.write(to: fixture.library, atomically: true, encoding: .utf8)
        let deadline = ContinuousClock.now + .seconds(10)
        while try await client.typed("north").sections.first?.rows.first?.cite == row.cite {
            guard ContinuousClock.now < deadline else {
                return XCTFail("The engine did not publish the changed library.")
            }
            try await Task.sleep(for: .milliseconds(30))
        }

        let request = PickerDeliverRequest(contentHash: preview.contentHash, paste: false, ticket: preview.ticket)
        do {
            _ = try await client.deliver(request) { _ in
                XCTFail("A stale reference must not reach the clipboard writer.")
                return 0
            }
            XCTFail("The engine accepted a stale reference.")
        } catch {
            XCTAssertEqual(error as? DaemonError, .changed)
        }
    }

    @MainActor
    func testReadReconnectsAfterCrashAndNewerBuildWins() async throws {
        let fixture = try Fixture()
        let original = try fixture.identifyDaemon()
        let client = fixture.client()
        _ = try await client.open(fixture.opening)
        let before = try await client.typed("north")

        _ = try await client.act(.add)
        fixture.stop(signal: SIGKILL)
        try await Task.sleep(for: .milliseconds(30))
        // The added reference went with the old session: an intent is refused
        // rather than applied to a new, empty one, until the shell opens again.
        do {
            _ = try await client.typed("north")
            XCTFail("An intent replayed into a new session.")
        } catch {
            XCTAssertEqual(error as? DaemonError, .changed)
        }
        let reopened = try await client.open(fixture.opening)
        XCTAssertTrue(reopened.state.citation.allSatisfy { !$0.added })
        let recovered = try await client.typed("north")
        XCTAssertEqual(recovered.sections, before.sections)
        let restarted = try fixture.identifyDaemon()
        XCTAssertNotEqual(original, restarted)

        let newerBinary = fixture.root.appendingPathComponent("ibis-newer")
        try FileManager.default.copyItem(at: fixture.binary, to: newerBinary)
        let modified = try XCTUnwrap(FileManager.default.attributesOfItem(atPath: fixture.binary.path)[.modificationDate] as? Date)
        try FileManager.default.setAttributes([.modificationDate: modified.addingTimeInterval(60)],
                                              ofItemAtPath: newerBinary.path)
        let upgraded = fixture.client(binary: newerBinary)
        _ = try await upgraded.open(fixture.opening)
        let upgradedRows = try await upgraded.typed("north")
        XCTAssertEqual(upgradedRows.sections, before.sections)
        let replacement = try fixture.identifyDaemon(binary: newerBinary)
        XCTAssertNotEqual(replacement, restarted)

        do {
            _ = try await client.typed("north")
            XCTFail("The older client displaced the newer engine.")
        } catch {
            XCTAssertEqual(error as? DaemonError, .newerEngine)
        }
    }
}
