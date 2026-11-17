package dev.procastinatioff.app.family

import org.junit.Assert.assertEquals
import org.junit.Test

/** The IPv4/UDP header the VPN writes by hand: correct lengths, ports and checksum. */
class DnsVpnPacketTest {
    @Test fun ipv4UdpHeaderIsValid() {
        val payload = ByteArray(40) { it.toByte() }
        val p = DnsVpnService.ipv4Udp(byteArrayOf(10, 111, 0, 1), byteArrayOf(10, 111, 0, 2), 53, 40000, payload)
        fun u16(i: Int) = ((p[i].toInt() and 0xFF) shl 8) or (p[i + 1].toInt() and 0xFF)
        assertEquals(68, p.size)
        assertEquals(68, u16(2))            // total IP length
        assertEquals(53, u16(20))           // source port
        assertEquals(40000, u16(22))        // destination port
        assertEquals(48, u16(24))           // UDP length
        var sum = 0
        for (i in 0 until 20 step 2) sum += u16(i)
        while (sum shr 16 != 0) sum = (sum and 0xFFFF) + (sum shr 16)
        assertEquals(0xFFFF, sum)           // valid header checksum
        assertEquals(payload.toList(), p.copyOfRange(28, 68).toList())
    }
}
