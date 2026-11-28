import AppKit
import SwiftUI

/// A short notice near the top of the screen, as the system's own volume
/// and brightness HUDs show one. It never takes focus or clicks, and it
/// stays longer for VoiceOver to read it.
@MainActor
final class NoticeHUD {
    static let shared = NoticeHUD()
    private var panel: NSPanel?
    private var hide: DispatchWorkItem?

    func show(_ text: String) {
        let host = NSHostingView(rootView: NoticeView(text: text, symbol: "doc.on.clipboard"))
        host.safeAreaRegions = []
        let size = host.fittingSize
        let panel = self.panel ?? Self.makePanel()
        self.panel = panel
        panel.contentView = SystemGlass(content: host, corner: size.height / 2)
        let screen = NSScreen.screens.first { $0.frame.contains(NSEvent.mouseLocation) } ?? NSScreen.main
        let area = screen?.visibleFrame ?? .zero
        panel.setFrame(NSRect(x: area.midX - size.width / 2, y: area.maxY - size.height - 48,
                              width: size.width, height: size.height), display: true)
        panel.alphaValue = 1
        panel.orderFrontRegardless()
        NSAccessibility.post(element: NSApp as Any, notification: .announcementRequested,
                             userInfo: [.announcement: text, .priority: NSAccessibilityPriorityLevel.high.rawValue])
        hide?.cancel()
        let work = DispatchWorkItem { [weak self] in self?.fade() }
        hide = work
        let seconds = NSWorkspace.shared.isVoiceOverEnabled ? 4.0 : 1.8
        DispatchQueue.main.asyncAfter(deadline: .now() + seconds, execute: work)
    }

    private func fade() {
        guard let panel else { return }
        if NSWorkspace.shared.accessibilityDisplayShouldReduceMotion { panel.orderOut(nil); return }
        NSAnimationContext.runAnimationGroup({ $0.duration = 0.25; panel.animator().alphaValue = 0 },
                                             completionHandler: { panel.orderOut(nil) })
    }

    private static func makePanel() -> NSPanel {
        let panel = NSPanel(contentRect: .zero, styleMask: [.borderless, .nonactivatingPanel],
                            backing: .buffered, defer: true)
        panel.level = .statusBar
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.ignoresMouseEvents = true
        panel.hidesOnDeactivate = false
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .ignoresCycle, .stationary]
        return panel
    }
}

private struct NoticeView: View {
    let text: String
    let symbol: String

    var body: some View {
        Label(text, systemImage: symbol)
            .font(.body.weight(.medium))
            .padding(.horizontal, 18)
            .padding(.vertical, 10)
            .fixedSize()
    }
}
