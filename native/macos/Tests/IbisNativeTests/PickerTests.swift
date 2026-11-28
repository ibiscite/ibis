import AppKit
import Carbon
import CoreGraphics
import XCTest
@testable import IbisNative

final class PickerTests: XCTestCase {
    @MainActor
    func testOpeningShortcutPreservesIntentAndRejectsPickerCommands() throws {
        let logical = try OpeningShortcut.parse("Cmd+Option+Shift+Y")
        let physical = try OpeningShortcut.parse("Cmd+Option+Shift+KeyY")
        XCTAssertEqual(logical, .repoDefault)
        XCTAssertEqual(logical.savedValue, "Alt+Shift+Command+Y")
        XCTAssertEqual(physical.savedValue, "Alt+Shift+Command+KeyY")
        XCTAssertNotEqual(logical, physical)
        for (input, expected) in [("Shift+Y", ShortcutRefusal.needsModifier),
                                  ("Command+Q", .reserved), ("Option+Enter", .reserved),
                                  ("Control+Unknown", .invalid)] {
            XCTAssertThrowsError(try OpeningShortcut.parse(input)) { XCTAssertEqual($0 as? ShortcutRefusal, expected) }
        }
    }

    @MainActor
    func testHotKeyKeptUntilReplacementIsCommitted() throws {
        guard CGSessionCopyCurrentDictionary() != nil else { throw XCTSkip("A WindowServer session is needed.") }
        _ = NSApplication.shared
        let hotKey = HotKey {}
        let first = try OpeningShortcut.parse("Control+Alt+Shift+Command+F18")
        let next = try OpeningShortcut.parse("Control+Alt+Shift+Command+F19")
        guard hotKey.register(first) else { throw XCTSkip("The fixture shortcut is already in use.") }
        XCTAssertTrue(hotKey.reserve(next))
        XCTAssertEqual(hotKey.registered, first)
        hotKey.discardReservation()
        XCTAssertEqual(hotKey.registered, first)
        XCTAssertTrue(hotKey.reserve(next))
        hotKey.commitReservation()
        XCTAssertEqual(hotKey.registered, next)
    }

    @MainActor
    func testSettingsRecorderCancelsAndSavesItsAcceptedShortcut() async throws {
        guard CGSessionCopyCurrentDictionary() != nil else { throw XCTSkip("A WindowServer session is needed.") }
        _ = NSApplication.shared
        let fixture = try Fixture()
        try fixture.identifyDaemon()
        let engine = fixture.client()
        let shortcuts = OpeningShortcuts(engine: engine, action: {}, updateMenu: { _ in })
        let settings = NativeSettingsWindow(engine: engine, shortcuts: shortcuts) { _, _ in }
        let window = try XCTUnwrap(settings.window)
        let flags: NSEvent.ModifierFlags = [.control, .option, .shift, .command]
        let old = try OpeningShortcut.parse("Control+Alt+Shift+Command+F18")
        let writ = try await engine.settings(.patch(shortcut: ShortcutSetting(binding: old.savedValue)))
        shortcuts.restore(writ.shortcut)
        guard shortcuts.active == old else { throw XCTSkip("The fixture shortcut is already in use.") }

        shortcuts.beginRecording()
        try send("\u{1b}", key: UInt16(kVK_Escape), to: window)
        XCTAssertFalse(shortcuts.recording)
        XCTAssertEqual(shortcuts.active, old)

        shortcuts.beginRecording()
        let quit = try keyEvent("q", key: UInt16(kVK_ANSI_Q), flags: [.command], to: window)
        XCTAssertTrue(window.performKeyEquivalent(with: quit))
        XCTAssertEqual(shortcuts.issue, .shortcutReserved)
        XCTAssertEqual(shortcuts.active, old)

        shortcuts.beginRecording()
        let f19 = try XCTUnwrap(UnicodeScalar(NSF19FunctionKey))
        try send(String(f19), key: UInt16(kVK_F19), flags: flags, to: window)
        try await eventually { !shortcuts.saving }
        let saved = try await fixture.client().settings(.readOnly)
        XCTAssertEqual(saved.shortcut, "Control+Alt+Shift+Command+F19")
        XCTAssertEqual(shortcuts.active?.savedValue, saved.shortcut)
    }

