package dev.procastinacioff.app.sync

import android.content.Context
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import dev.procastinacioff.app.core.CoreHolder

/** FCM only carries {"t":"sync"}: the app syncs, decrypts on the phone and creates its own notification. (Removed when sync goes serverless.) */
class PushService : FirebaseMessagingService() {
    override fun onNewToken(token: String) {
        // If there is no relay yet, it is saved and registered when connecting (see onboarding).
        getSharedPreferences("push", Context.MODE_PRIVATE).edit().putString("fcm", token).apply()
        runCatching { CoreHolder.get(this).registerPush("fcm", token) }
    }

    override fun onMessageReceived(message: RemoteMessage) {
        // Runs on a background thread: it can sync directly.
        val report = runCatching { CoreHolder.get(this).sync(CoreHolder.clock()) }.getOrNull() ?: return
        if (message.priority == RemoteMessage.PRIORITY_HIGH && report.received > 0u) {
            LocalNotifications.showGeneric(this)
        }
    }
}
