import Darwin
import Foundation

enum DaemonError: LocalizedError, Equatable {
    case disconnected, invalidFrame, wrongPeer, newerEngine, noEngine, busy, changed

    var errorDescription: String? {
        let word: PickerWord = switch self {
        case .disconnected: .disconnected
        case .invalidFrame: .invalidFrame
        case .wrongPeer: .wrongPeer
        case .newerEngine: .newerEngine
        case .noEngine: .noEngine
        case .busy: .busy
        case .changed: .changed
        }
        return PickerWords.text(word)
    }
}

/// The bundled binary owns endpoint discovery, startup locking and version
/// negotiation. The app uses the returned identity in its own socket handshake.
struct DaemonEndpoint {
    let socket: String
    let deliveryLock: String
    let hello: Data

    init(binary: URL, configuration: URL?) throws {
        let output = Pipe()
        let process = Process()
        process.executableURL = binary
        process.arguments = ["__picker_endpoint"]
        process.standardOutput = output
        process.standardError = FileHandle.nullDevice
        if let configuration {
            var environment = ProcessInfo.processInfo.environment
            environment["IBIS_CONFIG_DIR"] = configuration.path
            process.environment = environment
        }
        do {
            try process.run()
        } catch {
            throw DaemonError.noEngine
        }
        let data = output.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        if process.terminationStatus == 6 { throw DaemonError.newerEngine }
        if process.terminationStatus == 7 { throw DaemonError.wrongPeer }
        guard process.terminationStatus == 0 else { throw DaemonError.noEngine }
        guard let info = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let socket = info["socket"] as? String,
              let lock = info["delivery_lock"] as? String,
              let hello = info["hello"], JSONSerialization.isValidJSONObject(hello) else {
            throw DaemonError.invalidFrame
        }
        self.socket = socket
        self.deliveryLock = lock
        self.hello = try JSONSerialization.data(withJSONObject: hello)
    }
}

/// One ordered stream of picker turns: the daemon's socket, or the pipes of
/// an engine the app started for itself. Its owner drops it on any malformed
/// or incomplete reply.
protocol EngineLink: AnyObject {
    func send(_ body: Data) throws
    func receive() throws -> Data
    var peerIsOpen: Bool { get }
    /// Hold what keeps a replacement engine from starting during a native effect.
    func withDeliveryFence<T>(_ body: () throws -> T) throws -> T
}

/// The engine's Content-Length frames, bounded, over a descriptor to read and
/// one to write (the same one for a socket).
final class Frames {
    private static let separator = Data("\r\n\r\n".utf8)
    private static let header = "Content-Length: "
    private let input: Int32
    private let output: Int32
    private var buffer = Data()
    /// How long one read waits, in milliseconds; `nil` leaves it to the
    /// descriptor's own timeout, as a socket's.
    var readLimit: Int32?

    init(input: Int32, output: Int32) {
        self.input = input
        self.output = output
    }

    func send(_ body: Data) throws {
        guard (1...1024 * 1024).contains(body.count) else { throw DaemonError.invalidFrame }
        var frame = Data("\(Self.header)\(body.count)\r\n\r\n".utf8)
        frame.append(body)
        try frame.withUnsafeBytes { bytes in
            guard let base = bytes.baseAddress else { throw DaemonError.invalidFrame }
            var sent = 0
            while sent < bytes.count {
                let count = Darwin.write(output, base.advanced(by: sent), bytes.count - sent)
                if count < 0, errno == EINTR { continue }
                guard count > 0 else { throw DaemonError.disconnected }
                sent += count
            }
        }
    }

    func receive() throws -> Data {
        func readMore() throws {
            if let readLimit {
                var event = pollfd(fd: input, events: Int16(POLLIN), revents: 0)
                var ready: Int32
                repeat { ready = poll(&event, 1, readLimit) } while ready < 0 && errno == EINTR
                guard ready > 0 else { throw DaemonError.disconnected }
            }
            var chunk = [UInt8](repeating: 0, count: 8192)
            let capacity = chunk.count
            var count: Int
            repeat { count = Darwin.read(input, &chunk, capacity) } while count < 0 && errno == EINTR
            guard count > 0 else { throw DaemonError.disconnected }
            buffer.append(contentsOf: chunk.prefix(count))
        }

        while buffer.range(of: Self.separator) == nil {
            guard buffer.count < 8192 else { throw DaemonError.invalidFrame }
            try readMore()
        }
        guard let separator = buffer.range(of: Self.separator),
              separator.upperBound - buffer.startIndex <= 8192,
              let line = String(data: buffer[..<separator.lowerBound], encoding: .ascii),
              line.hasPrefix(Self.header),
              let length = Int(line.dropFirst(Self.header.count)),
              (1...16 * 1024 * 1024).contains(length) else {
            throw DaemonError.invalidFrame
        }
        buffer.removeSubrange(..<separator.upperBound)
        while buffer.count < length { try readMore() }
        let body = Data(buffer.prefix(length))
        buffer.removeFirst(length)
        return body
    }
}

/// One authenticated fd to the daemon.
final class DaemonSocket: EngineLink {
    private let fd: Int32
    private let lockPath: String
    private let frames: Frames
    private(set) var pid: Int32 = 0

    private init(fd: Int32, lockPath: String) {
        self.fd = fd
        self.lockPath = lockPath
        frames = Frames(input: fd, output: fd)
    }

    convenience init(_ endpoint: DaemonEndpoint) throws {
        self.init(fd: try Self.connect(endpoint.socket), lockPath: endpoint.deliveryLock)
        try configureTimeouts(read: 2, write: 2)
        try send(endpoint.hello)
        struct Ready: Decodable { let pid: Int32 }
        struct Welcome: Decodable { let ready: Ready? }
        guard let ready = try JSONDecoder().decode(Welcome.self, from: receive()).ready else {
            throw DaemonError.disconnected
        }
        pid = ready.pid
        try configureTimeouts(read: 30, write: 5)
    }

