package dev.procastinacioff.app.sync

import dev.procastinacioff.app.R
import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import android.content.pm.PackageManager
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat

/** Local notifications: fixed, generic text without event data (details only inside the app). */
object LocalNotifications {
    private const val CHANNEL = "novedades" // channel id: never rename (Android keeps the user's settings per id)

    fun createChannel(context: Context) {
        val nm = context.getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel(CHANNEL, context.getString(R.string.channel_updates), NotificationManager.IMPORTANCE_DEFAULT))
    }

    /** Alerts from the person you support: fixed text; the details only inside the app. */
    fun showPartnerAlert(context: Context) = show(context, 2, R.string.notify_partner_alert)

    /** Guardian: fixed text; what is asked and by whom, only inside the app. */
    fun showFamilyRequest(context: Context) = show(context, 3, R.string.notify_family_request)

    fun showGeneric(context: Context) = show(context, 1, R.string.notify_generic)

    private fun show(context: Context, id: Int, text: Int) {
        val granted = ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) ==
            PackageManager.PERMISSION_GRANTED
        if (!granted) return
        val n = NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(android.R.drawable.ic_lock_idle_lock)
            .setContentTitle("ProcastinaciOFF")
            .setContentText(context.getString(text))
            .setAutoCancel(true)
            .build()
        NotificationManagerCompat.from(context).notify(id, n)
    }
}
