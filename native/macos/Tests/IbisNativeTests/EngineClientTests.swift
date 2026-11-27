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
        let opening = PickerOpenRequest(app: nil, file: nil, language: .enUS, reload: nil)
        let empty = try await settings.open(opening)
        XCTAssertNil(empty.library)
        let saved = try await settings.chooseLibrary(fixture.library)
        XCTAssertEqual(saved.library?.name, fixture.library.lastPathComponent)
        XCTAssertEqual(saved.library?.missing, false)

        let picker = fixture.client()
        let opened = try await picker.open(opening)
        XCTAssertEqual(opened.library?.name, fixture.library.lastPathComponent)
        XCTAssertEqual(opened.library?.condition, .ready)
        let reply = try await picker.search(PickerSearchRequest(query: "north", shorthand: false)).result
        let row = try XCTUnwrap(reply.sections.first?.rows.first)
        let preview = try await picker.preview(PickerPreviewRequest(
            entries: [PickerPreviewEntry(cite: row.cite, form: nil, item: row.id, locator: nil, prefix: nil, suffix: nil)],
            format: "apa", keepOrder: false
        )).result
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
        let recovered = try await settings.chooseLibrary(missing)
        XCTAssertEqual(recovered.library?.name, missing.lastPathComponent)
        let reopened = try await fixture.client().open(opening)
        XCTAssertEqual(reopened.library?.condition, .ready)
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
        let initial = try await client.search(PickerSearchRequest(query: "", shorthand: false)).result
        XCTAssertEqual(initial.sections.first?.kind, .recentlyAdded)
        let row = try XCTUnwrap(initial.sections.first?.rows.first)
        XCTAssertEqual(row.title, "Invented lanterns of North Quay")
        XCTAssertEqual(row.authors, ["Mira Vale"])
        XCTAssertEqual(row.year, "2024")
        XCTAssertEqual(row.journal, "Journal of Imaginary Cities")
        let draft = PickerPreviewRequest(
            entries: [PickerPreviewEntry(cite: row.cite, form: .narrative, item: row.id,
                                         locator: Locator(term: .page, value: "chapter 12"), prefix: nil, suffix: nil)],
            format: "apa", keepOrder: false
        )
        let (preview, session) = try await client.preview(draft)
        XCTAssertNil(preview.problem)
        XCTAssertTrue(preview.text.contains("chapter 12"), preview.text)
        let browse = try await client.search(PickerSearchRequest(query: "bridges", shorthand: false))
        XCTAssertEqual(browse.session, session)
        XCTAssertNotEqual(browse.result.sections.first?.rows.first?.id, row.id)

        let preferences = fixture.client()
        let disabled = try await preferences.settings(
            .patch(paste: false)
        )
        XCTAssertFalse(disabled.paste)

        let request = PickerDeliverRequest(contentHash: preview.contentHash, paste: true, ticket: preview.ticket)
        let settingsLock = fixture.settings.deletingPathExtension().appendingPathExtension("json.lock").path
        let copied = try await client.deliver(request) { grant in
            XCTAssertFalse(grant.paste)
            XCTAssertEqual(grant.text, preview.text)
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
        let recents = try await client.search(PickerSearchRequest(query: "", shorthand: false)).result
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
        let copyPreview = try await client.preview(draft).result
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
        let query = PickerSearchRequest(query: "north", shorthand: false)
        let original = try await client.search(query).result
        let row = try XCTUnwrap(original.sections.first?.rows.first)
        let preview = try await client.preview(PickerPreviewRequest(
            entries: [PickerPreviewEntry(cite: row.cite, form: nil, item: row.id, locator: nil, prefix: nil, suffix: nil)],
            format: "apa", keepOrder: false
        )).result
        XCTAssertNil(preview.problem)
        let old = try String(contentsOf: fixture.library, encoding: .utf8)
        let changed = old.replacingOccurrences(of: "North Quay", with: "North Quay after rain")
        try changed.write(to: fixture.library, atomically: true, encoding: .utf8)
        let deadline = ContinuousClock.now + .seconds(10)
        while try await client.search(query).result.sections.first?.rows.first?.cite == row.cite {
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
        let query = PickerSearchRequest(query: "north", shorthand: false)
        let before = try await client.search(query)

        fixture.stop(signal: SIGKILL)
        try await Task.sleep(for: .milliseconds(30))
        let recovered = try await client.search(query)
        XCTAssertEqual(recovered.result.sections, before.result.sections)
        XCTAssertNotEqual(recovered.session, before.session)
        let restarted = try fixture.identifyDaemon()
        XCTAssertNotEqual(original, restarted)

        let newerBinary = fixture.root.appendingPathComponent("ibis-newer")
        try FileManager.default.copyItem(at: fixture.binary, to: newerBinary)
        let modified = try XCTUnwrap(FileManager.default.attributesOfItem(atPath: fixture.binary.path)[.modificationDate] as? Date)
        try FileManager.default.setAttributes([.modificationDate: modified.addingTimeInterval(60)],
                                              ofItemAtPath: newerBinary.path)
        let upgraded = fixture.client(binary: newerBinary)
        _ = try await upgraded.open(fixture.opening)
        let upgradedRows = try await upgraded.search(query)
        XCTAssertEqual(upgradedRows.result.sections, before.result.sections)
        let replacement = try fixture.identifyDaemon(binary: newerBinary)
        XCTAssertNotEqual(replacement, restarted)

        do {
            _ = try await client.search(query)
            XCTFail("The older client displaced the newer engine.")
        } catch {
            XCTAssertEqual(error as? DaemonError, .newerEngine)
        }
    }
}
