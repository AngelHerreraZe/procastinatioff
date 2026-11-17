package dev.procastinatioff.app.family

import dev.procastinatioff.app.R
import android.content.Intent
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.net.VpnService
import android.os.ParcelFileDescriptor
import dev.procastinatioff.app.core.CoreHolder
import java.io.FileInputStream
import java.io.FileOutputStream
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.util.concurrent.Executors
import uniffi.procastinatioff_core.DnsAction

/**
 * SECURITY: Local VPN ONLY for DNS: only the fake DNS IP (10.111.0.1) enters the tunnel;
 * the rest of the traffic goes through the normal network. TCP/IP is not reimplemented and the content of
 * no connection is touched. WHAT gets answered is decided by the core (`dnsStep`/`dnsFinish`).
 * SECURITY: No queried name is logged.
 */
class DnsVpnService : VpnService() {
    @Volatile private var running = false
    private var tun: ParcelFileDescriptor? = null
    private val pool = Executors.newFixedThreadPool(4)

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (running) return START_STICKY
        val fd = Builder()
            .setSession(getString(R.string.vpn_session))
            .addAddress("10.111.0.2", 32)
            .addDnsServer("10.111.0.1")
            .addRoute("10.111.0.1", 32) // ONLY DNS enters the tunnel
            .setBlocking(true)
            .establish() ?: return START_NOT_STICKY
        tun = fd
        running = true
        Thread { loop(fd) }.start()
        return START_STICKY
    }

    /** DNS of the physical network (not the VPN's), to forward allowed queries. */
    private fun upstream(): InetAddress {
        val cm = getSystemService(ConnectivityManager::class.java)
        @Suppress("DEPRECATION")
        val net = cm.allNetworks.firstOrNull { n ->
            cm.getNetworkCapabilities(n)?.let {
                it.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) && !it.hasTransport(NetworkCapabilities.TRANSPORT_VPN)
            } == true
        }
        // ponytail: if the network announces no IPv4 DNS, Quad9 is used (no logging, no filtering of its own).
        return net?.let { cm.getLinkProperties(it)?.dnsServers?.firstOrNull { a -> a.address.size == 4 } }
            ?: InetAddress.getByName("9.9.9.9")
    }

    private fun loop(fd: ParcelFileDescriptor) {
        val input = FileInputStream(fd.fileDescriptor)
        val output = FileOutputStream(fd.fileDescriptor)
        val buf = ByteArray(32_767)
        while (running) {
            val n = try { input.read(buf) } catch (e: Exception) { break }
            if (n < 28) continue
            if ((buf[0].toInt() shr 4) != 4 || buf[9].toInt() != 17) continue // IPv4 + UDP
            val ihl = (buf[0].toInt() and 0x0F) * 4
            if (n < ihl + 8) continue
            val dstPort = ((buf[ihl + 2].toInt() and 0xFF) shl 8) or (buf[ihl + 3].toInt() and 0xFF)
            if (dstPort != 53) continue
            val packet = buf.copyOf(n)
            pool.execute { runCatching { handle(packet, ihl, output) } }
        }
    }

    private fun handle(p: ByteArray, ihl: Int, output: FileOutputStream) {
        val query = p.copyOfRange(ihl + 8, p.size)
        val core = CoreHolder.get(this)
        val reply: ByteArray = when (val a = core.dnsStep(CoreHolder.clock(), query)) {
            is DnsAction.Reply -> a.packet
            is DnsAction.Upstream -> {
                val r = forward(a.packet) ?: return
                core.dnsFinish(query, r, a.rewritten) ?: return
            }
            is DnsAction.Drop -> return
        }
        val out = ipv4Udp(
            src = p.copyOfRange(16, 20), dst = p.copyOfRange(12, 16),
            srcPort = 53, dstPort = ((p[ihl].toInt() and 0xFF) shl 8) or (p[ihl + 1].toInt() and 0xFF), payload = reply,
        )
        synchronized(output) { output.write(out) }
    }

    private fun forward(packet: ByteArray): ByteArray? = try {
        DatagramSocket().use { s ->
            protect(s) // this socket does NOT go through the VPN
            s.soTimeout = 3_000
            s.send(DatagramPacket(packet, packet.size, upstream(), 53))
            val resp = DatagramPacket(ByteArray(4096), 4096)
            s.receive(resp)
            resp.data.copyOf(resp.length)
        }
    } catch (e: Exception) {
        null
    }

    override fun onRevoke() {
        // Without Device Owner, the user can remove the VPN: it is recorded (the guardian sees it).
        runCatching {
            CoreHolder.get(this).reportTamper(CoreHolder.clock(), uniffi.procastinatioff_core.TamperKind.PERMISSION_REVOKED)
        }
        super.onRevoke()
    }

    override fun onDestroy() {
        running = false
        pool.shutdownNow()
        tun?.close()
        super.onDestroy()
    }

    companion object {
        /** IPv4 header (20 bytes) + UDP (8 bytes, checksum 0 allowed on IPv4) + data. */
        fun ipv4Udp(src: ByteArray, dst: ByteArray, srcPort: Int, dstPort: Int, payload: ByteArray): ByteArray {
            val total = 28 + payload.size
            val b = ByteArray(total)
            b[0] = 0x45; b[2] = (total shr 8).toByte(); b[3] = total.toByte(); b[8] = 64; b[9] = 17
            src.copyInto(b, 12); dst.copyInto(b, 16)
            var sum = 0
            for (i in 0 until 20 step 2) sum += ((b[i].toInt() and 0xFF) shl 8) or (b[i + 1].toInt() and 0xFF)
            while (sum shr 16 != 0) sum = (sum and 0xFFFF) + (sum shr 16)
            val cs = sum.inv() and 0xFFFF
            b[10] = (cs shr 8).toByte(); b[11] = cs.toByte()
            b[20] = (srcPort shr 8).toByte(); b[21] = srcPort.toByte()
            b[22] = (dstPort shr 8).toByte(); b[23] = dstPort.toByte()
            val udpLen = 8 + payload.size
            b[24] = (udpLen shr 8).toByte(); b[25] = udpLen.toByte()
            payload.copyInto(b, 28)
            return b
        }
    }
}
