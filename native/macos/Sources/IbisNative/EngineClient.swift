import CIbisNative
import Foundation

extension SettingsRequest {
    static let readOnly = patch()

    static func patch(appearance: Appearance? = nil, density: Density? = nil, largerText: Bool? = nil,
                      libraryFile: String? = nil, paste: Bool? = nil, shortcut: ShortcutSetting? = nil) -> Self {
        SettingsRequest(appearance: appearance, density: density, largerText: largerText, libraryFile: libraryFile,
                        paste: paste, shortcut: shortcut)
    }

    var isReadOnly: Bool {
        appearance == nil && density == nil && largerText == nil && paste == nil && shortcut == nil && libraryFile == nil
    }
}

/// Blocking file descriptors and daemon launches stay on one utility queue.
/// Read intents may reconnect and replay. Clipboard intents never replay.
final class EngineClient: @unchecked Sendable {
    private let binary: URL
    private let configuration: URL?
    private let queue = DispatchQueue(label: "Ibis engine", qos: .userInitiated)
    private let encoder = JSONEncoder()
    private let decoder = JSONDecoder()
    private var socket: DaemonSocket?
    private var lastOpening: PickerOpenRequest?
    private var nextID: UInt64 = 0
    private var session: UInt64 = 0

    init(binary: URL, configuration: URL? = nil) {
        self.binary = binary
        self.configuration = configuration
    }

    func open(_ request: PickerOpenRequest) async throws -> PickerOpenResult {
        try await onQueue {
            self.lastOpening = request
            return try self.read(PickerOpen.self, request, restoring: false)
        }
    }

    func search(_ request: PickerSearchRequest) async throws -> (result: PickerSearchResult, session: UInt64) {
        try await onQueue {
            let result = try self.read(PickerSearch.self, request)
            return (result, self.session)
        }
    }

    func preview(_ request: PickerPreviewRequest) async throws -> (result: PickerPreviewResult, session: UInt64) {
        try await onQueue {
            let result = try self.read(PickerPreview.self, request)
            return (result, self.session)
        }
    }

    func forms(_ request: PickerFormsRequest) async throws -> FormDetails {
        try await onQueue { try self.read(PickerForms.self, request) }
    }

    func settings(_ request: SettingsRequest) async throws -> SettingsResult {
        try await onQueue {
            if request.isReadOnly {
                return try self.read(Settings.self, request)
            }
            // A failed reply can follow a committed change. Let the window read
            // back the saved value without replaying the write.
            do {
                return try self.exchange(Settings.self, request, on: self.connect(restoring: true))
            } catch let error as Rejected {
                throw error
            } catch {
                self.socket = nil
                throw error
            }
        }
    }

    func chooseLibrary(_ file: URL) async throws -> SettingsResult {
        do {
            return try await settings(.patch(libraryFile: file.path))
        } catch let error as Rejected {
            throw error
        } catch {
            // A disconnected reply may follow the commit. Never replay the write.
            if let saved = try? await settings(.readOnly), saved.library?.file == file.path { return saved }
            throw error
        }
    }

    /// A nonzero write receipt is reported to the daemon even when paste is
    /// refused. The shell holds the daemon's grant through copy and paste.
    func deliver(
        _ request: PickerDeliverRequest,
        write: @escaping @MainActor @Sendable (PickerDeliverResult) -> UInt32
    ) async throws -> IbisDelivery? {
        try await onQueue {
            guard let socket = self.socket else { throw DaemonError.changed }
            do {
                return try socket.withDeliveryFence {
                    let grant = try self.exchange(PickerDeliver.self, request, on: socket)
                    let receipt = DispatchQueue.main.sync { socket.peerIsOpen ? write(grant) : 0 }
                    let result = receipt == 0 ? nil : (grant.paste ? ibis_native_paste(receipt) : IbisCopied)
                    do {
                        try socket.send(Data((receipt != 0 ? "true" : "false").utf8))
                    } catch {
                        // The effect already happened. Reconnect on the next read.
                        self.socket = nil
                    }
                    return result
                }
            } catch is Rejected {
                throw DaemonError.changed
            } catch {
                self.socket = nil
                throw error
            }
        }
    }

    private func onQueue<T>(_ body: @escaping @Sendable () throws -> T) async throws -> T {
        try Task.checkCancellation()
        return try await withCheckedThrowingContinuation { continuation in
            queue.async { continuation.resume(with: Result(catching: body)) }
        }
    }

    private func read<M: PickerRPC>(_ method: M.Type, _ params: M.Parameters, restoring: Bool = true) throws -> M.Reply {
        do {
            return try exchange(method, params, on: connect(restoring: restoring))
        } catch let error as Rejected {
            throw error
        } catch {
            socket = nil
            // An incompatible engine or unauthenticated peer must be fixed before retry.
            if let reason = error as? DaemonError, reason == .wrongPeer || reason == .newerEngine {
                throw reason
            }
            do {
                return try exchange(method, params, on: connect(restoring: restoring))
            } catch let error as Rejected {
                throw error
            } catch {
                socket = nil
                throw error
            }
        }
    }

    private func connect(restoring: Bool) throws -> DaemonSocket {
        if let socket { return socket }
        let connected = try DaemonSocket(DaemonEndpoint(binary: binary, configuration: configuration))
        if restoring, let opening = lastOpening {
            _ = try exchange(PickerOpen.self, opening, on: connected)
        }
        socket = connected
        session &+= 1
        return connected
    }

    private struct Request<Params: Encodable>: Encodable {
        let jsonrpc = "2.0"
        let id: UInt64
        let method: String
        let params: Params
    }

    private struct Reply<Value: Decodable>: Decodable {
        let jsonrpc: String
        let id: UInt64
        let result: Value?
        let error: Rejected?
    }

    private struct Rejected: Decodable, LocalizedError {
        let code: Int
        let message: String
        var errorDescription: String? { message }
    }

    private func exchange<M: PickerRPC>(_ method: M.Type, _ params: M.Parameters, on socket: DaemonSocket) throws -> M.Reply {
        nextID += 1
        let request = Request(id: nextID, method: method.name, params: params)
        try socket.send(encoder.encode(request))
        let reply = try decoder.decode(Reply<M.Reply>.self, from: socket.receive())
        guard reply.jsonrpc == "2.0", reply.id == nextID else { throw DaemonError.invalidFrame }
        if let error = reply.error { throw error }
        guard let result = reply.result else { throw DaemonError.invalidFrame }
        return result
    }
}
