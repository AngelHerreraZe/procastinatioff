package dev.procastinacioff.app.sync

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import android.content.pm.PackageManager
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat

/** Notificaciones locales. Fase 1: texto fijo, sin datos del evento. */
object LocalNotifications {
    private const val CHANNEL = "novedades"

    fun createChannel(context: Context) {
        val nm = context.getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel(CHANNEL, "Novedades", NotificationManager.IMPORTANCE_DEFAULT))
    }

    /** Avisos de la persona a la que acompañas: texto fijo; el detalle solo dentro de la app. */
    fun showPartnerAlert(context: Context) = show(context, 2, "Hay avisos de la persona a la que acompañas")

    /** Tutor: texto fijo; qué se pide y quién, solo dentro de la app. */
    fun showFamilyRequest(context: Context) = show(context, 3, "Tienes solicitudes de tu familia")

    fun showGeneric(context: Context) = show(context, 1, "Hay novedades de tu otro dispositivo")

    private fun show(context: Context, id: Int, text: String) {
        val granted = ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) ==
            PackageManager.PERMISSION_GRANTED
        if (!granted) return
        val n = NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(android.R.drawable.ic_lock_idle_lock)
            .setContentTitle("ProcastinaciOFF")
            .setContentText(text)
            .setAutoCancel(true)
            .build()
        NotificationManagerCompat.from(context).notify(id, n)
    }
}
