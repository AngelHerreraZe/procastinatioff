// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import BackgroundTasks
import Foundation
import Network
import UserNotifications

/// Sync without servers (plan 11): finds the profile's devices on the home network (Bonjour
/// `_pco._udp`), tells every core their address and syncs directly with them. Like on Android,
/// nothing is announced from the iPhone (the core only listens while a pairing QR is shown).
/// iOS does not let an app listen in the background: it syncs when the app becomes active, after
/// each own event and in `BGAppRefreshTask` (iOS decides when; it can take hours). Urgent approvals
/// use offline codes (plan 11b · step 4).
///
/// App wiring: `registerBackground()` in `App.init`; on `.active`: `StateBridge.refresh()` and
/// `syncSoon()`; on `.background`: `scheduleBackground()`.
enum LanSync {
    /// Must be listed in `BGTaskSchedulerPermittedIdentifiers` (Info.plist).
    static let refreshTask = "dev.procastinatioff.app.sync"
    private static let queue = DispatchQueue(label: "dev.procastinatioff.lan")
    private static let busy = NSLock()

    /// Syncs now in the background (after an own event or when the app opens).
    static func syncSoon() {
        DispatchQueue.global(qos: .utility).async { runOnce() }
    }

    static func registerBackground() {
        BGTaskScheduler.shared.register(forTaskWithIdentifier: refreshTask, using: nil) { task in
            scheduleBackground() // the next one
            let once = Once()
            task.expirationHandler = { once.run { task.setTaskCompleted(success: false) } }
            DispatchQueue.global(qos: .utility).async {
                runOnce()
                once.run { task.setTaskCompleted(success: true) }
            }
        }
    }

    static func scheduleBackground() {
        let request = BGAppRefreshTaskRequest(identifier: refreshTask)
        request.earliestBeginDate = Date(timeIntervalSinceNow: 15 * 60)
        try? BGTaskScheduler.shared.submit(request)
    }

    /// One full round (blocking). Skipped if another round is running.
    static func runOnce() {
        guard busy.try() else { return }
        defer { busy.unlock() }
        guard let core = try? CoreHolder.shared() else { return }
        checkScreenTime(core)
        // My trusted contact: approved changes come back; only alerts and numbers go out
        let contact = CoreHolder.existing("contact.db")
        if let contact { _ = try? core.forwardToContact(channel: contact, clock: CoreHolder.clock()) }
        let watching = CoreHolder.existing("watching.db")
        let families = FamilyProfiles.all().keys.compactMap { try? CoreHolder.family($0) }
        let cores = [core] + [contact, watching].compactMap { $0 } + families
        // Devices that are not here are normal (outside home): every device blocks with its own copy.
        browse(cores, nowMs: CoreHolder.clock().nowMs)
        for c in cores {
            _ = try? CoreClock.sync(c)
            if let notices = try? c.takeNotices() { Notices.show(notices, fromWatching: c === watching) }
        }
        // What the contact answered during this round is applied now, not at the next one
        if let contact { _ = try? core.forwardToContact(channel: contact, clock: CoreHolder.clock()) }
        StateBridge.refresh()
    }

    /// Screen Time authorization withdrawn with an active profile: recorded ONCE (the guardian or
    /// contact sees it), like Accessibility on Android.
    private static func checkScreenTime(_ core: Core) {
        guard core.status(clock: CoreHolder.clock()) != nil, let d = UserDefaults(suiteName: AppGroup.id) else { return }
        let key = "screen-time-reported"
        if Authorization.granted { return d.set(false, forKey: key) }
        guard !d.bool(forKey: key) else { return }
        try? core.reportTamper(clock: CoreHolder.clock(), what: .permissionRevoked)
        d.set(true, forKey: key)
    }

