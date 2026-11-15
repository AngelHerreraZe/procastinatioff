// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import FamilyControls
import ManagedSettings

extension ManagedSettingsStore.Name {
    static let day = Self("day")
    static let protection = Self("protection")
}

/// Aplica los escudos de Screen Time según el modo. Lo usan la app y las extensiones.
enum Shields {
    static func apply(_ mode: ShieldMode, _ s: SharedState) {
        ManagedSettingsStore(named: .protection).application.denyAppRemoval = true
        let store = ManagedSettingsStore(named: .day)
        store.clearAllSettings()
        switch mode {
        case .open:
            break
        case .sleep:
            // El Teléfono es una app del sistema que nunca se bloquea: las llamadas siguen entrando
            store.shield.applicationCategories = .all()
            store.shield.webDomainCategories = .all()
        case .locked:
            store.shield.applicationCategories = .all(except: s.essentials.applicationTokens)
            store.shield.webDomainCategories = .all(except: s.essentials.webDomainTokens)
        case .focus:
            store.shield.applicationCategories = .all(except: s.essentials.applicationTokens.union(s.focus.applicationTokens))
            store.webContent.blockedByFilter = .all(except: Set(s.webAllowlist.map { WebDomain(domain: $0) }))
        }
    }
}
