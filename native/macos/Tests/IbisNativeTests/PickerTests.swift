import AppKit
import CoreGraphics
import XCTest
@testable import IbisNative

final class PickerTests: XCTestCase {
    @MainActor
    func testSelectionAndDrawerUseOnlyTheCurrentPreview() async throws {
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
        model.details.form = .narrative
        model.details.term = .page
        model.details.locator = "12"
        model.details.prefix = "See "
        XCTAssertFalse(model.canDeliver)
        try await eventually { model.preview != nil }
        XCTAssertNil(model.preview?.problem)
        XCTAssertTrue(model.preview?.text.contains("12") == true)
        XCTAssertTrue(model.preview?.text.contains("See") == true)

        model.query = "north"
        XCTAssertFalse(model.canDeliver)
        try await eventually { model.canDeliver && model.rows.count == 1 }
        XCTAssertEqual(model.selected, first)
        model.drawerOpen = true
        XCTAssertTrue(model.escape())
        XCTAssertFalse(model.drawerOpen)
        XCTAssertTrue(model.escape())
        XCTAssertTrue(model.query.isEmpty)
        XCTAssertFalse(model.escape())
    }

    @MainActor
    func testNonactivatingPanelTakesSearchAndArrowKeys() async throws {
        guard CGMainDisplayID() != 0, CGSessionCopyCurrentDictionary() != nil else {
            throw XCTSkip("A WindowServer session is needed to host an AppKit panel.")
        }
        _ = NSApplication.shared
        let fixture = try Fixture()
        try fixture.identifyDaemon()
        let model = PickerModel(engine: fixture.client(), file: fixture.library)
        let panel = PickerPanel(model: model, chooseLibrary: {})
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
        try send("\u{f700}", key: 126, to: panel) // Up
        XCTAssertEqual(model.selected, first)
        try send("\u{1b}", key: 53, to: panel)
        XCTAssertFalse(panel.isVisible)
    }

    @MainActor
    private func send(_ character: String, key: UInt16, to panel: PickerPanel) throws {
        let event = try XCTUnwrap(NSEvent.keyEvent(
            with: .keyDown, location: .zero, modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime,
            windowNumber: panel.windowNumber, context: nil, characters: character, charactersIgnoringModifiers: character,
            isARepeat: false, keyCode: key
        ))
        panel.sendEvent(event)
    }
}
