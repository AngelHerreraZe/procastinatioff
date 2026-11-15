package dev.procastinacioff.app.block

import uniffi.procastinacioff_core.Config
import uniffi.procastinacioff_core.DayState

enum class Verdict { ALLOW, BLOCK }

/** Lógica pura: se prueba con tests unitarios JVM (sin dispositivo). */
object Policy {
    fun decide(state: DayState, pkg: String, appId: String?, cfg: Config?, systemAllowed: Set<String>): Verdict {
        when (state) {
            // Los días libres llegan como LIBRE desde el núcleo
            is DayState.Free, is DayState.Pass, is DayState.Emergency -> return Verdict.ALLOW
            else -> Unit
        }
        if (pkg in systemAllowed) return Verdict.ALLOW
        if (state is DayState.Sleep || cfg == null || appId == null) return Verdict.BLOCK
        val allowed = if (state is DayState.Focus) cfg.essentialApps + cfg.focusApps else cfg.essentialApps
        return if (allowed.any { it.equals(appId, ignoreCase = true) }) Verdict.ALLOW else Verdict.BLOCK
    }
}
