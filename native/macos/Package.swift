// swift-tools-version: 6.2
import Foundation
import PackageDescription

let rustTarget = ProcessInfo.processInfo.environment["CARGO_TARGET_DIR"]
    ?? URL(fileURLWithPath: #filePath).deletingLastPathComponent()
        .deletingLastPathComponent().deletingLastPathComponent().appendingPathComponent("target").path
let rustLibrary = URL(fileURLWithPath: rustTarget).appendingPathComponent("debug/libibis_native_ffi.a").path

let package = Package(
    name: "IbisNative",
    platforms: [.macOS(.v15)],
    targets: [
        .systemLibrary(name: "CIbisNative"),
        .executableTarget(
            name: "IbisNative",
            dependencies: ["CIbisNative"],
            linkerSettings: [
                .unsafeFlags([rustLibrary]),
                .linkedFramework("AppKit"),
                .linkedFramework("Carbon"),
                .linkedFramework("CoreGraphics"),
                .linkedFramework("CoreServices"),
                .linkedFramework("Security"),
            ]
        ),
        .testTarget(name: "IbisNativeTests", dependencies: ["IbisNative"]),
    ],
    swiftLanguageModes: [.v5]
)
