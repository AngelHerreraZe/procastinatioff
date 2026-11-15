// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import Foundation

/// Time for the rules: relay signed time + monotonic clock. Changing the system time
/// does not move it. Without an anchor yet, the system clock.
enum CoreClock {
    /// On Darwin, CLOCK_MONOTONIC keeps counting while the device sleeps (systemUptime does not).
    static func monoMs() -> Int64 { Int64(clock_gettime_nsec_np(CLOCK_MONOTONIC) / 1_000_000) }

    static func deviceNowMs() -> Int64 { Int64(Date().timeIntervalSince1970 * 1000) }

    static func now(_ core: Core?) -> Clock {
        let now = core?.trustedNowMs(deviceNowMs: deviceNowMs(), monoMs: monoMs()) ?? deviceNowMs()
        let offset = TimeZone.current.secondsFromGMT(for: Date(timeIntervalSince1970: Double(now) / 1000)) / 60
        return Clock(nowMs: now, utcOffsetMin: Int32(offset))
    }

    /// Anchors the time with the relay (call before every sync). Without network, nothing happens.
    static func anchor(_ core: Core) {
        try? core.anchorTime(deviceNowMs: deviceNowMs(), monoMs: monoMs())
    }
}

/// Text for the user. The core never sends sentences: `Denied.reason` is a stable code (`Denied::code` in
/// core/src/rules.rs) translated with the `denied.<code>` keys of Localizable.xcstrings.
func userMessage(_ error: Error) -> String {
    let generic = String(localized: "Something went wrong. Please try again.")
    switch error as? CoreError {
    case .Denied(let reason, let minutes)?:
        let key = "denied.\(reason)"
        let text = NSLocalizedString(key, comment: "Refusal from the core rules")
        return text == key ? generic : String(format: text, Int(minutes))
    case .Network?:
        return String(localized: "No connection with your other devices. It will retry.")
    default:
        return generic
    }
}
