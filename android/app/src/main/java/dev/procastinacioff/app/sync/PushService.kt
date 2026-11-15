package dev.procastinacioff.app.sync

import android.content.Context
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import dev.procastinacioff.app.core.CoreHolder

/** FCM solo trae {"t":"sync"}: la app sincroniza, descifra en el teléfono y crea su propia notificación. */
class PushService : FirebaseMessagingService() {
    override fun onNewToken(token: String) {
        // Si aún no hay relay, se guarda y se registra al conectar (ver Onboarding).
        getSharedPreferences("push", Context.MODE_PRIVATE).edit().putString("fcm", token).apply()
        runCatching { CoreHolder.get(this).registerPush("fcm", token) }
    }

    override fun onMessageReceived(message: RemoteMessage) {
        // Corre en un hilo de fondo: se puede sincronizar directamente.
        val report = runCatching { CoreHolder.get(this).sync(CoreHolder.clock()) }.getOrNull() ?: return
        if (message.priority == RemoteMessage.PRIORITY_HIGH && report.received > 0u) {
            LocalNotifications.showGeneric(this)
        }
    }
}
