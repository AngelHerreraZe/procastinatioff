// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import ManagedSettings
import ManagedSettingsUI
import UIKit

/// Shield text. Always explains the block and the next step; never blames.
final class ShieldConfig: ShieldConfigurationDataSource {
    private func config() -> ShieldConfiguration {
        let mode = SharedStore.load().mode
        let (title, subtitle): (String, String) = switch mode {
        case .sleep: (String(localized: "Bedtime"), String(localized: "Calls keep working."))
        case .focus: (String(localized: "Time for your task"), String(localized: "During focus only that task's apps are available."))
        default: (String(localized: "You haven't met your goals yet"), String(localized: "Open ProcastinaciOFF to see what's left or use a pass."))
        }
        return ShieldConfiguration(
            backgroundBlurStyle: .systemThickMaterial,
            title: .init(text: title, color: .label),
            subtitle: .init(text: subtitle, color: .secondaryLabel),
            primaryButtonLabel: .init(text: String(localized: "Got it"), color: .white),
            secondaryButtonLabel: mode == .sleep ? nil : .init(text: String(localized: "Open ProcastinaciOFF"), color: .systemBlue))
    }

    override func configuration(shielding application: Application) -> ShieldConfiguration { config() }
    override func configuration(shielding application: Application, in category: ActivityCategory) -> ShieldConfiguration { config() }
    override func configuration(shielding webDomain: WebDomain) -> ShieldConfiguration { config() }
    override func configuration(shielding webDomain: WebDomain, in category: ActivityCategory) -> ShieldConfiguration { config() }
}
