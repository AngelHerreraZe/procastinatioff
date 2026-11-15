package dev.procastinacioff.app.block

import android.app.NotificationManager
import android.content.Context

/**
 * DORMIR: solo llamadas de cualquiera. Al despertar se restaura el filtro previo del usuario.
 * ⚠️ Android 15 + targetSdk 35: setInterruptionFilter crea una AutomaticZenRule a nombre de la app;
 * verificar en Pixel y Samsung (spike B).
 */
object DndController {
    private const val PREFS = "dnd"

    fun apply(context: Context, sleeping: Boolean) {
        val nm = context.getSystemService(NotificationManager::class.java)
        if (!nm.isNotificationPolicyAccessGranted) return // el onboarding pide este permiso
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        if (sleeping) {
            if (!prefs.contains("previous")) prefs.edit().putInt("previous", nm.currentInterruptionFilter).apply()
            nm.notificationPolicy = NotificationManager.Policy(
                NotificationManager.Policy.PRIORITY_CATEGORY_CALLS,
                NotificationManager.Policy.PRIORITY_SENDERS_ANY,
                NotificationManager.Policy.PRIORITY_SENDERS_ANY,
            )
            nm.setInterruptionFilter(NotificationManager.INTERRUPTION_FILTER_PRIORITY)
        } else if (prefs.contains("previous")) {
            nm.setInterruptionFilter(prefs.getInt("previous", NotificationManager.INTERRUPTION_FILTER_ALL))
            prefs.edit().remove("previous").apply()
        }
    }
}
