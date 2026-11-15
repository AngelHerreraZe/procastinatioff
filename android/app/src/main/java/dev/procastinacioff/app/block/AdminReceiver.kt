package dev.procastinacioff.app.block

import android.app.admin.DeviceAdminReceiver
import android.content.Context
import android.content.Intent
import dev.procastinacioff.app.core.CoreHolder
import dev.procastinacioff.app.sync.SyncScheduler
import uniffi.procastinacioff_core.TamperKind

/** Device Admin: impide desinstalar en un impulso; desactivarlo queda registrado y se sincroniza. */
class AdminReceiver : DeviceAdminReceiver() {
    override fun onDisableRequested(context: Context, intent: Intent): CharSequence =
        "Si desactivas la protección, se registrará y tus otros dispositivos lo sabrán."

    override fun onDisabled(context: Context, intent: Intent) {
        runCatching { CoreHolder.get(context).reportTamper(CoreHolder.clock(), TamperKind.UNINSTALL_ATTEMPT) }
        SyncScheduler.syncNow(context)
    }
}
