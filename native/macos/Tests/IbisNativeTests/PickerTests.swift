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
    func testLocatorAndPopoverEditsStayWithTheirReference() async throws {
        let fixture = try Fixture()
        try fixture.identifyDaemon()
        let model = PickerModel(engine: fixture.client(), file: fixture.library)
        model.present(app: fixture.opening.app)
        defer { model.dismiss() }
        try await eventually { model.canDeliver }
        let first = model.selected
        model.move(1)
        XCTAssertNotEqual(first, model.selected)
        XCTAssertFalse(model.canDeliver)
        try await eventually { model.canDeliver }

        model.format = "apa"
        let second = try XCTUnwrap(model.citationItem)
        model.changeDetails(for: second) {
            $0.form = .narrative
            $0.locator = "12"
            $0.prefix = "See "
        }
        XCTAssertFalse(model.canDeliver)
        try await eventually { model.canDeliver }
        XCTAssertNil(model.preview?.problem)
        XCTAssertTrue(model.preview?.text.contains("12") == true)
        XCTAssertTrue(model.preview?.text.contains("See") == true)

        model.query = "north"
        XCTAssertFalse(model.canDeliver)
        try await eventually { model.canDeliver && model.rows.count == 1 }
        XCTAssertEqual(model.selected, first)
        XCTAssertTrue(model.escape())
        XCTAssertTrue(model.query.isEmpty)
        XCTAssertFalse(model.escape())
        try await eventually { model.canDeliver && model.rows.count == 2 }

        let firstItem = try XCTUnwrap(first)
        model.toggleCitation(try XCTUnwrap(model.rows.first { $0.id == firstItem }))
        model.changeDetails(for: firstItem) { $0.locator = "7" }
        try await eventually { model.canDeliver }
        let draft = try XCTUnwrap(model.preview)
        model.select(second)
        XCTAssertTrue(model.canDeliver)
        XCTAssertEqual(model.citationItem, firstItem)
        XCTAssertEqual(model.preview?.ticket, draft.ticket)

        model.query = "invented nonexistent result"
        XCTAssertTrue(model.canDeliver)
        try await eventually { model.rows.isEmpty }
        XCTAssertTrue(model.canDeliver)
        XCTAssertEqual(model.citationItem, firstItem)
        XCTAssertEqual(model.preview?.ticket, draft.ticket)
        XCTAssertTrue(model.escape())
        try await eventually { model.rows.count == 2 }
        XCTAssertEqual(model.preview?.ticket, draft.ticket)

        model.toggleCitation(try XCTUnwrap(model.rows.first { $0.id == second }))
        XCTAssertEqual(model.citationItem, second)
        XCTAssertEqual(model.details(for: second).locator, "12")
        model.changeDetails(for: second) { $0.form = nil }
        model.beginCitationEdit(firstItem)
        XCTAssertEqual(model.editingCitation, firstItem)
        XCTAssertEqual(model.citationItem, firstItem)
        XCTAssertEqual(model.details(for: firstItem).locator, "7")
        model.changeDetails(for: firstItem) { $0.suffix = " under glass" }
        model.endCitationEdit()
        try await eventually { model.previewEntries.contains { $0.details.suffix == " under glass" } }
        XCTAssertTrue(model.canDeliver, model.preview?.problem?.text ?? "The group is not ready.")
        XCTAssertTrue(model.preview?.text.contains("under glass") == true)
        XCTAssertEqual(model.details(for: second).locator, "12")
        XCTAssertTrue(model.preview?.text.contains("12") == true)
    }

    @MainActor
    func testOpeningWithoutLibraryAndSwitchingSourcesPreservesSearch() async throws {
        guard CGSessionCopyCurrentDictionary() != nil else { throw XCTSkip("A WindowServer session is needed.") }
        _ = NSApplication.shared
        let fixture = try Fixture()
        try fixture.identifyDaemon()
        let engine = fixture.client()
        let model = PickerModel(engine: engine)
        var choseLibrary = false
        let panel = PickerPanel(model: model, chooseLibrary: { _ in choseLibrary = true })
        panel.summon()
        defer { panel.dismiss() }
        try await eventually { model.catalog != nil }
        XCTAssertFalse(model.libraryReady)
        model.query = "invented"
        try send("\r", key: UInt16(kVK_Return), to: panel)
        XCTAssertTrue(choseLibrary)
        _ = try await engine.chooseLibrary(fixture.library)
        model.useSavedLibrary()
        try await eventually { model.canDeliver && model.rows.count == 2 }
        XCTAssertEqual(model.query, "invented")

        let previous = try XCTUnwrap(model.selection)
        model.toggleCitation(previous)
        model.changeDetails(for: previous.id) { $0.locator = "417" }
        let replacement = fixture.root.appendingPathComponent("Replacement.csljson")
        try Data("""
        [{"id":"replacement","citation-key":"invented-replacement","type":"book",
          "title":"Invented replacement","author":[{"family":"Vale"}],"issued":{"date-parts":[[2024]]}}]
        """.utf8).write(to: replacement)
        _ = try await engine.chooseLibrary(replacement)
        model.useSavedLibrary()
        try await eventually { model.preview != nil && model.rows.count == 1 }
        XCTAssertTrue(model.canDeliver, model.preview?.problem?.text ?? "The new citation should be ready.")
        XCTAssertEqual(model.query, "invented")
        XCTAssertFalse(model.hasDraft)
        XCTAssertNotEqual(model.citationItem, previous.id)
        XCTAssertFalse(model.preview?.text.contains("417") ?? true)
    }

    @MainActor
    func testOneTimeFormatKeepsTheDraftAndExpiresAtTheNextOpening() async throws {
        let fixture = try Fixture()
        try fixture.identifyDaemon()
        let model = PickerModel(engine: fixture.client(), file: fixture.library)
        model.present(app: "com.apple.TextEdit", name: "TextEdit")
        defer { model.dismiss() }
        try await eventually { model.canDeliver }
        let initial = model.format
        let row = try XCTUnwrap(model.selection)
        model.toggleCitation(row)
        model.changeDetails(for: row.id) { $0.locator = "417" }
        model.useFormatOnce("pandoc")
        try await eventually { model.canDeliver && model.format == "pandoc" }
        XCTAssertTrue(model.formatIsHeld)
        XCTAssertEqual(model.citationItem, row.id)
        XCTAssertEqual(model.details(for: row.id).locator, "417")
        model.present(app: "com.apple.TextEdit", name: "TextEdit")
        try await eventually { model.canDeliver && model.format == initial }
        XCTAssertFalse(model.formatIsHeld)
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
        let model = PickerModel(engine: fixture.client(), file: fixture.library)
        let panel = PickerPanel(model: model, chooseLibrary: { _ in })
        let controller = panel.controller
        let owner = NSWorkspace.shared.menuBarOwningApplication?.processIdentifier
        panel.summon()
        defer { panel.dismiss() }
        XCTAssertTrue(panel.styleMask.contains(.nonactivatingPanel))
        XCTAssertFalse(panel.styleMask.contains(.resizable))
        XCTAssertTrue(panel.canBecomeKey)
        XCTAssertFalse(panel.canBecomeMain)
        XCTAssertEqual(NSWorkspace.shared.menuBarOwningApplication?.processIdentifier, owner)
        model.query = "invented"
        try await eventually { model.canDeliver && model.rows.count == 14 }
        let search = try XCTUnwrap(panel.searchField)
        XCTAssertTrue(search.hasFocus)
        XCTAssertEqual(search.accessibilityLabel(), "Search references")
        XCTAssertTrue(controller.resultEmphasis, "The highlighted result is what Return delivers.")
        try await eventually { abs(panel.frame.height - controller.height(PickerGrid(s: model.scale))) < 1 }

        // Tab adds the highlighted result and clears the field for the next search.
        try send("\u{f701}", key: 125, to: panel) // Down
        let added = try XCTUnwrap(model.selected)
        try await eventually { model.canChangeSelectionMembership }
        try send("\t", key: 48, to: panel)
        XCTAssertEqual(model.citations.map(\.row.id), [added])
        XCTAssertEqual(model.query, "")
        XCTAssertTrue(search.hasFocus)

        // Up from the first result reaches the added reference; Return opens it in place.
        model.query = "invented"
        try await eventually { model.canDeliver && model.rows.count == 14 }
        for _ in 0..<20 where model.selected != model.rows.first?.id { try send("\u{f700}", key: 126, to: panel) }
        try send("\u{f700}", key: 126, to: panel)
        XCTAssertEqual(controller.pinned, added)
        XCTAssertFalse(controller.resultEmphasis)
        try send("\r", key: 36, to: panel)
        XCTAssertEqual(controller.editing, added)
        XCTAssertEqual(model.editingCitation, added)
        try await eventually { controller.locatorField?.hasFocus == true }
        try send("\u{1b}", key: 53, to: panel)
        XCTAssertNil(controller.editing)
        XCTAssertNil(model.editingCitation)
        XCTAssertTrue(search.hasFocus)

        // Delete in an empty field removes the highlighted added reference.
        model.query = ""
        try await eventually { model.canDeliver }
        try send("\u{7f}", key: 51, to: panel)
        XCTAssertTrue(model.citations.isEmpty)

        for _ in 0..<10 {
            try send("=", key: 24, flags: [.command], to: panel)
        }
        XCTAssertEqual(model.scale, 2)
        try await eventually { panel.frame.width > 700 }
        let screen = try XCTUnwrap(panel.screen)
        XCTAssertTrue(screen.visibleFrame.insetBy(dx: -1, dy: -1).contains(panel.frame))

        model.query = "invented"
        try await eventually { model.canDeliver && model.rows.count == 14 }
        controller.showNumbers(true)
        try await eventually { !controller.visibleRows.isEmpty }
        let target = controller.visibleRows[0]
        XCTAssertEqual(controller.number(for: target), 1)
        controller.selectNumber(1)
        XCTAssertEqual(model.selected, target)

        model.query = ""
        try await eventually { model.canDeliver }
        try send("\u{1b}", key: 53, to: panel)
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
