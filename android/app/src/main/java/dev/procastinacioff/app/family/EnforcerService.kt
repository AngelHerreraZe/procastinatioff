package dev.procastinacioff.app.family

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.app.admin.DevicePolicyManager
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import dev.procastinacioff.app.block.AppIdentity
import dev.procastinacioff.app.block.Policy
import dev.procastinacioff.app.block.Verdict
import dev.procastinacioff.app.core.CoreHolder
import dev.procastinacioff.app.ui.launchableApps
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import uniffi.procastinacioff_core.DayState

/**
 * 🔒 Con Device Owner no hace falta Accesibilidad: lo no permitido se SUSPENDE a nivel de sistema
 * ("App pausada por el administrador"). Revisa cada 15 s y solo cambia lo que cambió.
 */
class EnforcerService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    @Volatile private var running = false

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel("enforcer", "Protección activa", NotificationManager.IMPORTANCE_MIN))
        val n = Notification.Builder(this, "enforcer")
            .setContentTitle("ProcastinaciOFF activo")
            .setSmallIcon(android.R.drawable.ic_lock_idle_lock)
            .build()
        if (Build.VERSION.SDK_INT >= 34) startForeground(1, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_SYSTEM_EXEMPTED)
        else startForeground(1, n)
        if (!running) {
            running = true
            scope.launch { loop() }
        }
        return START_STICKY
    }

    private suspend fun loop() {
        val dpm = getSystemService(DevicePolicyManager::class.java)
        val admin = DeviceOwnerPolicies.admin(this)
        var suspended = emptySet<String>()
        var chrome: List<String>? = listOf("—") // fuerza la primera escritura
        while (scope.isActive) {
            runCatching {
                val core = CoreHolder.get(this)
                val clock = CoreHolder.clock()
                val status = core.status(clock) ?: return@runCatching
                val cfg = core.activeConfig(clock)
                val system = AppIdentity.systemAllowed(this, sleeping = status.state is DayState.Sleep)
                val toSuspend = launchableApps(this)
                    .filter { it.pkg !in system && Policy.decide(status.state, it.pkg, it.id, cfg, system) == Verdict.BLOCK }
                    .map { it.pkg }
                    .toSet()
                if (toSuspend != suspended) {
                    dpm.setPackagesSuspended(admin, (suspended - toSuspend).toTypedArray(), false)
                    dpm.setPackagesSuspended(admin, toSuspend.toTypedArray(), true) // devuelve los que no se pudieron
                    suspended = toSuspend
                }
                val want = (status.state as? DayState.Focus)?.let { cfg?.webAllowlist }
                if (want != chrome) {
                    DeviceOwnerPolicies.applyChromePolicy(this, want)
                    chrome = want
                }
            }
            delay(15_000)
        }
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }
}
