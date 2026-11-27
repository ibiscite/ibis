import AppKit
import CoreGraphics
import XCTest
@testable import IbisNative

final class PickerActionsAccessibilityTests: XCTestCase {
    func testLargeTextActionsFitTheAvailableScreenEdges() {
        let screen = NSRect(x: -1180, y: 45, width: 1180, height: 760)
        let size = NSSize(width: 704, height: 692)
        let anchors: [(rect: NSRect, edge: NSRectEdge, height: CGFloat)] = [
            (NSRect(x: -160, y: 63, width: 64, height: 52), .maxY, 662),
            (NSRect(x: -1160, y: 740, width: 64, height: 52), .minY, 667),
            (NSRect(x: -1010, y: 399, width: 64, height: 52), .maxX, 692),
            (NSRect(x: -230, y: 399, width: 64, height: 52), .minX, 692),
        ]
        for (anchor, edge, height) in anchors {
            let placement = PickerActionsPlacement(size: size, anchor: anchor, visibleFrame: screen,
                                                   minimumWidth: 480, minimumHeight: 488)
            XCTAssertEqual(placement.edge, edge)
            XCTAssertEqual(placement.size, NSSize(width: 704, height: height))
        }
    }

    @MainActor
    func testConstrainedActionsKeepSearchAndFooterReachableAtLargeText() throws {
        guard CGSessionCopyCurrentDictionary() != nil else { throw XCTSkip("A WindowServer session is needed.") }
        _ = NSApplication.shared
        let toggle = PickerCommand.typeCitation.action(checked: false)
        let controller = PickerActionsView(actions: [toggle, PickerCommand.settings.action()],
                                           metrics: PickerMetrics(scale: 2, density: .compact))
        let screen = NSRect(x: 0, y: 0, width: 940, height: 640)
        let placement = controller.placement(around: NSRect(x: 280, y: 300, width: 380, height: 56), in: screen)
        XCTAssertEqual(placement.edge, .minY)
        XCTAssertEqual(placement.size, NSSize(width: 704, height: 272))
        let window = NSWindow(contentRect: NSRect(origin: .zero, size: placement.size), styleMask: .titled,
                              backing: .buffered, defer: false)
        window.contentViewController = controller
        window.setContentSize(placement.size)
        let content = try XCTUnwrap(window.contentView)
        content.layoutSubtreeIfNeeded()

        let back = try XCTUnwrap(content.subviews.compactMap { $0 as? PickerButton }.first { $0.accessibilityLabel() == PickerWords.text(.backFromActions) })
        let run = try XCTUnwrap(content.subviews.compactMap { $0 as? PickerButton }.first { $0.accessibilityLabel() == PickerWords.text(.runAction) })
        let scroll = try XCTUnwrap(content.subviews.compactMap { $0 as? NSScrollView }.first)
        for child in [controller.search, back, run, scroll] as [NSView] {
            XCTAssertTrue(content.bounds.contains(child.frame))
            XCTAssertGreaterThan(child.frame.height, 0)
        }
        XCTAssertFalse(back.frame.intersects(run.frame))
        XCTAssertEqual(controller.search.font?.pointSize, 26)
        XCTAssertGreaterThanOrEqual(scroll.contentView.bounds.height, 60)
        XCTAssertTrue(run.accessibilityHelp()?.contains(toggle.title) == true)
        XCTAssertTrue(run.accessibilityHelp()?.contains(PickerWords.text(.actionStateOff)) == true)

        let table = try XCTUnwrap(scroll.documentView as? NSTableView)
        let cell = try XCTUnwrap(table.view(atColumn: 0, row: table.selectedRow, makeIfNecessary: true))
        XCTAssertEqual(cell.accessibilityRole(), .checkBox)
        XCTAssertEqual(cell.accessibilityLabel(), toggle.title)
        XCTAssertEqual((cell.accessibilityValue() as? NSNumber)?.boolValue, false)
        XCTAssertTrue(cell.subviews.allSatisfy { $0.isAccessibilityHidden() })
    }

    @MainActor
    func testShownActionsStayOnScreenWhenTheAnchorMovesBetweenCorners() async throws {
        guard CGSessionCopyCurrentDictionary() != nil, let screen = NSScreen.main else {
            throw XCTSkip("A WindowServer session and display are needed.")
        }
        let application = NSApplication.shared
        let fixture = try Fixture()
        let popover = PickerPopover(model: PickerModel(engine: fixture.client()))
        let owner = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 244, height: 168), styleMask: .titled,
                             backing: .buffered, defer: false)
        let anchor = NSButton(frame: NSRect(x: 160, y: 12, width: 68, height: 46))
        let ownerContent = try XCTUnwrap(owner.contentView)
        ownerContent.addSubview(anchor)
        let display = screen.visibleFrame
        owner.setFrameOrigin(NSPoint(x: display.maxX - owner.frame.width - 12, y: display.minY + 12))
        owner.orderFrontRegardless()
        defer {
            popover.close()
            owner.orderOut(nil)
        }

        func actionSearch(in view: NSView) -> PickerSearchField? {
            if let search = view as? PickerSearchField, search.accessibilityLabel() == PickerWords.text(.searchActions) {
                return search
            }
            return view.subviews.lazy.compactMap { actionSearch(in: $0) }.first
        }
        func actionSearchFields() -> [PickerSearchField] {
            application.windows.filter(\.isVisible).compactMap { $0.contentView.flatMap { actionSearch(in: $0) } }
        }
        let previousFields = Set(actionSearchFields().map(ObjectIdentifier.init))
        let command = PickerCommand.settings.action()
        popover.showActions([command], at: anchor, metrics: PickerMetrics(scale: 2, density: .compact))
        try await Task.sleep(for: .milliseconds(220))
        let search = try XCTUnwrap(actionSearchFields().first { !previousFields.contains(ObjectIdentifier($0)) })
        let window = try XCTUnwrap(search.window)
        let content = try XCTUnwrap(search.superview)
        let back = try XCTUnwrap(content.subviews.compactMap { $0 as? PickerButton }.first { $0.accessibilityLabel() == PickerWords.text(.backFromActions) })
        let run = try XCTUnwrap(content.subviews.compactMap { $0 as? PickerButton }.first { $0.accessibilityLabel() == PickerWords.text(.runAction) })

        for origin in [owner.frame.origin,
                       NSPoint(x: display.minX + 12, y: display.maxY - owner.frame.height - 12)] {
            owner.setFrameOrigin(origin)
            try await Task.sleep(for: .milliseconds(220))
            content.layoutSubtreeIfNeeded()
            XCTAssertTrue(window.isVisible)
            for control in [search, back, run] as [NSView] {
                let frame = window.convertToScreen(control.convert(control.bounds, to: nil))
                XCTAssertTrue(display.contains(frame), "An action control fell beyond the visible screen: \(frame)")
            }
            XCTAssertFalse(run.isHidden)
        }
        XCTAssertTrue(search.window === window)
        XCTAssertEqual(search.font?.pointSize, 26)
    }
}
