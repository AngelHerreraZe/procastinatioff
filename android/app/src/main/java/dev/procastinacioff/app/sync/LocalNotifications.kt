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
import uniffi.procastinacioff_core.Notice
import uniffi.procastinacioff_core.NoticeKind

/** Local notifications: fixed, generic text without event data (details only inside the app). */
object LocalNotifications {
    private const val CHANNEL = "novedades" // channel id: never rename (Android keeps the user's settings per id)

    fun createChannel(context: Context) {
        val nm = context.getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel(CHANNEL, context.getString(R.string.channel_updates), NotificationManager.IMPORTANCE_DEFAULT))
    }

    /**
     * Notices the core produced after a sync. One notification per kind (a new one replaces the
     * previous). `fromWatching`: they come from the person I support (their contact channel).
     */
    fun showNotices(context: Context, notices: List<Notice>, fromWatching: Boolean) {
        notices.map { it.kind }.distinct().forEach { kind ->
            when (kind) {
                is NoticeKind.RequestReceived -> show(context, 3, R.string.notify_family_request)
                is NoticeKind.RequestAnswered -> show(context, 4, R.string.notify_request_answered)
                is NoticeKind.TamperAlert, is NoticeKind.EmergencyAlert ->
                    if (fromWatching) show(context, 2, R.string.notify_contact_alert) else show(context, 5, R.string.notify_family_alert)
                is NoticeKind.DeviceSilent -> show(context, 6, R.string.notify_device_silent)
                is NoticeKind.LocalResetStarted -> show(context, 7, R.string.notify_local_reset)
                is NoticeKind.ContactExitScheduled -> show(context, 8, R.string.notify_contact_exit)
            }
        }
    }

    private fun show(context: Context, id: Int, text: Int) {
        val granted = ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) ==
            PackageManager.PERMISSION_GRANTED
        if (!granted) return
        val n = NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.ic_stat_logo)
            .setContentTitle("ProcastinaciOFF")
            .setContentText(context.getString(text))
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
            .setAutoCancel(true)
            .build()
        NotificationManagerCompat.from(context).notify(id, n)
    }
}
