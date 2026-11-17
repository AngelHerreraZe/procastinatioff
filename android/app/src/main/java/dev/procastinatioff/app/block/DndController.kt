package dev.procastinatioff.app.block

import android.app.NotificationManager
import android.content.Context

/**
 * SLEEP: only notifications go quiet. Calls from anyone, alarms, media and system sounds keep their
 * sound (the app never touches the volume). On waking up the user's previous filter is restored.
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
            // Without MEDIA/ALARMS/SYSTEM, priority mode mutes music and even the wake-up alarm.
            nm.notificationPolicy = NotificationManager.Policy(
                NotificationManager.Policy.PRIORITY_CATEGORY_CALLS or
                    NotificationManager.Policy.PRIORITY_CATEGORY_MEDIA or
                    NotificationManager.Policy.PRIORITY_CATEGORY_ALARMS or
                    NotificationManager.Policy.PRIORITY_CATEGORY_SYSTEM,
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
