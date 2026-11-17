package dev.procastinatioff.app

import android.app.Application
import dev.procastinatioff.app.family.DeviceOwnerPolicies
import dev.procastinatioff.app.sync.LocalNotifications
import dev.procastinatioff.app.sync.SyncScheduler

class App : Application() {
    override fun onCreate() {
        super.onCreate()
        LocalNotifications.createChannel(this)
        SyncScheduler.schedulePeriodic(this)
        // A minor's phone: policies, app suspension and DNS filter (off the main thread)
        Thread { runCatching { DeviceOwnerPolicies.startMinorProtection(this) } }.start()
    }
}
