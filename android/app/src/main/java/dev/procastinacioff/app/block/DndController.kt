package dev.procastinacioff.app.block

import android.app.NotificationManager
import android.content.Context

/**
 * SLEEP: only calls from anyone. On waking up the user's previous filter is restored.
 * VERIFY: Android 15 + targetSdk 35: setInterruptionFilter creates an AutomaticZenRule in the app's name;
 * check on Pixel and Samsung (spike B).
 */
object DndController {
    private const val PREFS = "dnd"

    fun apply(context: Context, sleeping: Boolean) {
        val nm = context.getSystemService(NotificationManager::class.java)
        if (!nm.isNotificationPolicyAccessGranted) return // onboarding asks for this permission
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
