package dev.procastinacioff.app.core

import android.content.Context
import android.content.pm.PackageManager
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.security.keystore.StrongBoxUnavailableException
import java.io.File
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import uniffi.procastinacioff_core.CoreException
import uniffi.procastinacioff_core.SecretStore

/**
 * SECURITY: Android keystore. An AES-256-GCM key lives in the Keystore (in StrongBox if available) and never
 * leaves the hardware. It encrypts the core's secrets, stored in noBackupFilesDir.
 */
class KeystoreSecretStore(private val context: Context) : SecretStore {
    private val dir = File(context.noBackupFilesDir, "secrets").apply { mkdirs() }
    private val alias = "procastinacioff_master_v1"

    private fun file(name: String): File {
        if (name.isEmpty() || !name.all { it.isLetterOrDigit() || it == '_' }) {
            throw CoreException.Secret("invalid secret name")
        }
        return File(dir, "$name.bin")
    }

    @Synchronized
    private fun masterKey(): SecretKey {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (ks.getKey(alias, null) as? SecretKey)?.let { return it }
        val strongBox = context.packageManager.hasSystemFeature(PackageManager.FEATURE_STRONGBOX_KEYSTORE)
        return try {
            generate(strongBox)
        } catch (e: StrongBoxUnavailableException) {
            generate(false) // regular TEE if StrongBox is not available
        }
    }

    private fun generate(strongBox: Boolean): SecretKey {
        val spec = KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
            .setRandomizedEncryptionRequired(true)
            .setIsStrongBoxBacked(strongBox)
            .build()
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").run {
            init(spec)
            generateKey()
        }
    }

    override fun load(name: String): ByteArray? {
        val f = file(name)
        if (!f.exists()) return null
        try {
            val bytes = f.readBytes()
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.DECRYPT_MODE, masterKey(), GCMParameterSpec(128, bytes, 0, 12))
            return cipher.doFinal(bytes, 12, bytes.size - 12)
        } catch (e: Exception) {
            throw CoreException.Secret("could not decrypt the secret") // no details: they could leak information
        }
    }

    override fun save(name: String, value: ByteArray) {
        try {
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.ENCRYPT_MODE, masterKey())
            val tmp = File(dir, "$name.tmp")
            tmp.writeBytes(cipher.iv + cipher.doFinal(value))
            check(tmp.renameTo(file(name))) // atomic write
        } catch (e: Exception) {
            throw CoreException.Secret("could not save the secret")
        }
    }
}
