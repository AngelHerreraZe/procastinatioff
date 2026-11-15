package dev.procastinacioff.app.family

import android.content.Intent
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.net.VpnService
import android.os.ParcelFileDescriptor
import dev.procastinacioff.app.core.CoreHolder
import java.io.FileInputStream
import java.io.FileOutputStream
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress
import java.util.concurrent.Executors
import uniffi.procastinacioff_core.DnsAction

/**
 * 🔒 VPN local SOLO para el DNS (07b · 3): únicamente la IP falsa del DNS (10.111.0.1) entra al túnel;
 * el resto del tráfico sale por la red normal. No se reimplementa TCP/IP ni se toca el contenido de
 * ninguna conexión. QUÉ se responde lo decide el núcleo (`dnsStep`/`dnsFinish`).
 * 🔒 No se registra ningún nombre consultado.
 */
class DnsVpnService : VpnService() {
    @Volatile private var running = false
    private var tun: ParcelFileDescriptor? = null
    private val pool = Executors.newFixedThreadPool(4)

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (running) return START_STICKY
        val fd = Builder()
            .setSession("ProcastinaciOFF · filtro")
            .addAddress("10.111.0.2", 32)
            .addDnsServer("10.111.0.1")
            .addRoute("10.111.0.1", 32) // SOLO el DNS entra al túnel
            .setBlocking(true)
            .establish() ?: return START_NOT_STICKY
        tun = fd
        running = true
        Thread { loop(fd) }.start()
        return START_STICKY
    }

    /** DNS de la red física (no de la VPN), para reenviar las consultas permitidas. */
    private fun upstream(): InetAddress {
        val cm = getSystemService(ConnectivityManager::class.java)
        @Suppress("DEPRECATION")
        val net = cm.allNetworks.firstOrNull { n ->
            cm.getNetworkCapabilities(n)?.let {
                it.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) && !it.hasTransport(NetworkCapabilities.TRANSPORT_VPN)
            } == true
        }
        // ponytail: si la red no anuncia DNS IPv4 se usa Quad9 (sin registro, sin filtrado propio).
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
            protect(s) // este socket NO pasa por la VPN
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
        // Sin Device Owner, el usuario puede quitar la VPN: queda registrado (el tutor lo ve).
        runCatching {
            CoreHolder.get(this).reportTamper(CoreHolder.clock(), uniffi.procastinacioff_core.TamperKind.PERMISSION_REVOKED)
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
        /** Cabecera IPv4 (20 bytes) + UDP (8 bytes, checksum 0 permitido en IPv4) + datos. */
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
