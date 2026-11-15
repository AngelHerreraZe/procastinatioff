// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import FamilyControls
import Foundation

enum Authorization {
    /// Modo personal: el propio usuario autoriza.
    static func requestIndividual() async throws {
        try await AuthorizationCenter.shared.requestAuthorization(for: .individual)
    }

    /// iPhone de un menor: Apple pide el Apple ID del tutor (En Familia). Con esta autorización el
    /// menor no puede borrar la app ni revocarla sin el tutor.
    static func requestChild() async throws {
        try await AuthorizationCenter.shared.requestAuthorization(for: .child)
    }

    static var granted: Bool { AuthorizationCenter.shared.authorizationStatus == .approved }
}

/// Traduce el estado del núcleo a escudos de Screen Time. Llamar tras cada acción, cada sync y al
/// volver a primer plano. El núcleo es la fuente de verdad; las extensiones solo cubren los huecos
/// mientras la app está cerrada.
enum StateBridge {
    static func refresh() {
        guard let core = try? CoreHolder.shared() else { return }
        let clock = CoreHolder.clock()
        guard let status = core.status(clock: clock) else { return } // sin perfil: no se bloquea nada
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
        // Selección de apps más permisiva pedida ayer: hoy ya rige (anti-impulso)
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

    /// Guarda la selección de apps. Añadir apps relaja: en un perfil personal rige desde el próximo
    /// despertar; en el iPhone de un menor no se permite (solo el tutor, en persona, al configurarlo).
    /// Quitar apps rige al momento. Devuelve un texto para la UI.
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
            return "Guardado."
        }
        if core?.myRole() == .minor {
            return "Añadir apps solo lo puede hacer tu tutor al configurar este iPhone. Puedes pedírselo."
        }
        let wake = Date(timeIntervalSince1970: Double(status!.nextWakeMs) / 1000)
        (s.pendingEssentials, s.pendingFocus, s.pendingFrom) = (essentials, focus, wake)
        SharedStore.save(s)
        return "Añadir apps relaja tus reglas: rige desde mañana al despertar."
    }
}