    /// Browses for `seconds` and tells every core about each device found. Blocking.
    private static func browse(_ cores: [Core], nowMs: Int64, seconds: TimeInterval = 4) {
        let pending = DispatchGroup()
        let browser = NWBrowser(for: .bonjour(type: "_pco._udp", domain: nil), using: .udp)
        browser.browseResultsChangedHandler = { _, changes in
            for change in changes {
                guard case let .added(result) = change, case let .service(name, _, _, _) = result.endpoint else { continue }
                pending.enter()
                resolve(result.endpoint) { ip, port in
                    if let ip {
                        for c in cores { _ = c.lanPeerSeen(nowMs: nowMs, tag: name, host: ip, port: port) }
                    }
                    pending.leave()
                }
            }
        }
        browser.start(queue: queue)
        Thread.sleep(forTimeInterval: seconds)
        browser.cancel()
        _ = pending.wait(timeout: .now() + 4)
    }

    /// A UDP "connection" to the Bonjour service resolves it to IP and port. Nothing is sent: UDP
    /// becomes ready without packets.
    private static func resolve(_ endpoint: NWEndpoint, _ done: @escaping (String?, UInt16) -> Void) {
        let conn = NWConnection(to: endpoint, using: .udp)
        let once = Once()
        let finish = { (ip: String?, port: UInt16) in
            once.run {
                conn.stateUpdateHandler = nil
                conn.cancel()
                done(ip, port)
            }
        }
        conn.stateUpdateHandler = { state in
            switch state {
            case .ready:
                guard case let .hostPort(host, port)? = conn.currentPath?.remoteEndpoint else { return finish(nil, 0) }
                finish(ipString(host), port.rawValue)
            case .failed, .cancelled:
                finish(nil, 0)
            default:
                break
            }
        }
        conn.start(queue: queue)
        queue.asyncAfter(deadline: .now() + 3) { finish(nil, 0) }
    }

    /// The core wants a bare IP address (no `%en0` interface suffix).
    private static func ipString(_ host: NWEndpoint.Host) -> String? {
        let text: String
        switch host {
        case .ipv4(let a): text = "\(a)"
        case .ipv6(let a): text = "\(a)"
        default: return nil
        }
        return text.split(separator: "%").first.map(String.init)
    }
}

/// Runs its body only the first time (a BGTask is completed exactly once; a resolution ends once).
private final class Once: @unchecked Sendable {
    private let lock = NSLock()
    private var done = false

    func run(_ body: () -> Void) {
        lock.lock()
        defer { lock.unlock() }
        guard !done else { return }
        done = true
        body()
    }
}

/// Local notifications with fixed, generic text: the lock screen shows no event data; the details
/// are only inside the app. One per kind (a new one replaces the previous).
enum Notices {
    static func requestPermission() {
        UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound]) { _, _ in }
    }

    /// `fromWatching`: they come from the person I support (their contact channel).
    static func show(_ notices: [Notice], fromWatching: Bool) {
        var shown = Set<String>()
        for notice in notices {
            let (id, text) = content(notice.kind, fromWatching)
            guard shown.insert(id).inserted else { continue }
            let c = UNMutableNotificationContent()
            c.title = "ProcastinatiOFF"
            c.body = text
            UNUserNotificationCenter.current().add(UNNotificationRequest(identifier: "notice-\(id)", content: c, trigger: nil))
        }
    }

    private static func content(_ kind: NoticeKind, _ fromWatching: Bool) -> (String, String) {
        switch kind {
        case .requestReceived:
            return ("request", String(localized: "You have requests from your family"))
        case .requestAnswered:
            return ("answer", String(localized: "Your request has an answer"))
        case .tamperAlert, .emergencyAlert:
            return fromWatching
                ? ("contact-alert", String(localized: "There are alerts from the person you support"))
                : ("family-alert", String(localized: "There are alerts from devices in your family"))
        case .deviceSilent:
            return ("silent", String(localized: "One of your devices has not been seen for a while"))
        case .localResetStarted:
            return ("reset", String(localized: "The person you support used their recovery code"))
        case .contactExitScheduled:
            return ("contact-exit", String(localized: "The person you support no longer wants to need your approval"))
        }
    }
}
