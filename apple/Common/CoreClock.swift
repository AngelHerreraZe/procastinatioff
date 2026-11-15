// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import Foundation

/// Hora para las reglas: hora firmada del relay + reloj monotónico (ADR 0007). Cambiar la hora del
/// sistema no la mueve. Sin ancla todavía, el reloj del sistema.
enum CoreClock {
    /// En Darwin, CLOCK_MONOTONIC sigue contando con el equipo en reposo (systemUptime no).
    static func monoMs() -> Int64 { Int64(clock_gettime_nsec_np(CLOCK_MONOTONIC) / 1_000_000) }

    static func deviceNowMs() -> Int64 { Int64(Date().timeIntervalSince1970 * 1000) }

    static func now(_ core: Core?) -> Clock {
        let now = core?.trustedNowMs(deviceNowMs: deviceNowMs(), monoMs: monoMs()) ?? deviceNowMs()
        let offset = TimeZone.current.secondsFromGMT(for: Date(timeIntervalSince1970: Double(now) / 1000)) / 60
        return Clock(nowMs: now, utcOffsetMin: Int32(offset))
    }

    /// Ancla la hora con el relay (llamar antes de cada sincronización). Sin red, no pasa nada.
    static func anchor(_ core: Core) {
        try? core.anchorTime(deviceNowMs: deviceNowMs(), monoMs: monoMs())
    }
}

/// Texto para la persona usuaria: el motivo del núcleo (en español, sin datos personales) o uno genérico.
func userMessage(_ error: Error) -> String {
    if case .Denied(let reason, _)? = error as? CoreError { return reason }
    if case .Network? = error as? CoreError { return "Sin conexión con el servidor. Se reintentará." }
    return "Algo falló. Inténtalo de nuevo."
}
