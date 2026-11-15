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
        CoreHolder.anchor(core) // relay signed time before anything else
        checkAccessibility(ctx)
        val clock = CoreHolder.clock()
        val result = try {
            core.sync(clock)
            Result.success()
        } catch (e: CoreException.Network) {
            Result.retry() // offline: WorkManager retries with exponential backoff
        } catch (e: CoreException) {
            Result.success() // no relay configured: nothing to sync
        }
        // My partner: only tampering, emergencies and each day's summary (numbers)
        CoreHolder.partnerBoxOrNull(ctx)?.let { box ->
            runCatching { core.forwardToPartner(box, clock); box.sync(clock) }
        }
        // A minor's phone: category list (daily) and active protection
        if (runCatching { core.myRole() }.getOrNull() == uniffi.procastinacioff_core.Role.MINOR) {
            val prefs = ctx.getSharedPreferences("family", Context.MODE_PRIVATE)
            if (System.currentTimeMillis() - prefs.getLong("lists_at", 0) > 24 * 3_600_000L) {
                if (runCatching { core.refreshCategoryList() }.isSuccess) prefs.edit().putLong("lists_at", System.currentTimeMillis()).apply()
            }
            runCatching { dev.procastinacioff.app.family.DeviceOwnerPolicies.startMinorProtection(ctx) }
            // Web filter not enabled (or removed in favor of another VPN): recorded ONCE; the guardian sees it
            val vpnOff = android.net.VpnService.prepare(ctx) != null
            if (vpnOff && !prefs.getBoolean("vpn_reported", false)) {
                runCatching { core.reportTamper(clock, TamperKind.PERMISSION_REVOKED) }
                prefs.edit().putBoolean("vpn_reported", true).apply()
            } else if (!vpnOff) {
                prefs.edit().putBoolean("vpn_reported", false).apply()
            }
        }
        // Guardian: the minors' profiles; notify if something new arrived and requests are waiting
        CoreHolder.familyProfiles(ctx).keys.forEach { id ->
            val box = CoreHolder.family(ctx, id)
            val report = runCatching { box.sync(clock) }.getOrNull()
            if (report != null && report.received > 0u && box.pendingRequests().isNotEmpty()) LocalNotifications.showFamilyRequest(ctx)
        }
        // The person I support: fetch their alerts and notify if there are new ones
        CoreHolder.watchingBoxOrNull(ctx)?.let { box ->
            val report = runCatching { box.sync(clock) }.getOrNull()
            if (report != null && report.received > 0u) LocalNotifications.showPartnerAlert(ctx)
        }
        result
    }

    /** If Accessibility is off with an active profile, it is recorded ONCE. */
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
