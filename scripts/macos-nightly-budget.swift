// Installed-nightly diagnostic for a disposable macOS CI desktop.
import AppKit
import Darwin

struct Unavailable: Error {
    let reason: String
    init(_ reason: String) { self.reason = reason }
}

func fixture(in directory: URL) throws -> URL {
    let library = directory.appendingPathComponent("synthetic.bib")
    let topics = ["Mapping invented tidal stations", "Mesures fictives du littoral", "架空の潮位観測", "قياسات مدّ اصطناعية"]
    let abstract = String(repeating: "The simulated stations track tidal changes over a synthetic year. Each series is compared with a control set using the same sampling schedule. ", count: 5)
    let records = (0..<10_000).map { index in
        let authors = (0..<12).map { "Example\((index + $0) % 5000), Avery" }.joined(separator: " and ")
        return """
        @article{synthetic\(index), title={\(topics[index % topics.count]) \(index)},
        author={\(authors)}, year={\(1950 + index % 75)}, journal={Invented Letters},
        pages={1--20}, abstract={\(abstract)}}
        """
    }
    try records.joined(separator: "\n").write(to: library, atomically: true, encoding: .utf8)
    return library
}

func resourceCoalition(_ pid: pid_t) -> UInt64? {
    // XNU's PROC_PIDCOALITIONINFO returns two coalition IDs and three reserved
    // words. Index 0 is the resource coalition used by macOS energy reporting.
    var info = [UInt64](repeating: 0, count: 5)
    return info.withUnsafeMutableBytes { buffer in
        proc_pidinfo(pid, 20, 0, buffer.baseAddress, Int32(buffer.count)) == buffer.count
    } ? info[0] : nil
}

func appProcesses(_ app: pid_t) throws -> Set<pid_t> {
    guard let coalition = resourceCoalition(app), coalition > 1 else {
        throw Unavailable("the app's resource coalition could not be read")
    }
    let bytes = proc_listpids(UInt32(PROC_UID_ONLY), getuid(), nil, 0)
    guard bytes > 0 else { throw Unavailable("the process list could not be read") }
    var pids = [pid_t](repeating: 0, count: Int(bytes) / MemoryLayout<pid_t>.stride + 64)
    let written = pids.withUnsafeMutableBytes { buffer in
        proc_listpids(UInt32(PROC_UID_ONLY), getuid(), buffer.baseAddress, Int32(buffer.count))
    }
    guard written > 0, written < pids.count * MemoryLayout<pid_t>.stride else {
        throw Unavailable("the process list could not be read in full")
    }
    var members: Set<pid_t> = [app]
    var webContent = false
    for pid in pids where pid > 0 && pid != app {
        var name = [CChar](repeating: 0, count: 256)
        guard proc_name(pid, &name, UInt32(name.count)) > 0 else { continue }
        let processName = String(cString: name)
        guard processName.hasPrefix("com.apple.WebKit.") else { continue }
        guard let owner = resourceCoalition(pid) else {
            throw Unavailable("a WebKit helper could not be attributed to its app")
        }
        guard owner == coalition else { continue }
        members.insert(pid)
        webContent = webContent || processName.hasPrefix("com.apple.WebKit.WebContent")
    }
    guard webContent else { throw Unavailable("no WebKit page process could be attributed to the app") }
    return members
}

func usage(_ pid: pid_t) throws -> rusage_info_v2 {
    var info = rusage_info_v2()
    let status = withUnsafeMutablePointer(to: &info) { pointer in
        pointer.withMemoryRebound(to: rusage_info_t?.self, capacity: 1) {
            proc_pid_rusage(pid, RUSAGE_INFO_V2, $0)
        }
    }
    guard status == 0 else { throw Unavailable("a process's kernel resource counters could not be read") }
    return info
}

func tool(_ path: String, _ arguments: [String]) -> String? {
    guard FileManager.default.isExecutableFile(atPath: path) else { return nil }
    let child = Process(), output = Pipe()
    child.executableURL = URL(fileURLWithPath: path)
    child.arguments = arguments
    child.standardOutput = output
    child.standardError = FileHandle.nullDevice
    child.environment = ["LC_ALL": "C"]
    do { try child.run() } catch { return nil }
    let timeout = DispatchWorkItem { if child.isRunning { kill(child.processIdentifier, SIGKILL) } }
    DispatchQueue.global().asyncAfter(deadline: .now() + 5, execute: timeout)
    let data = output.fileHandleForReading.readDataToEndOfFile()
    child.waitUntilExit()
    timeout.cancel()
    return child.terminationStatus == 0 ? String(data: data, encoding: .utf8) : nil
}

