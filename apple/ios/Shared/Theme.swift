// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import Foundation
import UIKit

/// One look's colors (apple/ios/DESIGN.md). Views take colors only from here, never a hex value.
/// SwiftUI: `Color(uiColor: palette.accent)`.
struct Palette {
    let bg, surface, chrome, text, muted, accent, onAccent, accentSoft, track, border, danger: UIColor
    let dark: Bool
}

/// The two looks of every screen: Calma by day, Nocturno from 1 h before bedtime until waking.
enum Theme {
    static let calma = Palette(
        bg: .hex(0xF5F7F5), surface: .hex(0xFFFFFF), chrome: .hex(0xEEF1EF), text: .hex(0x172321),
        muted: .hex(0x64736F), accent: .hex(0x087C70), onAccent: .hex(0xFFFFFF), accentSoft: .hex(0xDDF3ED),
        track: .hex(0xE6EBE8), border: .hex(0xE6EBE8), danger: .hex(0xB3403C), dark: false)

    static let nocturno = Palette(
        bg: .hex(0x0E1320), surface: .hex(0x161D2E), chrome: .hex(0x0B0F1A), text: .hex(0xECEFF7),
        muted: .hex(0xA9B3C7), accent: .hex(0xF5B544), onAccent: .hex(0x1A1300), accentSoft: .hex(0x2A3348),
        track: .hex(0x2A3348), border: .hex(0x2A3348), danger: .hex(0xFCA5A5), dark: true)

    /// Same rule as `lookFor` (Android) and App.svelte (desktop): SLEEP, or the last hour before
    /// bedtime on a night that has SLEEP. From the shared state, so the extensions can use it too.
    static func current(_ s: SharedState, now: Date = Date()) -> Palette {
        if s.mode == .sleep { return nocturno }
        guard s.bedtimeMin != s.wakeMin, !FreeDays.nightIsFree(now, s) else { return calma }
        let cal = Calendar.current
        let minute = cal.component(.hour, from: now) * 60 + cal.component(.minute, from: now)
        let untilBed = (s.bedtimeMin - minute + 1440) % 1440
        return untilBed <= 60 ? nocturno : calma
    }
}

extension UIColor {
    static func hex(_ v: UInt32) -> UIColor {
        UIColor(red: CGFloat((v >> 16) & 0xFF) / 255, green: CGFloat((v >> 8) & 0xFF) / 255,
                blue: CGFloat(v & 0xFF) / 255, alpha: 1)
    }
}
