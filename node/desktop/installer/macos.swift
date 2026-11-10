import AppKit
import Darwin
import Security

private let identifier = "macOS.citizenappchain"
private let files = FileManager.default

private struct InstallError: LocalizedError {
    let errorDescription: String?
    init(_ description: String) { errorDescription = description }
}

private func run(_ executable: String, _ arguments: [String], output: Bool = false) throws -> Data {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: executable)
    process.arguments = arguments
    let pipe = Pipe()
    process.standardOutput = output ? pipe : FileHandle.nullDevice
    process.standardError = FileHandle.nullDevice
    try process.run()
    let data = output ? pipe.fileHandleForReading.readDataToEndOfFile() : Data()
    process.waitUntilExit()
    guard process.terminationStatus == 0 else { throw InstallError("系统操作失败或已取消。") }
    return data
}

private func signingTeam() throws -> String {
    var code: SecStaticCode?
    var info: CFDictionary?
    guard SecStaticCodeCreateWithPath(Bundle.main.bundleURL as CFURL, [], &code) == errSecSuccess, let code,
          SecCodeCopySigningInformation(code, SecCSFlags(rawValue: kSecCSSigningInformation), &info) == errSecSuccess,
          let team = (info as? [String: Any])?[kSecCodeInfoTeamIdentifier as String] as? String,
          team.count == 10, team.utf8.allSatisfy({ (48...57).contains($0) || (65...90).contains($0) })
    else { throw InstallError("无法确认安装器的签名身份。") }
    return team
}

private func replaceApp(from source: URL, to destination: URL, requirement: String) throws {
    let script = """
    set -eu
    source=$1; destination=$2; requirement=$3
    [ ! -L "$destination" ]
    if [ -e "$destination" ]; then
      /usr/bin/codesign --verify -R "$requirement" "$destination"
    fi
    /usr/bin/codesign --verify --deep --strict -R "$requirement" "$source"
    parent=$(/usr/bin/dirname "$destination")
    work=$(/usr/bin/mktemp -d "$parent/.citizenchain-install.XXXXXXXX")
    trap '/bin/rm -rf "$work"' EXIT
    /usr/bin/ditto "$source" "$work/citizenchain.app"
    /usr/bin/codesign --verify --deep --strict -R "$requirement" "$work/citizenchain.app"
    if [ -e "$destination" ]; then /bin/mv "$destination" "$work/previous.app"; fi
    if ! /bin/mv "$work/citizenchain.app" "$destination"; then
      if [ -e "$work/previous.app" ]; then
        /bin/mv "$work/previous.app" "$destination" || trap - EXIT
      fi
      exit 1
    fi
    """
    let args = [source.path, destination.path, requirement]
    if files.isWritableFile(atPath: destination.deletingLastPathComponent().path) {
        _ = try run("/bin/sh", ["-c", script, "citizenchain-install"] + args)
    } else {
        let authorization = """
        on run argv
          set commandText to "/bin/sh -c " & quoted form of item 1 of argv & " citizenchain-install"
          repeat with index from 2 to count argv
            set commandText to commandText & " " & quoted form of item index of argv
          end repeat
          do shell script commandText with administrator privileges
        end run
        """
        _ = try run("/usr/bin/osascript", ["-e", authorization, "--", script] + args)
    }
}

private struct InstallCancelled: Error {}