    deinit { close(fd) }

    func send(_ body: Data) throws { try frames.send(body) }

    func receive() throws -> Data { try frames.receive() }

    private static func connect(_ path: String) throws -> Int32 {
        var info = stat()
        guard lstat(path, &info) == 0 else { throw DaemonError.disconnected }
        guard info.st_mode & S_IFMT == S_IFSOCK, info.st_uid == geteuid() else { throw DaemonError.wrongPeer }
        let bytes = Array(path.utf8CString)
        var address = sockaddr_un()
        guard !path.utf8.contains(0), bytes.count <= MemoryLayout.size(ofValue: address.sun_path) else {
            throw DaemonError.invalidFrame
        }
        let length = UInt8(MemoryLayout<sockaddr_un>.offset(of: \.sun_path)! + bytes.count)
        address.sun_family = sa_family_t(AF_UNIX)
        address.sun_len = length
        withUnsafeMutableBytes(of: &address.sun_path) { $0.copyBytes(from: bytes.map { UInt8(bitPattern: $0) }) }

        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { throw DaemonError.disconnected }
        do {
            guard fcntl(fd, F_SETNOSIGPIPE, 1) == 0,
                  fcntl(fd, F_SETFD, FD_CLOEXEC) == 0,
                  fcntl(fd, F_SETFL, O_NONBLOCK) == 0 else {
                throw DaemonError.disconnected
            }
            let opened = withUnsafePointer(to: &address) {
                $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                    Darwin.connect(fd, $0, socklen_t(length))
                }
            }
            guard opened == 0, fcntl(fd, F_SETFL, 0) == 0 else { throw DaemonError.disconnected }
            var uid: uid_t = 0, gid: gid_t = 0
            guard getpeereid(fd, &uid, &gid) == 0, uid == geteuid() else { throw DaemonError.wrongPeer }
            return fd
        } catch {
            close(fd)
            throw error
        }
    }

    private func configureTimeouts(read: Int, write: Int) throws {
        var receive = timeval(tv_sec: read, tv_usec: 0)
        var send = timeval(tv_sec: write, tv_usec: 0)
        let size = socklen_t(MemoryLayout<timeval>.size)
        guard setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &receive, size) == 0,
              setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &send, size) == 0 else {
            throw DaemonError.disconnected
        }
    }

    var peerIsOpen: Bool {
        var event = pollfd(fd: fd, events: Int16(POLLIN), revents: 0)
        guard poll(&event, 1, 0) >= 0 else { return false }
        return event.revents & Int16(POLLERR | POLLHUP | POLLNVAL) == 0
    }

    /// A successor daemon waits for this fence before opening its listener.
    /// Keep it even if the serving daemon exits during a native effect.
    func withDeliveryFence<T>(_ body: () throws -> T) throws -> T {
        let file = open(lockPath, O_RDONLY | O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK)
        guard file >= 0 else { throw DaemonError.disconnected }
        defer { close(file) }
        var info = stat()
        guard fstat(file, &info) == 0, info.st_uid == geteuid(),
              info.st_mode & S_IFMT == S_IFREG, info.st_mode & 0o077 == 0 else {
            throw DaemonError.wrongPeer
        }
        guard flock(file, LOCK_EX | LOCK_NB) == 0 else { throw DaemonError.busy }
        return try body()
    }
}

/// An engine of the app's own on its standard streams (`ibis __picker_stdio`),
/// for when no daemon can be started or reached. It serves only this app and
/// works on the same files, so nothing can take over from it during a
/// delivery and it needs no fence; each Return still pastes at most once.
final class PrivateEngine: EngineLink {
    private let process: Process
    private let toEngine: FileHandle
    private let fromEngine: FileHandle
    private let frames: Frames

    init(binary: URL, configuration: URL?) throws {
        let input = Pipe()
        let output = Pipe()
        let process = Process()
        process.executableURL = binary
        process.arguments = ["__picker_stdio"]
        process.standardInput = input
        process.standardOutput = output
        process.standardError = FileHandle.nullDevice
        if let configuration {
            var environment = ProcessInfo.processInfo.environment
            environment["IBIS_CONFIG_DIR"] = configuration.path
            process.environment = environment
        }
        let toEngine = input.fileHandleForWriting
        let fromEngine = output.fileHandleForReading
        for fd in [toEngine.fileDescriptor, fromEngine.fileDescriptor] {
            guard fcntl(fd, F_SETFD, FD_CLOEXEC) == 0 else { throw DaemonError.noEngine }
        }
        // A write after the engine exits fails instead of signalling the app.
        guard fcntl(toEngine.fileDescriptor, F_SETNOSIGPIPE, 1) == 0 else { throw DaemonError.noEngine }
        do {
            try process.run()
        } catch {
            throw DaemonError.noEngine
        }
        let frames = Frames(input: fromEngine.fileDescriptor, output: toEngine.fileDescriptor)
        frames.readLimit = 30_000
        self.process = process
        self.toEngine = toEngine
        self.fromEngine = fromEngine
        self.frames = frames
    }

    /// Closing its input ends the engine; one still running a moment later is stopped.
    deinit {
        try? toEngine.close()
        for _ in 0..<20 where process.isRunning { usleep(10_000) }
        if process.isRunning { process.terminate() }
        try? fromEngine.close()
    }

    func send(_ body: Data) throws { try frames.send(body) }

    func receive() throws -> Data { try frames.receive() }

    var peerIsOpen: Bool { process.isRunning }

    func withDeliveryFence<T>(_ body: () throws -> T) throws -> T { try body() }
}
