// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import ManagedSettings
import UserNotifications

/// Botones del escudo. El escudo no puede abrir la app: "Abrir ProcastinaciOFF" publica una
/// notificación local que la abre al tocarla.
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
        content.body = "Toca aquí para ver tus metas o usar un permiso."
        UNUserNotificationCenter.current().add(UNNotificationRequest(identifier: "open-app", content: content, trigger: nil)) { _ in
            done(.defer)
        }
    }
}
