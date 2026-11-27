import AppKit
import Carbon
import CIbisNative

/// Carbon registers only this one shortcut. It does not listen to other keys.
@MainActor
final class HotKey {
    private static let signature: OSType = 0x49626973 // Ibis
    private var handler: EventHandlerRef?
    private var key: EventHotKeyRef?
    private let action: () -> Void

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
                MainActor.assumeIsolated { hotKey.action() }
                return noErr
            }, 1, &event, Unmanaged.passUnretained(self).toOpaque(), &handler
        )
    }

    /// The repo bundle has its own shortcut so it can run beside Tauri.
    func register() -> Bool {
        if let key { UnregisterEventHotKey(key) }
        key = nil
        let code = ibis_native_command_key(UInt16(kVK_ANSI_Y), UInt8(ascii: "y"))
        guard handler != nil, code >= 0 else { return false }
        let identity = EventHotKeyID(signature: Self.signature, id: 1)
        return RegisterEventHotKey(
            UInt32(code), UInt32(cmdKey | shiftKey | optionKey), identity,
            GetApplicationEventTarget(), UInt32(kEventHotKeyExclusive), &key
        ) == noErr
    }

    deinit {
        if let key { UnregisterEventHotKey(key) }
        if let handler { RemoveEventHandler(handler) }
    }
}
