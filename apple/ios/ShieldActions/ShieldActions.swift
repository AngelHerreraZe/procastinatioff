// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import ManagedSettings
import UserNotifications

/// Shield buttons. The shield cannot open the app: "Open ProcastinaciOFF" posts a
/// local notification that opens it when tapped.
final class ShieldActions: ShieldActionDelegate {
    override func handle(action: ShieldAction, for application: ApplicationToken, completionHandler: @escaping (ShieldActionResponse) -> Void) {
        respond(action, completionHandler)
    }

    override func handle(action: ShieldAction, for webDomain: WebDomainToken, completionHandler: @escaping (ShieldActionResponse) -> Void) {
        respond(action, completionHandler)
    }

    override func handle(action: ShieldAction, for category: ActivityCategoryToken, completionHandler: @escaping (ShieldActionResponse) -> Void) {
        respond(action, completionHandler)
    }

    private func respond(_ action: ShieldAction, _ done: @escaping (ShieldActionResponse) -> Void) {
        guard action == .secondaryButtonPressed else { return done(.close) }
        let content = UNMutableNotificationContent()
        content.title = "ProcastinaciOFF"
        content.body = String(localized: "Tap here to see your goals or use a pass.")
        UNUserNotificationCenter.current().add(UNNotificationRequest(identifier: "open-app", content: content, trigger: nil)) { _ in
            done(.defer)
        }
    }
}
