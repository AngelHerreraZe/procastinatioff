package dev.procastinacioff.app.block

import android.accessibilityservice.AccessibilityService
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.SystemClock
import android.view.accessibility.AccessibilityEvent
import androidx.core.content.ContextCompat
import dev.procastinacioff.app.MainActivity
import dev.procastinacioff.app.core.CoreHolder
import dev.procastinacioff.app.shield.ShieldActivity
import dev.procastinacioff.app.sync.SyncScheduler
import kotlin.math.abs
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import uniffi.procastinacioff_core.DayState
import uniffi.procastinacioff_core.TamperKind

/**
 * 🔒 Único componente de larga vida. Recibe solo el NOMBRE DE PAQUETE de la app en primer plano
 * (canRetrieveWindowContent=false) y muestra el escudo si no está permitida.
 */
class BlockerService : AccessibilityService() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private var foreground: String? = null
    private var lastState: DayState? = null
    private var lastShieldAt = 0L

    /** Primer desbloqueo tras la hora de dormir: abre el plan del día. */
    private val unlockReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            val st = runCatching { CoreHolder.get(context).status(CoreHolder.clock()) }.getOrNull() ?: return
            if (st.needsPlan && st.state is DayState.Locked) {
                startActivity(MainActivity.intent(context, MainActivity.ROUTE_PLAN))
            }
        }
    }

    override fun onServiceConnected() {
        ContextCompat.registerReceiver(
            this, unlockReceiver, IntentFilter(Intent.ACTION_USER_PRESENT), ContextCompat.RECEIVER_NOT_EXPORTED,
        )
        scope.launch { ticker() }
    }

    override fun onAccessibilityEvent(event: AccessibilityEvent) {
        if (event.eventType != AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED) return
        val pkg = event.packageName?.toString() ?: return
        foreground = pkg
        enforce(pkg)
    }

    /** Cada 30 s: transiciones de estado (DORMIR, fin de permiso) aunque nadie toque el teléfono. */
    private suspend fun ticker() {
        var lastSync = 0L
        var lastWall = System.currentTimeMillis()
        var lastElapsed = SystemClock.elapsedRealtime()
        while (scope.isActive) {
            val core = CoreHolder.get(this)
            // Cambio de hora manual: se registra y se sincroniza (no se puede impedir sin Device Owner)
            val wall = System.currentTimeMillis()
            val elapsed = SystemClock.elapsedRealtime()
            if (abs((wall - lastWall) - (elapsed - lastElapsed)) > 120_000) {
                runCatching { core.reportTamper(CoreHolder.clock(), TamperKind.CLOCK_CHANGED) }
                SyncScheduler.syncNow(this)
            }
            lastWall = wall
            lastElapsed = elapsed

            val state = runCatching { core.status(CoreHolder.clock())?.state }.getOrNull()
            if (state != null && state != lastState) {
                DndController.apply(this, sleeping = state is DayState.Sleep)
                lastState = state
                foreground?.let { enforce(it) } // la app abierta puede haber dejado de estar permitida
            }
            if (SystemClock.elapsedRealtime() - lastSync > 5 * 60_000) {
                lastSync = SystemClock.elapsedRealtime()
                SyncScheduler.syncNow(this)
            }
            delay(30_000)
        }
    }

    private fun enforce(pkg: String) {
        if (pkg == packageName) return
        val core = CoreHolder.get(this)
        val clock = CoreHolder.clock()
        val state = runCatching { core.status(clock)?.state }.getOrNull() ?: return // sin perfil: no se bloquea nada
        val verdict = Policy.decide(
            state = state,
            pkg = pkg,
            appId = AppIdentity.idOf(this, pkg),
            cfg = core.activeConfig(clock),
            systemAllowed = AppIdentity.systemAllowed(this, sleeping = state is DayState.Sleep),
        )
        if (verdict == Verdict.BLOCK) showShield(pkg)
    }

    private fun showShield(pkg: String) {
        val now = SystemClock.elapsedRealtime()
        if (now - lastShieldAt < 500) return // evita bucles de eventos
        lastShieldAt = now
        performGlobalAction(GLOBAL_ACTION_HOME)
        startActivity(
            Intent(this, ShieldActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP)
                .putExtra(ShieldActivity.EXTRA_PACKAGE, pkg),
        )
    }

    override fun onInterrupt() = Unit

    override fun onUnbind(intent: Intent?): Boolean {
        // El usuario desactivó la Accesibilidad: queda registrado y se sincroniza.
        runCatching { CoreHolder.get(this).reportTamper(CoreHolder.clock(), TamperKind.PERMISSION_REVOKED) }
        SyncScheduler.syncNow(this)
        return super.onUnbind(intent)
    }

    override fun onDestroy() {
        runCatching { unregisterReceiver(unlockReceiver) }
        scope.cancel()
        super.onDestroy()
    }
}
