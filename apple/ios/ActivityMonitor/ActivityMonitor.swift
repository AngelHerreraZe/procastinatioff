// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import DeviceActivity
import Foundation

/// Applies the shields when SLEEP starts/ends and when a pass ends, even with the app closed.
/// Very limited memory: NO Rust core; only the shared state. When the app opens, `StateBridge`
/// recomputes everything with the core (source of truth).
final class ActivityMonitor: DeviceActivityMonitor {
    private func apply(_ mode: ShieldMode, base: ShieldMode? = nil) {
        var s = SharedStore.load()
        if let base { s.baseMode = base }
        s.mode = mode
        SharedStore.save(s)
        Shields.apply(mode, s)
    }

    override func intervalDidStart(for activity: DeviceActivityName) {
        super.intervalDidStart(for: activity)
        // The night before a free day has no SLEEP (as in the core)
        if activity == .sleep && !FreeDays.nightIsFree(Date(), SharedStore.load()) { apply(.sleep) }
    }

    override func intervalDidEnd(for activity: DeviceActivityName) {
        super.intervalDidEnd(for: activity)
        switch activity {
        case .sleep:
            // New day: goals start from zero → LOCKED (or FREE if today is a free day)
            let base: ShieldMode = FreeDays.isFree(Date(), SharedStore.load()) ? .open : .locked
            apply(base, base: base)
        case .pass:
            apply(SharedStore.load().baseMode)
        default:
            break
        }
    }

    override func eventDidReachThreshold(_ event: DeviceActivityEvent.Name, activity: DeviceActivityName) {
        super.eventDidReachThreshold(event, activity: activity)
        if event == .passUsed { apply(SharedStore.load().baseMode) }
    }
}
