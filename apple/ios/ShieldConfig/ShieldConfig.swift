// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import ManagedSettings
import ManagedSettingsUI
import UIKit

/// Shield text and look. Always explains the block and the next step; never blames. Colors follow
/// the app's rule (apple/ios/DESIGN.md): Nocturno at night, Calma by day.
final class ShieldConfig: ShieldConfigurationDataSource {
    private func config() -> ShieldConfiguration {
        let state = SharedStore.load()
        let p = Theme.current(state)
        let mode = state.mode
        let (title, subtitle, symbol): (String, String, String) = switch mode {
        case .sleep: (String(localized: "Bedtime"), String(localized: "Calls keep working."), "moon")
        case .focus: (String(localized: "Time for your task"), String(localized: "During focus only that task's apps are available."), "target")
        default: (String(localized: "You haven't met your goals yet"), String(localized: "Open ProcastinaciOFF to see what's left or use a pass."), "lock")
        }
        return ShieldConfiguration(
            backgroundColor: p.bg,
            icon: UIImage(systemName: symbol)?.withTintColor(p.accent, renderingMode: .alwaysOriginal),
            title: .init(text: title, color: p.text),
            subtitle: .init(text: subtitle, color: p.muted),
            primaryButtonLabel: .init(text: String(localized: "Got it"), color: p.onAccent),
            primaryButtonBackgroundColor: p.accent,
            secondaryButtonLabel: mode == .sleep ? nil : .init(text: String(localized: "Open ProcastinaciOFF"), color: p.accent))
    }

    override func configuration(shielding application: Application) -> ShieldConfiguration { config() }
    override func configuration(shielding application: Application, in category: ActivityCategory) -> ShieldConfiguration { config() }
    override func configuration(shielding webDomain: WebDomain) -> ShieldConfiguration { config() }
    override func configuration(shielding webDomain: WebDomain, in category: ActivityCategory) -> ShieldConfiguration { config() }
}
