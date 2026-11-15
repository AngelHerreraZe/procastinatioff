// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import FamilyControls
import Foundation

enum Authorization {
    /// Personal mode: the user authorizes themselves.
    static func requestIndividual() async throws {
        try await AuthorizationCenter.shared.requestAuthorization(for: .individual)
    }

    /// A minor's iPhone: Apple asks for the guardian's Apple ID (Family Sharing). With this authorization the
    /// minor cannot delete the app nor revoke it without the guardian.
    static func requestChild() async throws {
        try await AuthorizationCenter.shared.requestAuthorization(for: .child)
    }

    static var granted: Bool { AuthorizationCenter.shared.authorizationStatus == .approved }
}

/// Translates the core's state into Screen Time shields. Call after every action, every sync and when
/// returning to the foreground. The core is the source of truth; the extensions only cover the gaps
/// while the app is closed.
enum StateBridge {
    static func refresh() {
        guard let core = try? CoreHolder.shared() else { return }
        let clock = CoreHolder.clock()
        guard let status = core.status(clock: clock) else { return } // no profile: nothing is blocked
        var s = SharedStore.load()
        let cfg = core.activeConfig(clock: clock)
        s.webAllowlist = cfg?.webAllowlist ?? []
        if let cfg {
            let (wake, bed) = (Int(cfg.schedule.wake.minutes), Int(cfg.schedule.bedtime.minutes))
            if wake != s.wakeMin || bed != s.bedtimeMin {
                try? Schedules.scheduleSleep(bedtimeMin: bed, wakeMin: wake)
            }
            (s.wakeMin, s.bedtimeMin, s.freeDays) = (wake, bed, Int(cfg.freeDays))
        }
        // More permissive app selection asked for yesterday: it takes effect today (anti-impulse)
        if let from = s.pendingFrom, Date() >= from {
            if let e = s.pendingEssentials { s.essentials = e }
            if let f = s.pendingFocus { s.focus = f }
            (s.pendingEssentials, s.pendingFocus, s.pendingFrom) = (nil, nil, nil)
        }
        let keysDone = status.goals.filter { $0.kind == .key }.allSatisfy { $0.completed }
        s.baseMode = status.freeDay || (!status.needsPlan && keysDone) ? .open : .locked
        switch status.state {
        case .sleep: s.mode = .sleep
        case .locked: s.mode = .locked
        case .focus: s.mode = .focus
        case .free, .pass, .emergency: s.mode = .open
        }
        SharedStore.save(s)
        Shields.apply(s.mode, s)
    }

    /// Saves the app selection. Adding apps relaxes: in a personal profile it takes effect at the next
    /// wake-up; on a minor's iPhone it is not allowed (only the guardian, in person, when setting it up).
    /// Removing apps takes effect immediately. Returns a text for the UI.
    static func saveSelection(essentials: FamilyActivitySelection, focus: FamilyActivitySelection,
                              everything: FamilyActivitySelection, setup: Bool) -> String {
        var s = SharedStore.load()
        s.everything = everything
        let widens = essentials.widens(s.essentials) || focus.widens(s.focus)
        let core = try? CoreHolder.shared()
        let status = core?.status(clock: CoreHolder.clock())
        if setup || !widens || status == nil {
            (s.essentials, s.focus, s.pendingEssentials, s.pendingFocus, s.pendingFrom) = (essentials, focus, nil, nil, nil)
            SharedStore.save(s)
            refresh()
            return String(localized: "Saved.")
        }
        if core?.myRole() == .minor {
            return String(localized: "Only your guardian can add apps, when setting up this iPhone. You can ask them.")
        }
        let wake = Date(timeIntervalSince1970: Double(status!.nextWakeMs) / 1000)
        (s.pendingEssentials, s.pendingFocus, s.pendingFrom) = (essentials, focus, wake)
        SharedStore.save(s)
        return String(localized: "Adding apps relaxes your rules: it takes effect tomorrow when you wake up.")
    }
}
