// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import DeviceActivity
import Foundation

/// Aplica los escudos al empezar/terminar DORMIR y al acabar un permiso, aunque la app esté cerrada.
/// Memoria muy limitada: SIN núcleo Rust; solo el estado compartido. Al abrir la app, `StateBridge`
/// vuelve a calcular todo con el núcleo (fuente de verdad).
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
        // La noche previa a un día libre no hay DORMIR (como en el núcleo)
        if activity == .sleep && !FreeDays.nightIsFree(Date(), SharedStore.load()) { apply(.sleep) }
    }

    override func intervalDidEnd(for activity: DeviceActivityName) {
        super.intervalDidEnd(for: activity)
        switch activity {
        case .sleep:
            // Nuevo día: las metas empiezan de cero → BLOQUEADO (o LIBRE si hoy es día libre)
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
