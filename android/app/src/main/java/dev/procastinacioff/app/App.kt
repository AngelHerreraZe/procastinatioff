package dev.procastinacioff.app

import android.app.Application
import dev.procastinacioff.app.family.DeviceOwnerPolicies
import dev.procastinacioff.app.sync.LocalNotifications
import dev.procastinacioff.app.sync.SyncScheduler

class App : Application() {
    override fun onCreate() {
        super.onCreate()
        LocalNotifications.createChannel(this)
        SyncScheduler.schedulePeriodic(this)
        // Teléfono de un menor: políticas, suspensión de apps y filtro DNS (fuera del hilo principal)
        Thread { runCatching { DeviceOwnerPolicies.startMinorProtection(this) } }.start()
    }
}
