// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import FamilyControls
import Foundation

/// State shared between the app and its extensions (App Group). The Screen Time extensions do NOT
/// load the core (very limited memory): they read this. SECURITY: Nothing from the camera or audio is ever written here.
enum ShieldMode: String, Codable { case sleep, locked, focus, open }

struct SharedState: Codable {
    /// Apps chosen with the system picker: opaque tokens valid only on THIS iPhone, which is why
    /// they do not travel in `Config`.
    var essentials = FamilyActivitySelection()
    var focus = FamilyActivitySelection()
    /// "All categories": to measure the minutes of use of a pass.
    var everything = FamilyActivitySelection()
    /// More permissive selection asked for in a personal profile: takes effect from `pendingFrom` (anti-impulse).
    var pendingEssentials: FamilyActivitySelection?
    var pendingFocus: FamilyActivitySelection?
    var pendingFrom: Date?

    var webAllowlist: [String] = []
    /// Mode outside passes: `open` if the key goals are already done, otherwise `locked`.
    var baseMode: ShieldMode = .locked
    var mode: ShieldMode = .locked
    /// From `Config` (for the extensions): schedule and free days (bit 0 = Monday).
    var wakeMin = 390
    var bedtimeMin = 1350
    var freeDays = 0
}

enum SharedStore {
    private static let key = "shared-state-v1"
    private static var defaults: UserDefaults? { UserDefaults(suiteName: AppGroup.id) }

    static func load() -> SharedState {
        guard let data = defaults?.data(forKey: key), let s = try? JSONDecoder().decode(SharedState.self, from: data) else {
            return SharedState()
        }
        return s
    }

    static func save(_ s: SharedState) {
        if let data = try? JSONEncoder().encode(s) { defaults?.set(data, forKey: key) }
    }
}

extension FamilyActivitySelection {
    /// Does `self` let through something `old` did not? (adding essential or focus apps relaxes).
    func widens(_ old: FamilyActivitySelection) -> Bool {
        !applicationTokens.isSubset(of: old.applicationTokens)
            || !categoryTokens.isSubset(of: old.categoryTokens)
            || !webDomainTokens.isSubset(of: old.webDomainTokens)
    }
}

/// Free days with the same mask as the core (bit 0 = Monday … bit 6 = Sunday).
enum FreeDays {
    static func bit(for date: Date) -> Int {
        let weekday = Calendar.current.component(.weekday, from: date) // 1 = Sunday … 7 = Saturday
        return 1 << ((weekday + 5) % 7)
    }

    static func isFree(_ date: Date, _ s: SharedState) -> Bool { s.freeDays & bit(for: date) != 0 }

    /// The night starting now ends at the next wake-up; it is free if that day is free
    /// (same as `night_is_free` in core/src/rules.rs).
    static func nightIsFree(_ now: Date, _ s: SharedState) -> Bool {
        let cal = Calendar.current
        let minute = cal.component(.hour, from: now) * 60 + cal.component(.minute, from: now)
        let wakeDay = minute < s.wakeMin ? now : cal.date(byAdding: .day, value: 1, to: now) ?? now
        return isFree(wakeDay, s)
    }
}
