// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import UserNotifications

/// El push solo dice "Tienes una novedad" (el relay no sabe más). Aquí, EN el iPhone, se sincroniza,
/// se descifra y se sustituye el texto. Si algo falla, queda el texto genérico.
final class NotificationService: UNNotificationServiceExtension {
    override func didReceive(_ request: UNNotificationRequest, withContentHandler handler: @escaping (UNNotificationContent) -> Void) {
        guard let content = request.content.mutableCopy() as? UNMutableNotificationContent else { return handler(request.content) }
        defer { handler(content) }
        let clock = CoreHolder.clock()
        // Tutor: una solicitud nueva de un menor
        for (id, name) in FamilyProfiles.all() {
            guard let box = try? CoreHolder.family(id), let report = try? box.sync(clock: clock), report.received > 0,
                  let r = box.pendingRequests().last else { continue }
            content.body = "\(name) pide: \(Texts.describe(r.what))"
            return
        }
        if let box = CoreHolder.existing("watching.db"), let report = try? box.sync(clock: clock), report.received > 0 {
            content.body = "Hay avisos de la persona a la que acompañas"
            return
        }
        if let core = try? CoreHolder.shared(), let report = try? core.sync(clock: clock), report.received > 0 {
            content.body = "Hay novedades de tu otro dispositivo"
        }
    }
}
