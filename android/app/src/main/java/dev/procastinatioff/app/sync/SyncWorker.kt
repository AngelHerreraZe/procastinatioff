package dev.procastinatioff.app.sync

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
import dev.procastinatioff.app.block.BlockerService
import dev.procastinatioff.app.core.CoreHolder
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.procastinatioff_core.TamperKind

class SyncWorker(ctx: Context, params: WorkerParameters) : CoroutineWorker(ctx, params) {
    override suspend fun doWork(): Result = withContext(Dispatchers.IO) {
        val ctx = applicationContext
        val core = CoreHolder.get(ctx)
        checkAccessibility(ctx)
        val clock = CoreHolder.clock()
        // My trusted contact: approved changes come back; only alerts and numbers go out
        val contact = CoreHolder.contactChannelOrNull(ctx)?.also { box -> runCatching { core.forwardToContact(box, clock) } }
        val watching = CoreHolder.watchingBoxOrNull(ctx)
        val families = CoreHolder.familyProfiles(ctx).keys.map { CoreHolder.family(ctx, it) }
        val cores = listOfNotNull(core, contact, watching) + families
        // Who of the profiles' devices is on this network now; then sync directly with them.
        // Devices that are not here are normal (outside home): every device blocks with its own copy.
        LanDiscovery.browse(ctx, cores, clock.nowMs)
        for (c in cores) {
            runCatching { CoreHolder.syncNow(c) }
            runCatching { c.takeNotices() }.getOrNull()?.let { LocalNotifications.showNotices(ctx, it, fromWatching = c === watching) }
        }
        // What the contact answered during this sync is applied now, not in 15 minutes.
        contact?.let { box -> runCatching { core.forwardToContact(box, clock) } }
        // A minor's phone: category list (bundled in the app) and active protection
        if (runCatching { core.myRole() }.getOrNull() == uniffi.procastinatioff_core.Role.MINOR) {
            val prefs = ctx.getSharedPreferences("family", Context.MODE_PRIVATE)
            CoreHolder.loadBundledList(ctx, core)
            runCatching { dev.procastinatioff.app.family.DeviceOwnerPolicies.startMinorProtection(ctx) }
            // Web filter not enabled (or removed in favor of another VPN): recorded ONCE; the guardian sees it
            val vpnOff = android.net.VpnService.prepare(ctx) != null
            if (vpnOff && !prefs.getBoolean("vpn_reported", false)) {
                runCatching { core.reportTamper(clock, TamperKind.PERMISSION_REVOKED) }
                prefs.edit().putBoolean("vpn_reported", true).apply()
            } else if (!vpnOff) {
                prefs.edit().putBoolean("vpn_reported", false).apply()
            }
        }
        Result.success()
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
