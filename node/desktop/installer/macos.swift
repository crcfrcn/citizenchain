import AppKit
import Darwin
import MachO
import os

private let identifier = "macOS.citizenappchain"
private let files = FileManager.default
private let log = Logger(subsystem: "macOS.citizenappchain", category: "installer")

private func installerData(_ section: String) -> Data? {
    guard let image = _dyld_get_image_header(0) else { return nil }
    var size: UInt = 0
    let bytes = image.withMemoryRebound(to: mach_header_64.self, capacity: 1) {
        getsectiondata($0, "__DATA_CONST", section, &size)
    }
    guard let bytes, size > 0, let count = Int(exactly: size) else { return nil }
    return Data(bytesNoCopy: UnsafeMutableRawPointer(bytes), count: count, deallocator: .none)
}

private struct InstallError: LocalizedError {
    let errorDescription: String?
    init(_ description: String) { errorDescription = description }
}

private func run(_ executable: String, _ arguments: [String]) throws {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: executable)
    process.arguments = arguments
    let diagnostics = Pipe()
    process.standardOutput = FileHandle.nullDevice
    process.standardError = diagnostics
    try process.run()
    let error = diagnostics.fileHandleForReading.readDataToEndOfFile()
    process.waitUntilExit()
    guard process.terminationStatus == 0 else {
        log.error("\(executable, privacy: .public) 退出码 \(process.terminationStatus, privacy: .public)：\(String(decoding: error.prefix(4096), as: UTF8.self), privacy: .private)")
        throw InstallError("系统操作失败或已取消。")
    }
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
    /bin/mv "$work/citizenchain.app" "$destination"
    """
    let args = [source.path, destination.path, requirement]
    if files.isWritableFile(atPath: destination.deletingLastPathComponent().path) {
        try run("/bin/sh", ["-c", script, "citizenchain-install"] + args)
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
        try run("/usr/bin/osascript", ["-e", authorization, "--", script] + args)
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

    private var installerBundle: URL {
        URL(fileURLWithPath: CommandLine.arguments[0]).resolvingSymlinksInPath()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
    }

    private var embeddedApp: URL {
        let installer = installerBundle
        let packaged = installer.appendingPathComponent("Contents/Helpers/citizenchain.app", isDirectory: true)
        let parent = installer.deletingLastPathComponent()
        // 随公民链内置的入口在 Contents/Resources/installer 下。
        let resources = parent.deletingLastPathComponent()
        let contents = resources.deletingLastPathComponent()
        if parent.lastPathComponent == "installer", resources.lastPathComponent == "Resources",
           contents.lastPathComponent == "Contents" {
            return contents.deletingLastPathComponent()
        }
        return packaged
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        let args = Array(CommandLine.arguments.dropFirst())
        if args.count.isMultiple(of: 2) {
            for i in stride(from: 0, to: args.count, by: 2) { arguments[args[i]] = args[i + 1] }
        }
        let image = installerData("__cc_icon").flatMap { NSImage(data: $0) }
        let icon = NSImageView(image: image ?? NSApp.applicationIconImage)
        icon.imageScaling = .scaleProportionallyUpOrDown
        icon.widthAnchor.constraint(equalToConstant: 56).isActive = true
        icon.heightAnchor.constraint(equalToConstant: 56).isActive = true
        let title = NSTextField(labelWithString: "公民链")
        title.font = .boldSystemFont(ofSize: 22)
        version.font = .systemFont(ofSize: 12)
        version.textColor = .secondaryLabelColor
        if arguments["--package"] == nil {
            version.stringValue = "v\(InstallerSigning.version)"
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
        let info = source.appendingPathComponent("Contents/Info.plist")
        return NSDictionary(contentsOf: info)?["CFBundleShortVersionString"] as? String
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
            // 构建时从公民链签名声明写入固定团队，校验内置应用及待替换的旧版本。
            // codesign 的 -R 用等号区分内联规则和规则文件路径。
            let requirement = "=anchor apple generic and identifier \"\(identifier)\" and certificate leaf[subject.OU] = \"\(InstallerSigning.team)\""
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
                try run("/usr/bin/hdiutil", ["attach", "-readonly", "-nobrowse", "-noautoopen", "-mountpoint", mount.path, package.path])
                defer { try? run("/usr/bin/hdiutil", ["detach", mount.path]) }
                let apps = try files.contentsOfDirectory(at: mount, includingPropertiesForKeys: nil)
                    .filter { $0.pathExtension == "app" && Bundle(url: $0)?.bundleIdentifier == identifier }
                guard apps.count == 1 else { throw InstallError("安装包中缺少唯一的公民链应用。") }
                source = apps[0]
                try installApp(source: source, target: target, requirement: requirement)
            } else {
                // 从已运行的签名可执行文件读取内置应用，桌面启动时不依赖读取包目录。
                source = scratch.appendingPathComponent("citizenchain.app", isDirectory: true)
                if let payload = installerData("__cc_payload") {
                    let archive = scratch.appendingPathComponent("payload.tar")
                    try payload.write(to: archive, options: .withoutOverwriting)
                    try run("/usr/bin/tar", ["-xpf", archive.path, "-C", scratch.path])
                    try files.removeItem(at: archive)
                } else {
                    try run("/usr/bin/ditto", [embeddedApp.path, source.path])
                }
                try installApp(source: source, target: target, requirement: requirement)
            }
            guard finishInstallation() else { throw InstallCancelled() }
            show(90, "正在打开公民链")
            DispatchQueue.main.async { self.cancelButton.isEnabled = false }
            try run("/usr/bin/open", [target.path])
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
            if !cancelled {
                let failure = error as NSError
                log.error("安装失败：\(failure.domain, privacy: .public) \(failure.code, privacy: .public) \(message, privacy: .private)")
            }
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
        try run("/usr/bin/codesign", ["--verify", "--deep", "--strict", "-R", requirement, source.path])
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

@main
enum CitizenChainInstaller {
    static func main() {
        let app = NSApplication.shared
        let installer = Installer()
        app.setActivationPolicy(.regular)
        app.delegate = installer
        withExtendedLifetime(installer) { app.run() }
    }
}
