import AppKit
import SwiftUI

/// The system's Liquid Glass behind a borderless panel on macOS 26, the
/// standard HUD material before it, and an opaque window with Reduce
/// Transparency, as the system's own panels do.
@MainActor
final class SystemGlass: NSView {
    private let corner: CGFloat
    private let content: NSView
    /// Reading surfaces (Peek) take the popover material on every macOS:
    /// Liquid Glass is for controls, not for text people read (HIG, Materials).
    private let readable: Bool
    private var glass: NSView?
    private var reduced: Bool?

    init(content: NSView, corner: CGFloat, readable: Bool = false) {
        self.content = content
        self.corner = corner
        self.readable = readable
        super.init(frame: .zero)
        wantsLayer = true
        layer?.cornerRadius = corner
        layer?.cornerCurve = .continuous
        layer?.masksToBounds = true
        refresh()
    }

    required init?(coder: NSCoder) { nil }

    func refresh() {
        let reduce = NSWorkspace.shared.accessibilityDisplayShouldReduceTransparency
        layer?.borderWidth = NSWorkspace.shared.accessibilityDisplayShouldIncreaseContrast ? 1 : 0
        layer?.borderColor = NSColor.separatorColor.cgColor
        guard reduced != reduce else { return }
        reduced = reduce
        content.removeFromSuperview()
        glass?.removeFromSuperview()
        glass = nil
        if reduce {
            layer?.backgroundColor = NSColor.windowBackgroundColor.cgColor
            addSubview(content)
        } else {
            layer?.backgroundColor = NSColor.clear.cgColor
            let view: NSView
            if #available(macOS 26, *), !readable {
                let liquid = NSGlassEffectView()
                liquid.style = .regular
                liquid.cornerRadius = corner
                liquid.contentView = content
                view = liquid
            } else {
                let material = NSVisualEffectView()
                material.material = readable ? .popover : .hudWindow
                material.blendingMode = .behindWindow
                material.state = .active
                material.addSubview(content)
                view = material
            }
            addSubview(view)
            glass = view
        }
        needsLayout = true
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer?.borderColor = NSColor.separatorColor.cgColor
            if reduced == true { layer?.backgroundColor = NSColor.windowBackgroundColor.cgColor }
        }
    }

    override func layout() {
        super.layout()
        glass?.frame = bounds
        content.frame = bounds
    }
}

extension View {
    /// The same glass in SwiftUI: Liquid Glass on macOS 26, the regular material before.
    @ViewBuilder
    func systemGlass(corner: CGFloat) -> some View {
        let shape = RoundedRectangle(cornerRadius: corner, style: .continuous)
        if #available(macOS 26, *) {
            glassEffect(.regular, in: shape)
        } else {
            background(.regularMaterial, in: shape)
        }
    }
}
