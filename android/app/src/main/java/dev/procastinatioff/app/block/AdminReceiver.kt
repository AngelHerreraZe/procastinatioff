package dev.procastinatioff.app.block

import dev.procastinatioff.app.R
import android.app.admin.DeviceAdminReceiver
import android.content.Context
import android.content.Intent
import dev.procastinatioff.app.core.CoreHolder
import dev.procastinatioff.app.sync.SyncScheduler
import uniffi.procastinatioff_core.TamperKind

/** Device Admin: prevents uninstalling on an impulse; disabling it is recorded and synced. */
class AdminReceiver : DeviceAdminReceiver() {
    override fun onDisableRequested(context: Context, intent: Intent): CharSequence =
        context.getString(R.string.admin_disable_warning)

    override fun onDisabled(context: Context, intent: Intent) {
        runCatching { CoreHolder.get(context).reportTamper(CoreHolder.clock(), TamperKind.UNINSTALL_ATTEMPT) }
        SyncScheduler.syncNow(context)
    }
}
