// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import DeviceActivity
import FamilyControls
import Foundation

extension DeviceActivityName {
    static let sleep = Self("sleep")
    static let pass = Self("pass")
}

extension DeviceActivityEvent.Name {
    static let passUsed = Self("passUsed")
}

/// Horarios que se aplican con la app cerrada (la extensión `ActivityMonitor` los ejecuta).
enum Schedules {
    static let center = DeviceActivityCenter()

    /// DORMIR todos los días (cruza medianoche; ⚠️ confirmar en el spike D). Las noches previas a un
    /// día libre las salta la extensión.
    static func scheduleSleep(bedtimeMin: Int, wakeMin: Int) throws {
        center.stopMonitoring([.sleep])
        guard bedtimeMin != wakeMin else { return } // sin periodo de sueño
        let schedule = DeviceActivitySchedule(
            intervalStart: DateComponents(hour: bedtimeMin / 60, minute: bedtimeMin % 60),
            intervalEnd: DateComponents(hour: wakeMin / 60, minute: wakeMin % 60),
            repeats: true)
        try center.startMonitoring(.sleep, during: schedule)
    }

    /// Permiso de N minutos de USO: ventana mínima de 15 min + evento al alcanzar N minutos.
    /// Si no se eligió "todo" en el selector, el permiso dura la ventana de 15 min.
    static func startPass(minutes: Int, everything: FamilyActivitySelection) throws {
        let cal = Calendar.current
        let now = Date()
        let schedule = DeviceActivitySchedule(
            intervalStart: cal.dateComponents([.hour, .minute, .second], from: now),
            intervalEnd: cal.dateComponents([.hour, .minute, .second], from: now.addingTimeInterval(15 * 60)),
            repeats: false)
        let used = DeviceActivityEvent(
            applications: everything.applicationTokens,
            categories: everything.categoryTokens,
            webDomains: everything.webDomainTokens,
            threshold: DateComponents(minute: minutes))
        center.stopMonitoring([.pass])
        try center.startMonitoring(.pass, during: schedule, events: [.passUsed: used])
    }
}
