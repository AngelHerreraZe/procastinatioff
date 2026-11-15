package dev.procastinacioff.app.sync

import android.content.ComponentName
import android.content.Context
import android.provider.Settings
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import dev.procastinacioff.app.block.BlockerService
import dev.procastinacioff.app.core.CoreHolder
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.procastinacioff_core.CoreException
import uniffi.procastinacioff_core.TamperKind

class SyncWorker(ctx: Context, params: WorkerParameters) : CoroutineWorker(ctx, params) {
    override suspend fun doWork(): Result = withContext(Dispatchers.IO) {
        val ctx = applicationContext
        val core = CoreHolder.get(ctx)
        CoreHolder.anchor(core) // hora firmada del relay antes de nada
        checkAccessibility(ctx)
        val clock = CoreHolder.clock()
        val result = try {
            core.sync(clock)
            Result.success()
        } catch (e: CoreException.Network) {
            Result.retry() // sin conexión: WorkManager reintenta con espera exponencial
        } catch (e: CoreException) {
            Result.success() // sin relay configurado: nada que sincronizar
        }
        // Mi compañero: solo manipulaciones, emergencias y el resumen de cada día (números)
        CoreHolder.partnerBoxOrNull(ctx)?.let { box ->
            runCatching { core.forwardToPartner(box, clock); box.sync(clock) }
        }
        // Teléfono de un menor: lista de categorías (a diario) y protección activa
        if (runCatching { core.myRole() }.getOrNull() == uniffi.procastinacioff_core.Role.MINOR) {
            val prefs = ctx.getSharedPreferences("family", Context.MODE_PRIVATE)
            if (System.currentTimeMillis() - prefs.getLong("lists_at", 0) > 24 * 3_600_000L) {
                if (runCatching { core.refreshCategoryList() }.isSuccess) prefs.edit().putLong("lists_at", System.currentTimeMillis()).apply()
            }
            runCatching { dev.procastinacioff.app.family.DeviceOwnerPolicies.startMinorProtection(ctx) }
            // Filtro web sin activar (o quitado en favor de otra VPN): se registra UNA vez; el tutor lo ve
            val vpnOff = android.net.VpnService.prepare(ctx) != null
            if (vpnOff && !prefs.getBoolean("vpn_reported", false)) {
                runCatching { core.reportTamper(clock, TamperKind.PERMISSION_REVOKED) }
                prefs.edit().putBoolean("vpn_reported", true).apply()
            } else if (!vpnOff) {
                prefs.edit().putBoolean("vpn_reported", false).apply()
            }
        }
        // Tutor: perfiles de los menores; aviso si llegó algo nuevo y hay solicitudes esperando
        CoreHolder.familyProfiles(ctx).keys.forEach { id ->
            val box = CoreHolder.family(ctx, id)
            val report = runCatching { box.sync(clock) }.getOrNull()
            if (report != null && report.received > 0u && box.pendingRequests().isNotEmpty()) LocalNotifications.showFamilyRequest(ctx)
        }
        // La persona a la que acompaño: traer sus avisos y avisar si hay nuevos
        CoreHolder.watchingBoxOrNull(ctx)?.let { box ->
            val report = runCatching { box.sync(clock) }.getOrNull()
            if (report != null && report.received > 0u) LocalNotifications.showPartnerAlert(ctx)
        }
        result
    }

    /** Si la Accesibilidad está desactivada con un perfil activo, se registra UNA vez. */
    private fun checkAccessibility(ctx: Context) {
        val core = CoreHolder.get(ctx)
        if (core.status(CoreHolder.clock()) == null) return
        val prefs = ctx.getSharedPreferences("tamper", Context.MODE_PRIVATE)
        val enabled = isAccessibilityEnabled(ctx)
        if (!enabled && !prefs.getBoolean("a11y_reported", false)) {
            runCatching { core.reportTamper(CoreHolder.clock(), TamperKind.PERMISSION_REVOKED) }
            prefs.edit().putBoolean("a11y_reported", true).apply()
        } else if (enabled) {
            prefs.edit().putBoolean("a11y_reported", false).apply()
        }
    }
}

fun isAccessibilityEnabled(ctx: Context): Boolean {
    val enabled = Settings.Secure.getString(ctx.contentResolver, Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES)
        ?: return false
    val me = ComponentName(ctx, BlockerService::class.java).flattenToString()
    return enabled.split(':').any { it.equals(me, ignoreCase = true) }
}

object SyncScheduler {
    private val online = Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build()

    fun schedulePeriodic(ctx: Context) {
        WorkManager.getInstance(ctx).enqueueUniquePeriodicWork(
            "sync-periodic",
            ExistingPeriodicWorkPolicy.KEEP,
            PeriodicWorkRequestBuilder<SyncWorker>(15, TimeUnit.MINUTES).setConstraints(online).build(),
        )
    }

    fun syncNow(ctx: Context) {
        WorkManager.getInstance(ctx).enqueueUniqueWork(
            "sync-now",
            ExistingWorkPolicy.REPLACE,
            OneTimeWorkRequestBuilder<SyncWorker>().setConstraints(online).build(),
        )
    }
}
