import Darwin
import Foundation
import XCTest
@testable import IbisNative

/// Every test gets a daemon namespace and invented data inside the build directory.
final class Fixture {
    let binary: URL
    let root: URL
    let library: URL
    let configuration: URL
    private var daemon: DaemonSocket?

    init() throws {
        let package = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
        let repo = package.deletingLastPathComponent().deletingLastPathComponent()
        let target = ProcessInfo.processInfo.environment["CARGO_TARGET_DIR"].map(URL.init(fileURLWithPath:))
            ?? repo.appendingPathComponent("target")
        binary = target.appendingPathComponent("debug/ibis")
        guard FileManager.default.isExecutableFile(atPath: binary.path) else {
            throw XCTSkip("Build the bundled engine with scripts/native-macos.sh build first.")
        }
        root = package.appendingPathComponent(".build/Fixtures/\(UUID().uuidString)")
        library = root.appendingPathComponent("Invented.csljson")
        configuration = root.appendingPathComponent("Config")
        try FileManager.default.createDirectory(at: configuration, withIntermediateDirectories: true)
        try Data(Self.records.utf8).write(to: library)
    }

    deinit {
        stop()
        try? FileManager.default.removeItem(at: root)
    }

    func client(binary: URL? = nil) -> EngineClient {
        EngineClient(binary: binary ?? self.binary, configuration: configuration)
    }

    var opening: PickerOpenRequest {
        PickerOpenRequest(app: "com.apple.TextEdit", file: library.path, language: .enUS, reload: nil)
    }

    var settings: URL {
        configuration.appendingPathComponent("com.ibiscite.ibis.native/settings.json")
    }

    @discardableResult
    func identifyDaemon(binary: URL? = nil) throws -> Int32 {
        let connection = try DaemonSocket(DaemonEndpoint(binary: binary ?? self.binary, configuration: configuration))
        daemon = connection
        return connection.pid
    }

    func stop(signal: Int32 = SIGTERM) {
        guard let daemon else { return }
        if daemon.peerIsOpen { _ = kill(daemon.pid, signal) }
        self.daemon = nil
    }

    private static let records = """
    [
      {
        "id": "invented-lanterns", "citation-key": "vale2024lanterns", "type": "article-journal",
        "title": "Invented lanterns of North Quay", "author": [{"family": "Vale", "given": "Mira"}],
        "issued": {"date-parts": [[2024]]}, "container-title": "Journal of Imaginary Cities", "page": "1–24",
        "date-added": "2024-01-05"
      },
      {
        "id": "invented-bridges", "citation-key": "fen2023bridges", "type": "article-journal",
        "title": "Invented bridges and their shadows", "author": [{"family": "Fen", "given": "Tavi"}],
        "issued": {"date-parts": [[2023]]}, "container-title": "Journal of Imaginary Cities",
        "date-added": "2023-01-05"
      }
    ]
    """
}

@MainActor
func eventually(_ condition: () -> Bool, file: StaticString = #filePath, line: UInt = #line) async throws {
    let deadline = ContinuousClock.now + .seconds(10)
    while !condition() {
        guard ContinuousClock.now < deadline else {
            XCTFail("The picker did not reach the expected state.", file: file, line: line)
            throw CancellationError()
        }
        try await Task.sleep(for: .milliseconds(10))
    }
}
