import AppKit
import CoreGraphics
import XCTest
@testable import IbisNative

final class SettingsSearchTests: XCTestCase {
    func testMatchesPaneControlsByLabelAndTaskWords() throws {
        var search = SettingsSearch()
        search.update(SettingsSearchRow.shortcut.title.uppercased())
        XCTAssertEqual(search.highlighted, .shortcut)

        search.update(PickerWords.text(.comfortable))
        XCTAssertEqual(search.highlighted, .density)
        XCTAssertEqual(search.highlighted?.pane, .appearance)

        search.update(SettingsSearchRow.library.title)
        XCTAssertEqual(search.highlighted, .library)
        XCTAssertEqual(search.highlighted?.pane, .library)

        search.update(SettingsPane.general.title)
        XCTAssertTrue(search.matches.contains(.shortcut))
        XCTAssertTrue(search.matches.contains(.paste))

        let hotkey = try XCTUnwrap(PickerWords.text(.settingsShortcutWords).split(whereSeparator: \.isWhitespace).last)
        search.update(String(hotkey))
        XCTAssertEqual(search.highlighted, .shortcut)
        if PickerWords.language == .enUS {
            search.update("clipboard")
            XCTAssertEqual(search.highlighted, .paste)
            search.update("dark theme")
            XCTAssertEqual(search.highlighted, .appearance)
        }

        search.update("ivoryquartzmissing")
        XCTAssertTrue(search.isSearching)
        search.move(1)
        XCTAssertNil(search.highlighted)
        search.update(" \n ")
        XCTAssertFalse(search.isSearching)
    }

    @MainActor
    func testReturnRevealsTheSettingPaneAndEscapeKeepsIt() throws {
        guard CGSessionCopyCurrentDictionary() != nil else { throw XCTSkip("A WindowServer session is needed.") }
        _ = NSApplication.shared
        let fixture = try Fixture()
        let engine = fixture.client()
        let shortcuts = OpeningShortcuts(engine: engine, action: {}, updateMenu: { _ in })
        let settings = NativeSettingsWindow(engine: engine, shortcuts: shortcuts) { _, _ in }
        let window = try XCTUnwrap(settings.window)
        let field = settings.searchField
        let editor = NSTextView()
        let originalFrame = window.frame

        field.stringValue = PickerWords.text(.compact)
        settings.controlTextDidChange(Notification(name: NSControl.textDidChangeNotification, object: field))
        XCTAssertTrue(settings.control(field, textView: editor, doCommandBy: #selector(NSResponder.insertNewline(_:))))
        XCTAssertEqual(window.title, SettingsPane.appearance.title)
        XCTAssertEqual(field.stringValue, "")
        XCTAssertEqual(window.frame, originalFrame)

        field.stringValue = "ivoryquartzmissing"
        settings.controlTextDidChange(Notification(name: NSControl.textDidChangeNotification, object: field))
        XCTAssertTrue(settings.control(field, textView: editor, doCommandBy: #selector(NSResponder.insertNewline(_:))))
        XCTAssertEqual(field.stringValue, "ivoryquartzmissing")
        XCTAssertTrue(settings.control(field, textView: editor, doCommandBy: #selector(NSResponder.cancelOperation(_:))))
        XCTAssertEqual(field.stringValue, "")
        XCTAssertEqual(window.title, SettingsPane.appearance.title)

        field.stringValue = SettingsSearchRow.library.title
        settings.controlTextDidChange(Notification(name: NSControl.textDidChangeNotification, object: field))
        XCTAssertTrue(settings.control(field, textView: editor, doCommandBy: #selector(NSResponder.insertNewline(_:))))
        XCTAssertEqual(window.title, SettingsPane.library.title)
        XCTAssertEqual(window.frame, originalFrame)
    }
}
