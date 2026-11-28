import AppKit
import Carbon

/// Reserve a candidate before changing the working shortcut. While Settings
/// saves, both combinations stay reserved and only the working one opens Ibis.
@MainActor
final class HotKey {
    private struct Registration {
        let reference: EventHotKeyRef
        let id: UInt32
        let code: UInt16
        var shortcut: OpeningShortcut
    }

    private enum Reservation {
        case existing(OpeningShortcut)
        case replacement(Registration)
    }

    private static let signature: OSType = 0x49626973 // Ibis
    private var handler: EventHandlerRef?
    private var active: Registration?
    private var reservation: Reservation?
    private var nextID: UInt32 = 0
    private let action: () -> Void

    var registered: OpeningShortcut? { active?.shortcut }

    init(action: @escaping () -> Void) {
        self.action = action
        var event = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
        InstallEventHandler(
            GetApplicationEventTarget(), { _, event, context in
                var identity = EventHotKeyID()
                guard let event, let context,
                      GetEventParameter(event, EventParamName(kEventParamDirectObject), EventParamType(typeEventHotKeyID),
                                        nil, MemoryLayout<EventHotKeyID>.size, nil, &identity) == noErr,
                      identity.signature == HotKey.signature else { return OSStatus(eventNotHandledErr) }
                let hotKey = Unmanaged<HotKey>.fromOpaque(context).takeUnretainedValue()
                MainActor.assumeIsolated {
                    if hotKey.active?.id == identity.id { hotKey.action() }
                }
                return noErr
            }, 1, &event, Unmanaged.passUnretained(self).toOpaque(), &handler
        )
    }

    func register(_ shortcut: OpeningShortcut) -> Bool {
        guard reserve(shortcut) else { return false }
        commitReservation()
        return true
    }

    func reserve(_ shortcut: OpeningShortcut) -> Bool {
        guard handler != nil, reservation == nil else { return false }
        let code = shortcut.resolvedCode()
        guard !shortcut.conflictsWithPicker(at: code) else { return false }
        let candidate = shortcut.registered(at: code)
        if let active, active.code == code, active.shortcut.modifiers == shortcut.modifiers {
            reservation = .existing(candidate)
            return true
        }

        nextID &+= 1
        let identity = EventHotKeyID(signature: Self.signature, id: nextID)
        var reference: EventHotKeyRef?
        guard RegisterEventHotKey(UInt32(code), shortcut.carbonModifiers, identity,
                                  GetApplicationEventTarget(), UInt32(kEventHotKeyExclusive), &reference) == noErr,
              let reference else { return false }
        reservation = .replacement(Registration(reference: reference, id: identity.id, code: code, shortcut: candidate))
        return true
    }

    func commitReservation() {
        guard let reservation else { return }
        self.reservation = nil
        switch reservation {
        case .existing(let shortcut):
            active?.shortcut = shortcut
        case .replacement(let replacement):
            let previous = active
            active = replacement
            if let previous { UnregisterEventHotKey(previous.reference) }
        }
    }

    func discardReservation() {
        if case .replacement(let candidate) = reservation { UnregisterEventHotKey(candidate.reference) }
        reservation = nil
    }

    deinit {
        if case .replacement(let candidate) = reservation { UnregisterEventHotKey(candidate.reference) }
        if let active { UnregisterEventHotKey(active.reference) }
        if let handler { RemoveEventHandler(handler) }
    }
}
