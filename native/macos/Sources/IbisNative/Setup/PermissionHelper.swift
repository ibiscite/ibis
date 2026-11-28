import AppKit
import CIbisNative
import SwiftUI

/// macOS lets no app switch on its own Accessibility access, so Ibis opens the
/// list and waits beside it with its icon to drag in. The card follows the
/// System Settings window, watches the grant and closes once it is on.
@MainActor
final class PermissionHelper {
    static let shared = PermissionHelper()
    private var askedAtPaste = false
    private var panel: NSPanel?
    private var watch: Task<Void, Never>?

    func show(granted: @escaping @MainActor () -> Void = {}) {
        guard !ibis_native_paste_granted() else { return granted() }
        ibis_native_request_paste_access()
        if let pane = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility") {
            NSWorkspace.shared.open(pane)
        }
        let panel = self.panel ?? makePanel()
        self.panel = panel
        place(panel)
        panel.orderFrontRegardless()
        watch?.cancel()
        watch = Task { [weak self] in
            for _ in 0 ..< 600 {
                try? await Task.sleep(for: .milliseconds(500))
                guard let self, !Task.isCancelled else { return }
                if ibis_native_paste_granted() {
                    self.close()
                    NSHapticFeedbackManager.defaultPerformer.perform(.levelChange, performanceTime: .now)
                    granted()
                    return
                }
                self.place(panel)
            }
            self?.close()
        }
    }

    /// The first paste that needs access asks; later ones only copy and say so.
    func showAtPaste() {
        guard !askedAtPaste else { return }
        askedAtPaste = true
        show()
    }

    func close() {
        watch?.cancel()
        watch = nil
        panel?.orderOut(nil)
    }

    private func makePanel() -> NSPanel {
        let host = NSHostingView(rootView: PermissionCard(close: { [weak self] in self?.close() }))
        host.safeAreaRegions = []
        let size = host.fittingSize
        let panel = NSPanel(contentRect: NSRect(origin: .zero, size: size),
                            styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        panel.isFloatingPanel = true
        panel.level = .floating
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.hidesOnDeactivate = false
        panel.isMovableByWindowBackground = true
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .ignoresCycle]
        panel.contentView = SystemGlass(content: host, corner: 22)
        panel.setAccessibilityLabel(PickerWords.text(.accessTitle, PermissionCard.appName))
        return panel
    }

    /// Beside the System Settings window when it is on screen, else the top
    /// right of the screen, where System Settings will open.
    private func place(_ panel: NSPanel) {
        let size = panel.frame.size
        let gap: CGFloat = 12
        guard let screen = NSScreen.main else { return }
        let area = screen.visibleFrame
        var origin = NSPoint(x: area.maxX - size.width - gap * 2, y: area.maxY - size.height - gap * 2)
        if let settings = Self.systemSettingsFrame() {
            let right = settings.maxX + gap
            let x = right + size.width <= area.maxX ? right : settings.minX - gap - size.width
            origin = NSPoint(x: max(area.minX, x), y: settings.maxY - size.height - 52)
        }
        panel.setFrameOrigin(origin)
    }

    /// Window bounds need no screen recording access; only window titles do.
    private static func systemSettingsFrame() -> NSRect? {
        let pids = Set(NSRunningApplication.runningApplications(withBundleIdentifier: "com.apple.systempreferences")
            .map(\.processIdentifier))
        guard !pids.isEmpty,
              let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
                as? [[String: Any]],
              let primary = NSScreen.screens.first else { return nil }
        for window in list {
            guard let pid = window[kCGWindowOwnerPID as String] as? Int32, pids.contains(pid),
                  (window[kCGWindowLayer as String] as? Int) == 0,
                  let bounds = window[kCGWindowBounds as String] as? NSDictionary,
                  let rect = CGRect(dictionaryRepresentation: bounds), rect.width > 200 else { continue }
            return NSRect(x: rect.minX, y: primary.frame.maxY - rect.maxY, width: rect.width, height: rect.height)
        }
        return nil
    }
}

private struct PermissionCard: View {
    static let appName = Bundle.main.object(forInfoDictionaryKey: "CFBundleDisplayName") as? String
        ?? Bundle.main.object(forInfoDictionaryKey: "CFBundleName") as? String
        ?? PickerWords.text(.app)
    let close: () -> Void

    var body: some View {
        VStack(spacing: 10) {
            Image(nsImage: NSWorkspace.shared.icon(forFile: Bundle.main.bundlePath))
                .resizable()
                .frame(width: 64, height: 64)
                .onDrag { NSItemProvider(object: Bundle.main.bundleURL as NSURL) }
                .help(PickerWords.text(.accessDrag))
            Text(PickerWords.text(.accessTitle, Self.appName))
                .font(.headline)
            Text(PickerWords.text(.accessDrag))
                .font(.callout)
                .foregroundStyle(.secondary)
            Text(PickerWords.text(.accessWait))
                .font(.caption)
                .foregroundStyle(.secondary)
            ProgressView().controlSize(.small)
        }
        .multilineTextAlignment(.center)
        .fixedSize(horizontal: false, vertical: true)
        .frame(width: 220)
        .padding(.horizontal, 16)
        .padding(.vertical, 18)
        .overlay(alignment: .topTrailing) {
            Button(action: close) { Image(systemName: "xmark") }
                .buttonStyle(.borderless)
                .controlSize(.small)
                .padding(10)
                .keyboardShortcut(.cancelAction)
                .accessibilityLabel(PickerWords.text(.closeWindow))
        }
    }
}