    @MainActor
    func testTypedPagesAndDetailsStayWithTheirReference() async throws {
        let fixture = try Fixture()
        try fixture.identifyDaemon()
        let store = PickerStore(engine: fixture.client(), file: fixture.library)
        store.present(app: fixture.opening.app)
        defer { store.dismiss() }
        try await eventually { store.canDeliver && store.settled }
        store.useFormatOnce("apa")
        let first = try XCTUnwrap(store.highlight)
        store.move(1)
        try await eventually { store.settled && store.highlight != first }
        let second = try XCTUnwrap(store.highlight)

        // What is typed around a search becomes the highlighted reference's page and prefix.
        store.type("see north 12")
        try await eventually { store.settled && store.canDeliver }
        XCTAssertEqual(store.citation.first?.details.prefix, "see")
        XCTAssertEqual(store.citation.first?.note, "p. 12")
        XCTAssertNotNil(store.state?.readsAs)
        XCTAssertTrue(store.state?.preview?.text.contains("12") == true)

        // Tab adds it and clears the field; the citation stays while browsing.
        store.add()
        XCTAssertEqual(store.query, "")
        try await eventually { store.settled && store.added.count == 1 }
        let ticket = store.state?.preview?.ticket
        store.type("invented nonexistent result")
        try await eventually { store.settled && store.rows.isEmpty }
        XCTAssertEqual(store.added.count, 1)
        XCTAssertEqual(store.state?.preview?.ticket, ticket)
        XCTAssertTrue(store.escape())
        try await eventually { store.settled && store.query.isEmpty }

        // Details edit one reference; a second added one keeps its own.
        let added = try XCTUnwrap(store.added.first)
        store.changeDetails(added.row.id, to: added.details.with(suffix: " under glass"))
        try await eventually { store.settled && store.state?.preview?.text.contains("under glass") == true }
        store.toggle(added.row.id == second ? first : second)
        try await eventually { store.settled && store.added.count == 2 }
        XCTAssertEqual(store.entry(added.row.id)?.details.suffix, " under glass")
        XCTAssertTrue(store.deleteInEmptyField())
        try await eventually { store.settled && store.added.count == 1 }
        store.undo()
        try await eventually { store.settled && store.added.count == 2 }
    }

    @MainActor
    func testOpeningWithoutLibraryAndSwitchingSourcesPreservesSearch() async throws {
        guard CGSessionCopyCurrentDictionary() != nil else { throw XCTSkip("A WindowServer session is needed.") }
        _ = NSApplication.shared
        let fixture = try Fixture()
        try fixture.identifyDaemon()
        let engine = fixture.client()
        let store = PickerStore(engine: engine)
        var choseLibrary = false
        let panel = PickerPanel(store: store, chooseLibrary: { _ in choseLibrary = true })
        panel.summon()
        defer { panel.dismiss() }
        try await eventually { store.catalog != nil }
        XCTAssertFalse(store.libraryReady)
        XCTAssertEqual(store.emptyState, .choose)
        try send("\r", key: UInt16(kVK_Return), to: panel)
        XCTAssertTrue(choseLibrary)
        _ = try await engine.chooseLibrary(fixture.library)
        store.useSavedLibrary()
        try await eventually { store.canDeliver && store.rows.count == 2 }
        store.type("invented")
        try await eventually { store.settled && store.rows.count == 2 }
        store.add()
        try await eventually { store.settled && store.added.count == 1 }

        let replacement = fixture.root.appendingPathComponent("Replacement.csljson")
        try Data("""
        [{"id":"replacement","citation-key":"invented-replacement","type":"book",
          "title":"Invented replacement","author":[{"family":"Vale"}],"issued":{"date-parts":[[2024]]}}]
        """.utf8).write(to: replacement)
        store.type("invented")
        try await eventually { store.settled }
        _ = try await engine.chooseLibrary(replacement)
        store.useSavedLibrary()
        try await eventually { store.settled && store.rows.count == 1 }
        XCTAssertEqual(store.query, "invented", "A new library keeps the search")
        XCTAssertTrue(store.added.isEmpty, "and starts a new citation.")
        XCTAssertTrue(store.canDeliver, store.state?.preview?.problem?.text ?? "The new citation should be ready.")
    }

    @MainActor
    func testOneTimeFormatKeepsTheCitationAndExpiresAtTheNextOpening() async throws {
        let fixture = try Fixture()
        try fixture.identifyDaemon()
        let store = PickerStore(engine: fixture.client(), file: fixture.library)
        store.present(app: "com.apple.TextEdit", name: "TextEdit")
        defer { store.dismiss() }
        try await eventually { store.canDeliver && store.settled }
        let initial = store.state?.format
        store.type("north 417")
        try await eventually { store.settled }
        store.add()
        try await eventually { store.settled && store.added.count == 1 }
        store.useFormatOnce("pandoc")
        try await eventually { store.settled && store.state?.format == "pandoc" }
        XCTAssertEqual(store.state?.formatHeld, true)
        XCTAssertEqual(store.added.first?.note, "p. 417")
        store.present(app: "com.apple.TextEdit", name: "TextEdit")
        try await eventually { store.settled && store.state?.format == initial }
        XCTAssertEqual(store.state?.formatHeld, false)
        XCTAssertTrue(store.added.isEmpty)
    }