final class Installer: NSObject, NSApplicationDelegate, NSWindowDelegate {
    private var window: NSWindow!
    private let status = NSTextField(labelWithString: "正在准备安装")
    private let version = NSTextField(labelWithString: "")
    private lazy var cancelButton = NSButton(title: "×", target: self, action: #selector(cancelInstall))
    private let percentage = NSTextField(labelWithString: "0%")
    private let progress = NSProgressIndicator()
    private var arguments: [String: String] = [:]
    private let phaseLock = NSLock()
    private enum Phase { case installing, cancelled, finished }
    private var phase = Phase.installing

    private var embeddedApp: URL {
        Bundle.main.bundleURL.resolvingSymlinksInPath()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        let args = Array(CommandLine.arguments.dropFirst())
        if args.count.isMultiple(of: 2) {
            for i in stride(from: 0, to: args.count, by: 2) { arguments[args[i]] = args[i + 1] }
        }
        let icon = NSImageView(image: NSApp.applicationIconImage)
        icon.imageScaling = .scaleProportionallyUpOrDown
        icon.widthAnchor.constraint(equalToConstant: 56).isActive = true
        icon.heightAnchor.constraint(equalToConstant: 56).isActive = true
        let title = NSTextField(labelWithString: "公民链")
        title.font = .boldSystemFont(ofSize: 22)
        version.font = .systemFont(ofSize: 12)
        version.textColor = .secondaryLabelColor
        if arguments["--package"] == nil, let number = appVersion(embeddedApp) {
            version.stringValue = "v\(number)"
        }
        progress.minValue = 0
        progress.maxValue = 100
        progress.isIndeterminate = false
        progress.widthAnchor.constraint(equalToConstant: 340).isActive = true
        percentage.font = .monospacedDigitSystemFont(ofSize: 13, weight: .medium)
        status.maximumNumberOfLines = 2
        status.alignment = .center
        status.widthAnchor.constraint(equalToConstant: 340).isActive = true
        let content = NSStackView(views: [icon, title, status, progress, percentage])
        content.orientation = .vertical
        content.alignment = .centerX
        content.spacing = 12
        content.edgeInsets = NSEdgeInsets(top: 24, left: 24, bottom: 24, right: 24)
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 388, height: 232),
                          styleMask: [.titled, .closable], backing: .buffered, defer: false)
        window.title = "公民链安装程序"
        window.delegate = self
        window.standardWindowButton(.closeButton)?.isHidden = true
        let container = NSView()
        window.contentView = container
        content.translatesAutoresizingMaskIntoConstraints = false
        version.translatesAutoresizingMaskIntoConstraints = false
        cancelButton.translatesAutoresizingMaskIntoConstraints = false
        cancelButton.isBordered = false
        cancelButton.font = .systemFont(ofSize: 20)
        cancelButton.toolTip = "取消安装"
        cancelButton.setAccessibilityLabel("取消安装")
        container.addSubview(content)
        container.addSubview(version)
        container.addSubview(cancelButton)
        NSLayoutConstraint.activate([
            content.centerXAnchor.constraint(equalTo: container.centerXAnchor),
            content.centerYAnchor.constraint(equalTo: container.centerYAnchor),
            version.leadingAnchor.constraint(equalTo: title.trailingAnchor, constant: 8),
            version.firstBaselineAnchor.constraint(equalTo: title.firstBaselineAnchor),
            cancelButton.topAnchor.constraint(equalTo: container.topAnchor, constant: 4),
            cancelButton.trailingAnchor.constraint(equalTo: container.trailingAnchor, constant: -8),
            cancelButton.widthAnchor.constraint(equalToConstant: 28),
            cancelButton.heightAnchor.constraint(equalToConstant: 28),
        ])
        window.center()
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        DispatchQueue.global(qos: .userInitiated).async { self.install() }
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        cancelInstall()
        return false
    }

    @objc private func cancelInstall() {
        phaseLock.lock()
        let requested = phase == .installing
        if requested { phase = .cancelled }
        phaseLock.unlock()
        if requested {
            cancelButton.isEnabled = false
            status.stringValue = "正在取消安装"
        }
    }

    private func checkCancellation() throws {
        phaseLock.lock()
        defer { phaseLock.unlock() }
        if phase == .cancelled { throw InstallCancelled() }
    }

    private func finishInstallation() -> Bool {
        phaseLock.lock()
        defer { phaseLock.unlock() }
        let cancelled = phase == .cancelled
        phase = .finished
        return !cancelled
    }

    private func appVersion(_ source: URL) -> String? {
        Bundle(url: source)?.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String
    }

    private func show(_ value: Int, _ message: String) {
        DispatchQueue.main.async {
            guard self.cancelButton.isEnabled else { return }
            self.progress.doubleValue = Double(value)
            self.percentage.stringValue = "\(value)%"
            self.status.stringValue = message
        }
    }

    private func install() {
        let state = arguments["--state"].map { URL(fileURLWithPath: $0, isDirectory: true) }
        let package = arguments["--package"].map { URL(fileURLWithPath: $0) }
        let target = URL(fileURLWithPath: arguments["--destination"] ?? "/Applications/citizenchain.app")
        do {
            let lockPath = files.temporaryDirectory.appendingPathComponent("citizenchain-install.lock")
            let lock = open(lockPath.path, O_CREAT | O_RDWR | O_NOFOLLOW, 0o600)
            guard lock >= 0 else { throw InstallError("无法准备安装锁。") }
            defer { close(lock) }
            guard flock(lock, LOCK_EX | LOCK_NB) == 0 else {
                throw InstallError("公民链正在安装，请等待完成。")
            }
            guard target.isFileURL, target.pathExtension == "app" else {
                throw InstallError("安装位置无效。")
            }
            let team = try signingTeam()
            let requirement = "anchor apple generic and identifier \"\(identifier)\" and certificate leaf[subject.OU] = \"\(team)\""
            if let state {
                try "ready".write(to: state.appendingPathComponent("started"), atomically: true, encoding: .utf8)
            }
            let scratch = files.temporaryDirectory.appendingPathComponent("citizenchain-install-\(UUID().uuidString)")
            try files.createDirectory(at: scratch, withIntermediateDirectories: false, attributes: [.posixPermissions: 0o700])
            defer { try? files.removeItem(at: scratch) }
            let source: URL
            show(10, "正在验证安装包")
            if let package {
                let mount = scratch.appendingPathComponent("mount")
                try files.createDirectory(at: mount, withIntermediateDirectories: false)
                _ = try run("/usr/bin/hdiutil", ["attach", "-readonly", "-nobrowse", "-noautoopen", "-mountpoint", mount.path, package.path])
                defer { _ = try? run("/usr/bin/hdiutil", ["detach", mount.path]) }
                let apps = try files.contentsOfDirectory(at: mount, includingPropertiesForKeys: nil)
                    .filter { $0.pathExtension == "app" && Bundle(url: $0)?.bundleIdentifier == identifier }
                guard apps.count == 1 else { throw InstallError("安装包中缺少唯一的公民链应用。") }
                source = apps[0]
                try installApp(source: source, target: target, requirement: requirement)
            } else {
                source = embeddedApp
                try installApp(source: source, target: target, requirement: requirement)
            }
            guard finishInstallation() else { throw InstallCancelled() }
            show(90, "正在打开公民链")
            DispatchQueue.main.async { self.cancelButton.isEnabled = false }
            _ = try run("/usr/bin/open", [target.path])
            if let state {
                for name in ["ready.json", "started", "install-error.txt"] {
                    try? files.removeItem(at: state.appendingPathComponent(name))
                }
                if let package { try? files.removeItem(at: package) }
            }
            DispatchQueue.main.async {
                self.progress.doubleValue = 100
                self.percentage.stringValue = "100%"
                self.status.stringValue = "安装成功"
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { NSApp.terminate(nil) }
            }
        } catch {
            let cancelled = !finishInstallation() || error is InstallCancelled
            let message = cancelled ? "已取消安装" : error.localizedDescription
            if let state {
                try? message.write(to: state.appendingPathComponent("install-error.txt"), atomically: true, encoding: .utf8)
            }
            DispatchQueue.main.async {
                self.cancelButton.isEnabled = false
                self.status.stringValue = message
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { NSApp.terminate(nil) }
            }
        }
    }

    private func installApp(source: URL, target: URL, requirement: String) throws {
        _ = try run("/usr/bin/codesign", ["--verify", "--deep", "--strict", "-R", requirement, source.path])
        if let number = appVersion(source) {
            DispatchQueue.main.async { self.version.stringValue = "v\(number)" }
        }
        try checkCancellation()
        show(25, "正在等待公民链退出")
        let pid = arguments["--pid"].flatMap(Int32.init)
        let running = NSRunningApplication.runningApplications(withBundleIdentifier: identifier)
        for app in running where app.processIdentifier != pid { _ = app.terminate() }
        let deadline = Date().addingTimeInterval(45)
        while running.contains(where: { !$0.isTerminated }) || pid.map({ kill($0, 0) == 0 }) == true {
            try checkCancellation()
            guard Date() < deadline else { throw InstallError("公民链未退出，安装已停止。") }
            Thread.sleep(forTimeInterval: 0.2)
        }
        try checkCancellation()
        show(45, "正在安装公民链")
        // Let an active replacement finish so cancellation cannot leave half an app.
        try replaceApp(from: source, to: target, requirement: requirement)
    }
}

let app = NSApplication.shared
let installer = Installer()
app.setActivationPolicy(.regular)
app.delegate = installer
app.run()
