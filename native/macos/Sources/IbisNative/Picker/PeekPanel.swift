import AppKit
import SwiftUI

/// Peek, as Quick Look shows a file: a panel of its own beside the picker,
/// on the system's glass. It never takes the keyboard, so typing goes on.
@MainActor
final class PeekPanel: NSPanel {
    private let store: PickerStore
    private let host = NSHostingView(rootView: PeekView(row: nil))
    private var row: PickerRow?

    init(store: PickerStore) {
        self.store = store
        super.init(contentRect: NSRect(x: 0, y: 0, width: 320, height: 200),
                   styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: true)
        isFloatingPanel = true
        level = .floating
        isOpaque = false
        backgroundColor = .clear
        hasShadow = true
        hidesOnDeactivate = false
        isExcludedFromWindowsMenu = true
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .ignoresCycle]
        host.safeAreaRegions = []
        contentView = SystemGlass(content: host, corner: 22)
    }

    override var canBecomeKey: Bool { false }

    /// Shows `row` beside the picker's frame, or hides Peek when `row` is nil.
    func show(_ row: PickerRow?, beside picker: NSRect) {
        guard let row else {
            self.row = nil
            orderOut(nil)
            return
        }
        if row != self.row {
            self.row = row
            host.rootView = PeekView(row: row)
        }
        let size = host.fittingSize
        let gap: CGFloat = 10
        let area = (screen ?? NSScreen.main)?.visibleFrame ?? picker
        var x = picker.maxX + gap
        if x + size.width > area.maxX { x = picker.minX - gap - size.width }
        setFrame(NSRect(x: x, y: picker.maxY - size.height, width: size.width, height: size.height), display: true)
        orderFront(nil)
    }
}

private struct PeekView: View {
    let row: PickerRow?

    var body: some View {
        if let row {
            VStack(alignment: .leading, spacing: 8) {
                Label(row.kindLabel, systemImage: row.kind.symbol)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Text(row.title ?? PickerWords.text(.untitled))
                    .font(.title3.weight(.semibold))
                    .fixedSize(horizontal: false, vertical: true)
                Text(row.authors.isEmpty ? row.fullByline : row.authors.joined(separator: ", "))
                    .foregroundStyle(.primary)
                    .fixedSize(horizontal: false, vertical: true)
                HStack(spacing: 6) {
                    if let journal = row.journal { Text(journal).italic() }
                    Text(row.year ?? PickerWords.text(.noDate))
                }
                .foregroundStyle(.secondary)
                if let key = row.citationKey {
                    Text(key)
                        .font(.callout.monospaced())
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                        .padding(.top, 2)
                }
            }
            .frame(width: 300, alignment: .leading)
            .padding(18)
            .accessibilityElement(children: .combine)
        }
    }
}
