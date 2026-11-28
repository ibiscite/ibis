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
    func testSearchingActionsPreservesTheQueryAndDraft() async throws {
        guard CGSessionCopyCurrentDictionary() != nil else { throw XCTSkip("A WindowServer session is needed.") }
        _ = NSApplication.shared
        let fixture = try Fixture()
        try fixture.identifyDaemon()
        let model = PickerModel(engine: fixture.client(), file: fixture.library)
        let panel = PickerPanel(model: model, chooseLibrary: { _ in })
        let controller = panel.controller
        panel.summon()
        defer { panel.dismiss() }
        try await eventually { model.canDeliver }
        model.query = "north"
        try await eventually { model.canDeliver && model.rows.count == 1 }
        let row = try XCTUnwrap(model.selection)
        model.toggleCitation(row)
        model.changeDetails(for: row.id) { $0.locator = "417" }
        try await eventually { model.canDeliver }
        let expected = try XCTUnwrap(model.preview)
        let search = try XCTUnwrap(panel.searchField)
        search.focus(restoring: NSRange(location: 2, length: 0))

        try send("k", key: UInt16(kVK_ANSI_K), flags: [.command], to: panel)
        XCTAssertTrue(controller.actionsOpen)
        try await eventually { controller.actionsField?.hasFocus == true }
        try send("\u{1b}", key: UInt16(kVK_Escape), to: panel)
        XCTAssertFalse(controller.actionsOpen)
        XCTAssertTrue(search.hasFocus)
        XCTAssertEqual(search.currentEditor()?.selectedRange, NSRange(location: 2, length: 0))

        try send("k", key: UInt16(kVK_ANSI_K), flags: [.command], to: panel)
        controller.actionsQuery = "invented no action"
        XCTAssertNil(controller.highlighted)
        try send("\r", key: UInt16(kVK_Return), to: panel)
        XCTAssertTrue(controller.actionsOpen, "Return with no actions must not close the list.")
        controller.actionsQuery = "locator"
        XCTAssertEqual(controller.highlighted?.command, .page)
        try send("\r", key: UInt16(kVK_Return), to: panel)
        XCTAssertFalse(controller.actionsOpen)
        XCTAssertTrue(controller.locatorField?.hasFocus == true)
        XCTAssertEqual(model.query, "north")
        XCTAssertEqual(model.citationItem, row.id)
        XCTAssertEqual(model.details(for: row.id).locator, "417")
        XCTAssertEqual(model.preview?.ticket, expected.ticket)

        let page = try XCTUnwrap(controller.locatorField)
        page.currentEditor()?.selectedRange = NSRange(location: 1, length: 1)
        try send("k", key: UInt16(kVK_ANSI_K), flags: [.command], to: panel)
        try send("\u{1b}", key: UInt16(kVK_Escape), to: panel)
        XCTAssertTrue(page.hasFocus)
        XCTAssertEqual(page.currentEditor()?.selectedRange, NSRange(location: 1, length: 1))
        try send("k", key: UInt16(kVK_ANSI_K), flags: [.command], to: panel)
        try send("\t", key: UInt16(kVK_Tab), to: panel)
        XCTAssertFalse(controller.actionsOpen)
        XCTAssertTrue(search.hasFocus)
        XCTAssertEqual(model.preview?.ticket, expected.ticket)
    }

    @MainActor
    func testActionHighlightOnlyKeepsAStillValidReturnTarget() async throws {
        let fixture = try Fixture()
        try fixture.identifyDaemon()
        let model = PickerModel(engine: fixture.client(), file: fixture.library)
        let controller = PickerController(model: model)
        model.present(app: fixture.opening.app)
        defer { model.dismiss() }
        try await eventually { model.canDeliver && model.rows.count > 1 }
        controller.showActions()
        controller.actionsQuery = "peek"
        XCTAssertEqual(controller.highlighted?.command, .peek)
        let end = try XCTUnwrap(NSEvent.keyEvent(
            with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0, windowNumber: 0, context: nil,
            characters: "\u{f72b}", charactersIgnoringModifiers: "\u{f72b}", isARepeat: false, keyCode: UInt16(kVK_End)))
        XCTAssertFalse(controller.actionKey(end), "End belongs to the search field while editing.")

        // Peek names the highlighted reference; a new highlight retires the old target.
        model.move(1)
        controller.refreshActions(newQuery: false)
        XCTAssertNil(controller.highlighted)
        XCTAssertEqual(controller.actionsQuery, "peek")
        controller.moveAction(1)
        XCTAssertEqual(controller.highlighted?.reference, model.selected)
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
        XCTAssertTrue(panel.canBecomeKey)
        XCTAssertFalse(panel.canBecomeMain)
        XCTAssertEqual(NSWorkspace.shared.menuBarOwningApplication?.processIdentifier, owner)
        try await eventually { model.canDeliver }
        let search = try XCTUnwrap(panel.searchField)
        XCTAssertTrue(search.hasFocus)
        XCTAssertEqual(search.accessibilityLabel(), "Search references")
        XCTAssertTrue(controller.quickDelivery, "With no draft the highlighted row is what Return delivers.")

        let first = model.selected
        try send("\u{f701}", key: 125, to: panel) // Down
        XCTAssertNotEqual(model.selected, first)
        try await eventually { model.canDeliver }
        try send("\r", key: 36, flags: [.shift], to: panel)
        XCTAssertEqual(model.citations.map(\.row.id), [model.selected].compactMap { $0 })
        XCTAssertFalse(controller.quickDelivery, "A draft moves the emphasis to the delivery button.")
        try await eventually { model.canDeliver }
        try send("i", key: 34, flags: [.command], to: panel)
        XCTAssertEqual(model.editingCitation, model.selected)
        XCTAssertEqual(controller.layer, .details(try XCTUnwrap(model.selected)))
        controller.closeLayer(restoring: true)
        XCTAssertNil(model.editingCitation)
        try send("l", key: 37, flags: [.command], to: panel)
        XCTAssertTrue(controller.locatorField?.hasFocus == true)
        try send("\t", key: 48, to: panel)
        XCTAssertTrue(search.hasFocus)
        try send("\u{f701}", key: 125, flags: [.command], to: panel) // ⌘↓ reaches the token
        XCTAssertEqual(controller.focus, .token(try XCTUnwrap(model.citationItem)))
        try send("\u{f700}", key: 126, to: panel) // Up returns to search
        XCTAssertTrue(search.hasFocus)
        try send("\u{f700}", key: 126, to: panel)
        XCTAssertEqual(model.selected, first)
        for _ in 0..<10 {
            try send("=", key: 24, flags: [.command], to: panel)
        }
        XCTAssertEqual(model.scale, 2)
        try await eventually { panel.frame.width > 720 }
        let screen = try XCTUnwrap(panel.screen)
        XCTAssertTrue(screen.visibleFrame.insetBy(dx: -1, dy: -1).contains(panel.frame))

        model.query = "invented"
        try await eventually { model.canDeliver && model.rows.count == 16 }
        controller.showNumbers(true)
        try await eventually { !controller.visibleRows.isEmpty }
        let draft = model.citations.map(\.row.id)
        let target = controller.visibleRows[0]
        XCTAssertEqual(controller.number(for: target), 1)
        try send("1", key: UInt16(kVK_ANSI_1), flags: [.command], to: panel)
        XCTAssertEqual(model.selected, target)
        XCTAssertEqual(model.citations.map(\.row.id), draft)

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