func footprint(_ pid: pid_t, _ kernelBytes: UInt64) -> (Double, String) {
    if let line = tool("/usr/bin/footprint", ["-f", "bytes", String(pid)])?
        .split(separator: "\n").first(where: { $0.trimmingCharacters(in: .whitespaces).hasPrefix("phys_footprint:") }),
       let bytes = line.split(whereSeparator: \.isWhitespace).dropFirst().first.flatMap({ Double($0) }) {
        return (bytes, "footprint")
    }
    if let line = tool("/usr/bin/vmmap", ["--summary", String(pid)])?
        .split(separator: "\n").first(where: { $0.hasPrefix("Physical footprint:") }),
       let size = line.split(whereSeparator: \.isWhitespace).last,
       let unit = size.last, let power = "BKMGT".firstIndex(of: unit),
       let value = Double(size.dropLast()) {
        return (value * pow(1024, Double("BKMGT".distance(from: "BKMGT".startIndex, to: power))), "vmmap")
    }
    // Hardened helpers can deny tool inspection. These are the same per-PID
    // physical-footprint bytes from macOS's kernel ledger.
    return (Double(kernelBytes), "proc_pid_rusage")
}

@MainActor func measure(bundle: URL, workspace: URL) async throws -> String {
    guard let identifier = Bundle(url: bundle)?.bundleIdentifier,
          NSRunningApplication.runningApplications(withBundleIdentifier: identifier).isEmpty else {
        throw Unavailable("the nightly bundle is missing or already running")
    }
    let library = try fixture(in: workspace)
    let profile = workspace.appendingPathComponent("Library/Application Support/\(identifier)")
    try FileManager.default.createDirectory(at: profile, withIntermediateDirectories: true)
    try #"{"schema_version":2,"update_checks":false}"#.write(
        to: profile.appendingPathComponent("settings.json"), atomically: true, encoding: .utf8)

    let configuration = NSWorkspace.OpenConfiguration()
    configuration.environment = ["HOME": workspace.path, "IBIS_LIBRARY": library.path]
    configuration.addsToRecentItems = false
    configuration.allowsRunningApplicationSubstitution = false
    configuration.promptsUserIfNeeded = false
    let app = try await NSWorkspace.shared.openApplication(at: bundle, configuration: configuration)
    defer { if !app.isTerminated { _ = app.forceTerminate() } }

    // Allow the picker to start its WebKit page and the engine to load the
    // generated library before starting the idle interval.
    try await Task.sleep(for: .seconds(15))
    let pid = app.processIdentifier
    guard app.isFinishedLaunching, !app.isTerminated, app.isHidden || app.hide() else {
        throw Unavailable("the app could not be hidden after startup")
    }
    try await Task.sleep(for: .seconds(1))
    guard app.isHidden else { throw Unavailable("macOS did not confirm the app was hidden") }
    let start = try Dictionary(uniqueKeysWithValues: appProcesses(pid).map { ($0, try usage($0)) })
    let clock = ContinuousClock()
    let began = clock.now
    try await Task.sleep(for: .seconds(240))
    guard !app.isTerminated, app.isHidden else { throw Unavailable("the app did not stay hidden and running") }
    let end = try Dictionary(uniqueKeysWithValues: appProcesses(pid).map { ($0, try usage($0)) })
    let elapsed = (clock.now - began).components
    let seconds = Double(elapsed.seconds) + Double(elapsed.attoseconds) / 1e18
    let samples = end.sorted { $0.key < $1.key }.map { footprint($0.key, $0.value.ri_phys_footprint) }
    let mib = samples.reduce(0) { $0 + $1.0 } / 1_048_576
    let tools = Set(samples.map { $0.1 }).sorted().joined(separator: ", ")
    let memory = String(format: "After 4 minutes hidden with the 10,000-reference fixture, %.1f MiB physical footprint across %d processes (%@, 200 MiB budget, %@)",
                        mib, end.count, mib <= 200 ? "within" : "over", tools)
    guard start.keys == end.keys, end.allSatisfy({ pid, now in
        guard let before = start[pid] else { return false }
        return before.ri_proc_start_abstime == now.ri_proc_start_abstime && before.ri_interrupt_wkups <= now.ri_interrupt_wkups
    }) else { return "\(memory). Idle wake-ups unavailable because the process set changed." }
    let wakeups = end.reduce(UInt64(0)) { $0 + ($1.value.ri_interrupt_wkups - start[$1.key]!.ri_interrupt_wkups) }
    return memory + String(format: ". %.2f idle wake-ups/s (%@, 1/s budget).", Double(wakeups) / seconds,
                           Double(wakeups) <= seconds ? "within" : "over")
}

do {
    // A stalled Launch Services request cannot consume the installed job's
    // qualification deadline. This helper runs after the gating checks.
    DispatchQueue.global().asyncAfter(deadline: .now() + 360) { exit(1) }
    let arguments = CommandLine.arguments
    guard arguments.count == 3 else { throw Unavailable("expected an app bundle and a disposable workspace") }
    let report = try await measure(bundle: URL(fileURLWithPath: arguments[1]), workspace: URL(fileURLWithPath: arguments[2]))
    print(report)
} catch let error as Unavailable {
    print("Unavailable (\(error.reason)).")
} catch {
    print("Unavailable (could not set up or launch the measurement).")
}
