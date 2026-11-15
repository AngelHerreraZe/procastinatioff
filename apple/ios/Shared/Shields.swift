// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import FamilyControls
import ManagedSettings

extension ManagedSettingsStore.Name {
    static let day = Self("day")
    static let protection = Self("protection")
}

/// Applies the Screen Time shields for the mode. Used by the app and the extensions.
enum Shields {
    static func apply(_ mode: ShieldMode, _ s: SharedState) {
        ManagedSettingsStore(named: .protection).application.denyAppRemoval = true
        let store = ManagedSettingsStore(named: .day)
        store.clearAllSettings()
        switch mode {
        case .open:
            break
        case .sleep:
            // The Phone is a system app that is never blocked: calls keep coming in
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
