// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import FamilyControls
import Foundation

/// Estado compartido entre la app y sus extensiones (App Group). Las extensiones de Screen Time NO
/// cargan el núcleo (memoria muy limitada): leen esto. 🔒 Nada de cámara ni audio se escribe nunca aquí.
enum ShieldMode: String, Codable { case sleep, locked, focus, open }

struct SharedState: Codable {
    /// Apps elegidas con el selector del sistema: tokens opacos válidos solo en ESTE iPhone, por eso
    /// no viajan en `Config` (08 · 1).
    var essentials = FamilyActivitySelection()
    var focus = FamilyActivitySelection()
    /// "Todas las categorías": para medir los minutos de uso de un permiso.
    var everything = FamilyActivitySelection()
    /// Selección más permisiva pedida en un perfil personal: rige desde `pendingFrom` (anti-impulso).
    var pendingEssentials: FamilyActivitySelection?
    var pendingFocus: FamilyActivitySelection?
    var pendingFrom: Date?

    var webAllowlist: [String] = []
    /// Modo fuera de permisos: `open` si ya cumplió las metas llave, si no `locked`.
    var baseMode: ShieldMode = .locked
    var mode: ShieldMode = .locked
    /// Del `Config` (para las extensiones): horario y días libres (bit 0 = lunes).
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
    /// ¿`self` deja pasar algo que `old` no dejaba? (añadir apps esenciales o de enfoque relaja).
    func widens(_ old: FamilyActivitySelection) -> Bool {
        !applicationTokens.isSubset(of: old.applicationTokens)
            || !categoryTokens.isSubset(of: old.categoryTokens)
            || !webDomainTokens.isSubset(of: old.webDomainTokens)
    }
}

/// Días libres con la misma máscara que el núcleo (bit 0 = lunes … bit 6 = domingo).
enum FreeDays {
    static func bit(for date: Date) -> Int {
        let weekday = Calendar.current.component(.weekday, from: date) // 1 = domingo … 7 = sábado
        return 1 << ((weekday + 5) % 7)
    }

    static func isFree(_ date: Date, _ s: SharedState) -> Bool { s.freeDays & bit(for: date) != 0 }

    /// La noche que empieza ahora termina en el próximo despertar; es libre si ese día es libre
    /// (igual que `night_is_free` en core/src/rules.rs).
    static func nightIsFree(_ now: Date, _ s: SharedState) -> Bool {
        let cal = Calendar.current
        let minute = cal.component(.hour, from: now) * 60 + cal.component(.minute, from: now)
        let wakeDay = minute < s.wakeMin ? now : cal.date(byAdding: .day, value: 1, to: now) ?? now
        return isFree(wakeDay, s)
    }
}
