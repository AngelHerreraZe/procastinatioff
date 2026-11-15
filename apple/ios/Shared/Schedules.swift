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

/// Schedules applied with the app closed (the `ActivityMonitor` extension runs them).
enum Schedules {
    static let center = DeviceActivityCenter()

    /// SLEEP every day (crosses midnight; VERIFY: confirm in spike D). The extension skips the nights before a
    /// free day.
    static func scheduleSleep(bedtimeMin: Int, wakeMin: Int) throws {
        center.stopMonitoring([.sleep])
        guard bedtimeMin != wakeMin else { return } // no sleep period
        let schedule = DeviceActivitySchedule(
            intervalStart: DateComponents(hour: bedtimeMin / 60, minute: bedtimeMin % 60),
            intervalEnd: DateComponents(hour: wakeMin / 60, minute: wakeMin % 60),
            repeats: true)
        try center.startMonitoring(.sleep, during: schedule)
    }

    /// Pass of N minutes of USE: minimum 15-min window + an event when N minutes are reached.
    /// If "everything" was not chosen in the picker, the pass lasts the 15-min window.
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
