package dev.procastinatioff.verifier

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.io.File
import java.nio.ByteBuffer
import java.security.KeyStore
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * SECURITY: The ONLY thing the verifier stores: the device mask (64 random bits) and the
 * SALTED perceptual hashes of the food photos from the last 7 days (they cannot
 * reconstruct the image). Encrypted with a Keystore key, in noBackupFilesDir.
 */
object HashStore {
    private const val ALIAS = "procastinatioff_verifier_hashes_v1"
    private const val KEEP_MS = 7L * 24 * 3600 * 1000

    private class Data(val mask: Long, val entries: List<Pair<Long, Long>>) // (instant, salted hash)

    private fun file(ctx: Context) = File(ctx.noBackupFilesDir, "hashes.bin")

    private fun key(): SecretKey {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (ks.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val spec = KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
            .build()
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").run { init(spec); generateKey() }
    }

    @Synchronized
    private fun load(ctx: Context): Data {
        val f = file(ctx)
        if (f.exists()) {
            runCatching {
                val bytes = f.readBytes()
                val c = Cipher.getInstance("AES/GCM/NoPadding")
                c.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, bytes, 0, 12))
                val buf = ByteBuffer.wrap(c.doFinal(bytes, 12, bytes.size - 12))
                val mask = buf.long
                val entries = buildList { while (buf.remaining() >= 16) add(buf.long to buf.long) }
                return Data(mask, entries)
            }
        }
        return Data(SecureRandom().nextLong(), emptyList())
    }

    @Synchronized
    private fun save(ctx: Context, d: Data) {
        val buf = ByteBuffer.allocate(8 + 16 * d.entries.size).putLong(d.mask)
        d.entries.forEach { (t, h) -> buf.putLong(t).putLong(h) }
        val c = Cipher.getInstance("AES/GCM/NoPadding")
        c.init(Cipher.ENCRYPT_MODE, key())
        val tmp = File(ctx.noBackupFilesDir, "hashes.tmp")
        tmp.writeBytes(c.iv + c.doFinal(buf.array()))
        check(tmp.renameTo(file(ctx)))
    }

    fun mask(ctx: Context): ULong = load(ctx).mask.toULong()

    fun recent(ctx: Context): List<ULong> {
        val now = System.currentTimeMillis()
        return load(ctx).entries.filter { now - it.first < KEEP_MS }.map { it.second.toULong() }
    }

    fun add(ctx: Context, salted: ULong) {
        val d = load(ctx)
        val now = System.currentTimeMillis()
        save(ctx, Data(d.mask, d.entries.filter { now - it.first < KEEP_MS } + (now to salted.toLong())))
    }
}
