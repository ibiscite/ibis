import AppKit
import CoreGraphics
import XCTest
@testable import IbisNative

final class PickerTests: XCTestCase {
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
        let owner = NSWorkspace.shared.menuBarOwningApplication?.processIdentifier
        panel.summon()
        defer { panel.dismiss() }
        XCTAssertTrue(panel.styleMask.contains(.nonactivatingPanel))
        XCTAssertTrue(panel.canBecomeKey)
        XCTAssertFalse(panel.canBecomeMain)
        XCTAssertEqual(NSWorkspace.shared.menuBarOwningApplication?.processIdentifier, owner)
        try await eventually { model.canDeliver }
        XCTAssertTrue(panel.searchField?.currentEditor() === panel.firstResponder)
        XCTAssertEqual(panel.searchField?.accessibilityLabel(), "Search references")

        let first = model.selected
        try send("\u{f701}", key: 125, to: panel) // Down
        XCTAssertNotEqual(model.selected, first)
        try send("\r", key: 36, flags: [.shift], to: panel)
        XCTAssertEqual(model.citations.map(\.row.id), [model.selected].compactMap { $0 })
        try await eventually { model.canDeliver }
        let beforePopover = try XCTUnwrap(resultTable(in: panel)?.enclosingScrollView?.frame)
        try send("i", key: 34, flags: [.command], to: panel)
        XCTAssertEqual(model.editingCitation, model.selected)
        XCTAssertEqual(resultTable(in: panel)?.enclosingScrollView?.frame, beforePopover)
        try send("\u{1b}", key: 53, to: panel)
        XCTAssertNil(model.editingCitation)
        try send("l", key: 37, flags: [.command], to: panel)
        let page = try XCTUnwrap(descendants(of: try XCTUnwrap(panel.contentView))
            .compactMap { $0 as? NSTextField }.first { $0.accessibilityLabel() == PickerWords.text(.page) })
        XCTAssertTrue(page.currentEditor() === panel.firstResponder)
        XCTAssertEqual(page.stringValue, "")
        try send("\t", key: 48, to: panel)
        XCTAssertTrue(panel.searchField?.currentEditor() === panel.firstResponder)
        panel.makeFirstResponder(panel.searchField)
        try send("\u{f700}", key: 126, to: panel) // Up
        XCTAssertEqual(model.selected, first)
        for _ in 0..<10 {
            try send("=", key: 24, flags: [.command], to: panel)
        }
        XCTAssertEqual(model.scale, 2)
        try await eventually { panel.frame.width > 720 }
        let frame = panel.frame
        panel.setFrame(NSRect(x: frame.minX, y: frame.maxY - 720, width: 960, height: 720), display: true)
        model.query = "invented"
        try await eventually { model.canDeliver && model.rows.count == 16 }
        let content = try XCTUnwrap(panel.contentView)
        let views = descendants(of: content)
        let scroll = try XCTUnwrap(views.compactMap { $0 as? NSScrollView }
            .first { $0.documentView is NSTableView })
        let table = try XCTUnwrap(scroll.documentView as? NSTableView)
        XCTAssertEqual(table.rowHeight, 72)
        XCTAssertGreaterThan(table.rect(ofRow: table.numberOfRows - 1).maxY, scroll.contentView.bounds.height)
        let library = try XCTUnwrap(views.compactMap { $0 as? NSButton }
            .first { $0.accessibilityLabel() == PickerWords.text(.library) })
        XCTAssertTrue(library.isHidden)
        let buttons = try [model.deliveryTitle, PickerWords.text(.more)].map { label in
            try XCTUnwrap(views.compactMap { $0 as? NSButton }
                .first { $0.accessibilityLabel() == label && !$0.isHidden })
        }
        let placements = buttons.map { $0.convert($0.bounds, to: content) }
        XCTAssertTrue(placements.allSatisfy { $0.height == 56 && content.bounds.contains($0) })
        XCTAssertTrue(zip(placements, placements.dropFirst()).allSatisfy { $0.maxX < $1.minX })
        model.query = ""
        try await eventually { model.canDeliver }
        try send("\u{1b}", key: 53, to: panel)
        try await eventually { !panel.isVisible }
    }

    @MainActor
    private func descendants(of view: NSView) -> [NSView] {
        view.subviews.flatMap { [$0] + descendants(of: $0) }
    }

    @MainActor
    private func resultTable(in panel: PickerPanel) -> NSTableView? {
        panel.contentView.flatMap { descendants(of: $0).compactMap { $0 as? NSTableView }.first }
    }

    @MainActor
    private func send(_ character: String, key: UInt16, flags: NSEvent.ModifierFlags = [], to panel: PickerPanel) throws {
        let event = try XCTUnwrap(NSEvent.keyEvent(
            with: .keyDown, location: .zero, modifierFlags: flags, timestamp: ProcessInfo.processInfo.systemUptime,
            windowNumber: panel.windowNumber, context: nil, characters: character, charactersIgnoringModifiers: character,
            isARepeat: false, keyCode: key
        ))
        panel.sendEvent(event)
    }
}
