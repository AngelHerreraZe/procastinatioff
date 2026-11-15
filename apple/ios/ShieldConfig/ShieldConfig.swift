// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import ManagedSettings
import ManagedSettingsUI
import UIKit

/// Texto del escudo. Siempre explica el bloqueo y el siguiente paso; nunca culpa.
final class ShieldConfig: ShieldConfigurationDataSource {
    private func config() -> ShieldConfiguration {
        let mode = SharedStore.load().mode
        let (title, subtitle): (String, String) = switch mode {
        case .sleep: ("Hora de dormir", "Las llamadas siguen funcionando.")
        case .focus: ("Ahora toca tu tarea", "Durante el enfoque solo están disponibles las apps de esa tarea.")
        default: ("Aún no cumples tus metas", "Abre ProcastinaciOFF para ver qué te falta o usar un permiso.")
        }
        return ShieldConfiguration(
            backgroundBlurStyle: .systemThickMaterial,
            title: .init(text: title, color: .label),
            subtitle: .init(text: subtitle, color: .secondaryLabel),
            primaryButtonLabel: .init(text: "Entendido", color: .white),
            secondaryButtonLabel: mode == .sleep ? nil : .init(text: "Abrir ProcastinaciOFF", color: .systemBlue))
    }

    override func configuration(shielding application: Application) -> ShieldConfiguration { config() }
    override func configuration(shielding application: Application, in category: ActivityCategory) -> ShieldConfiguration { config() }
    override func configuration(shielding webDomain: WebDomain) -> ShieldConfiguration { config() }
    override func configuration(shielding webDomain: WebDomain, in category: ActivityCategory) -> ShieldConfiguration { config() }
}
