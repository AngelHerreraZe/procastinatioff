package dev.procastinatioff.app.sync

import dev.procastinatioff.app.R
import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import dev.procastinatioff.app.MainActivity
import android.content.pm.PackageManager
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import uniffi.procastinatioff_core.Notice
import uniffi.procastinatioff_core.NoticeKind

/** Local notifications: fixed, generic text without event data (details only inside the app). */
object LocalNotifications {
    private const val CHANNEL = "novedades" // channel id: never rename (Android keeps the user's settings per id)
    private const val CHEERS = "metas" // same rule
    private const val CHEER_ID = 9

    fun createChannel(context: Context) {
        val nm = context.getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel(CHANNEL, context.getString(R.string.channel_updates), NotificationManager.IMPORTANCE_DEFAULT))
        // HIGH: drops down over the home screen the blocked app was sent to. Silent: the sound is never touched.
        nm.createNotificationChannel(
            NotificationChannel(CHEERS, context.getString(R.string.channel_goal_reminders), NotificationManager.IMPORTANCE_HIGH).apply {
                setSound(null, null)
                enableVibration(false)
            },
        )
    }

    /**
     * A blocked app during the day's goals: a short positive reminder (generic text: the goal's name
     * stays inside the app). Replaces the previous one; tapping it goes back to the focus, if any.
     */
    fun cheer(context: Context, focusGoalId: String?) {
        val open = focusGoalId?.let { MainActivity.intent(context, MainActivity.ROUTE_FOCUS, it) }
            ?: Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        post(
            context, CHEER_ID,
            NotificationCompat.Builder(context, CHEERS)
                .setContentTitle(context.getString(R.string.cheer_title))
                .setContentText(context.resources.getStringArray(R.array.cheers).random())
                .setContentIntent(PendingIntent.getActivity(context, CHEER_ID, open, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT))
                .setTimeoutAfter(60_000),
        )
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

    private fun show(context: Context, id: Int, text: Int) = post(
        context, id,
        NotificationCompat.Builder(context, CHANNEL).setContentTitle("ProcastinatiOFF").setContentText(context.getString(text)),
    )

    private fun post(context: Context, id: Int, builder: NotificationCompat.Builder) {
        val granted = ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) ==
            PackageManager.PERMISSION_GRANTED
        if (!granted) return
        val n = builder
            .setSmallIcon(R.drawable.ic_stat_logo)
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE)
            .setAutoCancel(true)
            .build()
        NotificationManagerCompat.from(context).notify(id, n)
    }
}
