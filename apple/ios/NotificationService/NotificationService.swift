// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import UserNotifications

/// The push only says "You have news" (the relay knows nothing else). Here, ON the iPhone, it syncs,
/// decrypts and replaces the text. If anything fails, the generic text stays. (Removed when sync goes serverless.)
final class NotificationService: UNNotificationServiceExtension {
    override func didReceive(_ request: UNNotificationRequest, withContentHandler handler: @escaping (UNNotificationContent) -> Void) {
        guard let content = request.content.mutableCopy() as? UNMutableNotificationContent else { return handler(request.content) }
        defer { handler(content) }
        let clock = CoreHolder.clock()
        // Guardian: a new request from a minor
        for (id, name) in FamilyProfiles.all() {
            guard let box = try? CoreHolder.family(id), let report = try? box.sync(clock: clock), report.received > 0,
                  let r = box.pendingRequests().last else { continue }
            content.body = String(localized: "\(name) asks: \(Texts.describe(r.what))")
            return
        }
        if let box = CoreHolder.existing("watching.db"), let report = try? box.sync(clock: clock), report.received > 0 {
            content.body = String(localized: "There are alerts from the person you support")
            return
        }
        if let core = try? CoreHolder.shared(), let report = try? core.sync(clock: clock), report.received > 0 {
            content.body = String(localized: "There is news from your other device")
        }
    }
}
