package dev.procastinatioff.app.sync

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.os.Build
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import uniffi.procastinatioff_core.Core

/**
 * Finds the profile's devices on the home network (DNS-SD `_pco._udp`) and tells each core their
 * address. The core recognizes its devices by the rotating tag of the day: it ignores the rest.
 * Nothing is announced from here: a phone only listens while showing a QR.
 */
object LanDiscovery {
    private const val SERVICE = "_pco._udp."

    /** Browses for `seconds` (blocking: call from a background thread). */
    fun browse(ctx: Context, cores: List<Core>, nowMs: Long, seconds: Long = 4) {
        if (cores.isEmpty()) return
        val nsd = ctx.getSystemService(NsdManager::class.java) ?: return
        val pool = Executors.newSingleThreadExecutor()
        val stopped = CountDownLatch(1)
        val listener = object : NsdManager.DiscoveryListener {
            override fun onServiceFound(info: NsdServiceInfo) {
                if (info.serviceType.startsWith("_pco._udp")) resolve(nsd, info, pool) { tag, host, port ->
                    cores.forEach { runCatching { it.lanPeerSeen(nowMs, tag, host, port.toUShort()) } }
                }
            }
            override fun onDiscoveryStarted(serviceType: String) {}
            override fun onDiscoveryStopped(serviceType: String) { stopped.countDown() }
            override fun onServiceLost(info: NsdServiceInfo) {}
            override fun onStartDiscoveryFailed(serviceType: String, errorCode: Int) { stopped.countDown() }
            override fun onStopDiscoveryFailed(serviceType: String, errorCode: Int) { stopped.countDown() }
        }
        runCatching { nsd.discoverServices(SERVICE, NsdManager.PROTOCOL_DNS_SD, listener) }.onFailure { return }
        Thread.sleep(seconds * 1000)
        runCatching { nsd.stopServiceDiscovery(listener) }
        stopped.await(2, TimeUnit.SECONDS)
        pool.shutdown()
    }

    @Suppress("DEPRECATION") // resolveService: the callback API replacing it needs Android 14+
    private fun resolve(nsd: NsdManager, info: NsdServiceInfo, pool: java.util.concurrent.Executor, found: (String, String, Int) -> Unit) {
        // Android before 14 resolves one service at a time: serialize.
        pool.execute {
            val done = CountDownLatch(1)
            nsd.resolveService(info, object : NsdManager.ResolveListener {
                override fun onServiceResolved(r: NsdServiceInfo) {
                    val host = if (Build.VERSION.SDK_INT >= 34) r.hostAddresses.firstOrNull() else r.host
                    host?.hostAddress?.let { found(r.serviceName, it, r.port) }
                    done.countDown()
                }
                override fun onResolveFailed(r: NsdServiceInfo, errorCode: Int) { done.countDown() }
            })
            done.await(3, TimeUnit.SECONDS)
        }
    }
}