    @MainActor
    func testNonactivatingPanelTakesSearchAndArrowKeys() async throws {
        guard CGMainDisplayID() != 0, CGSessionCopyCurrentDictionary() != nil else {
            throw XCTSkip("A WindowServer session is needed to host an AppKit panel.")
        }
        _ = NSApplication.shared
        let fixture = try Fixture()
        var records = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: fixture.library)) as? [[String: Any]])
        for index in 0..<14 {
            records.append(["id": "invented-window-\(index)", "type": "book", "title": "Invented window \(index)"])
        }
        try JSONSerialization.data(withJSONObject: records).write(to: fixture.library)
        try fixture.identifyDaemon()
        let store = PickerStore(engine: fixture.client(), file: fixture.library)
        let panel = PickerPanel(store: store, chooseLibrary: { _ in })
        let owner = NSWorkspace.shared.menuBarOwningApplication?.processIdentifier
        panel.summon()
        defer { panel.dismiss() }
        XCTAssertTrue(panel.styleMask.contains(.nonactivatingPanel))
        XCTAssertFalse(panel.styleMask.contains(.resizable))
        XCTAssertTrue(panel.canBecomeKey)
        XCTAssertFalse(panel.canBecomeMain)
        XCTAssertEqual(NSWorkspace.shared.menuBarOwningApplication?.processIdentifier, owner)
        store.type("invented")
        try await eventually { store.settled && store.canDeliver && store.rows.count == 16 }
        let search = try XCTUnwrap(panel.searchField)
        XCTAssertTrue(search.hasFocus)
        XCTAssertEqual(search.accessibilityLabel(), "Search references")
        try await eventually { abs(panel.frame.height - store.height(PickerGrid(s: store.scale))) < 1 }

        // Down moves the highlight; Tab adds it and clears the field for the next search.
        let first = store.highlight
        try send("\u{f701}", key: 125, to: panel) // Down
        try await eventually { store.settled && store.highlight != first }
        let added = try XCTUnwrap(store.highlight)
        try send("\t", key: 48, to: panel)
        XCTAssertEqual(store.query, "")
        try await eventually { store.settled && store.added.map(\.row.id) == [added] }
        XCTAssertTrue(search.hasFocus)

        // ⌘L opens the page popover for the citation; Escape closes it.
        try send("l", key: 37, flags: [.command], to: panel)
        XCTAssertEqual(store.editing, added)
        try send("\u{1b}", key: 53, to: panel)
        XCTAssertNil(store.editing)

        // Delete in an empty field takes back the last added reference.
        try send("\u{7f}", key: 51, to: panel)
        try await eventually { store.settled && store.added.isEmpty }

        for _ in 0..<10 {
            try send("=", key: 24, flags: [.command], to: panel)
        }
        XCTAssertEqual(store.scale, 2)
        try await eventually { panel.frame.width > 700 }
        let screen = try XCTUnwrap(panel.screen)
        XCTAssertTrue(screen.visibleFrame.insetBy(dx: -1, dy: -1).contains(panel.frame))

        store.type("invented")
        try await eventually { store.settled && store.rows.count == 16 }
        store.showNumbers(true)
        try await eventually { !store.visibleRows.isEmpty }
        let target = store.visibleRows[0]
        XCTAssertEqual(store.number(for: target), 1)
        XCTAssertTrue(store.selectNumber(1))
        try await eventually { store.settled && store.highlight == target }

        try send("\u{1b}", key: 53, to: panel) // Clears the field.
        try send("\u{1b}", key: 53, to: panel) // Closes the picker.
        try await eventually { !panel.isVisible }
    }

    @MainActor
    private func send(_ character: String, key: UInt16, flags: NSEvent.ModifierFlags = [], to window: NSWindow) throws {
        window.sendEvent(try keyEvent(character, key: key, flags: flags, to: window))
    }

    @MainActor
    private func keyEvent(_ character: String, key: UInt16, flags: NSEvent.ModifierFlags, to window: NSWindow) throws -> NSEvent {
        try XCTUnwrap(NSEvent.keyEvent(
            with: .keyDown, location: .zero, modifierFlags: flags, timestamp: ProcessInfo.processInfo.systemUptime,
            windowNumber: window.windowNumber, context: nil, characters: character, charactersIgnoringModifiers: character,
            isARepeat: false, keyCode: key
        ))
    }
}
