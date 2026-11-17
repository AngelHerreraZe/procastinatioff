package dev.procastinatioff.app.block

import uniffi.procastinatioff_core.Config
import uniffi.procastinatioff_core.DayState

enum class Verdict { ALLOW, BLOCK }

/** Pure logic: tested with JVM unit tests (no device). */
object Policy {
    fun decide(state: DayState, pkg: String, appId: String?, cfg: Config?, systemAllowed: Set<String>): Verdict {
        when (state) {
            // Free days arrive from the core as FREE
            is DayState.Free, is DayState.Pass, is DayState.Emergency -> return Verdict.ALLOW
            else -> Unit
        }
        if (pkg in systemAllowed) return Verdict.ALLOW
        if (state is DayState.Sleep || cfg == null || appId == null) return Verdict.BLOCK
        // During FOCUS the core already merged the global focus apps with the goal's own.
        val allowed = if (state is DayState.Focus) cfg.essentialApps + state.apps else cfg.essentialApps
        return if (allowed.any { it.equals(appId, ignoreCase = true) }) Verdict.ALLOW else Verdict.BLOCK
    }
}
