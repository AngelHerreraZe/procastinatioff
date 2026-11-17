package dev.procastinacioff.app.auth

import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import androidx.annotation.RequiresApi
import androidx.biometric.BiometricManager
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_STRONG
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import java.security.KeyStore
import javax.crypto.KeyGenerator
import javax.crypto.Mac
import kotlin.coroutines.resume
import kotlinx.coroutines.suspendCancellableCoroutine

/**
 * Biometrics as a local method (ADR 0013). SECURITY:
 * - Success is tied to a real crypto operation (`CryptoObject` with a Keystore HMAC key usable only
 *   right after class-3 biometrics), not just to a callback.
 * - `setInvalidatedByBiometricEnrollment(true)`: if someone enrolls another fingerprint (e.g. a
 *   minor on the guardian's phone), the key dies and biometrics must be re-enabled with the PIN.
 * - Android 10 has no per-use class-3 keys: there biometrics is not offered (PIN/password/pattern).
 */
object LocalAuth {
    private const val ALIAS = "pco_biometric_v1"

    fun available(activity: FragmentActivity): Boolean =
        Build.VERSION.SDK_INT >= 30 &&
            BiometricManager.from(activity).canAuthenticate(BIOMETRIC_STRONG) == BiometricManager.BIOMETRIC_SUCCESS

    @RequiresApi(30)
    fun ensureKey() {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        if (ks.containsAlias(ALIAS)) return
        KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_HMAC_SHA256, "AndroidKeyStore").apply {
            init(
                KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_SIGN)
                    .setUserAuthenticationRequired(true)
                    .setUserAuthenticationParameters(0, KeyProperties.AUTH_BIOMETRIC_STRONG)
                    .setInvalidatedByBiometricEnrollment(true)
                    .build(),
            )
        }.generateKey()
    }

    /** The key exists and is still valid (enrolling another fingerprint invalidates and deletes it). */
    fun keyValid(): Boolean = Build.VERSION.SDK_INT >= 30 && mac() != null

    /** null = no key, or invalidated (new fingerprint enrolled): offer the app's PIN/password/pattern. */
    private fun mac(): Mac? = try {
        val key = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }.getKey(ALIAS, null)
        key?.let { Mac.getInstance("HmacSHA256").apply { init(it) } }
    } catch (e: KeyPermanentlyInvalidatedException) {
        KeyStore.getInstance("AndroidKeyStore").apply { load(null) }.deleteEntry(ALIAS)
        null
    }

    /** true only if the OS confirmed class-3 biometrics AND the Keystore key computed a MAC. */
    suspend fun confirm(activity: FragmentActivity, title: String, subtitle: String): Boolean {
        if (!available(activity)) return false
        val mac = mac() ?: return false
        return suspendCancellableCoroutine { cont ->
            val prompt = BiometricPrompt(activity, ContextCompat.getMainExecutor(activity), object : BiometricPrompt.AuthenticationCallback() {
                override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                    val ok = runCatching { result.cryptoObject?.mac?.doFinal(ByteArray(32))?.size == 32 }.getOrDefault(false)
                    if (cont.isActive) cont.resume(ok)
                }
                override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
                    if (cont.isActive) cont.resume(false)
                }
            })
            prompt.authenticate(
                BiometricPrompt.PromptInfo.Builder()
                    .setTitle(title)
                    .setSubtitle(subtitle)
                    .setAllowedAuthenticators(BIOMETRIC_STRONG)
                    .setNegativeButtonText(activity.getString(android.R.string.cancel))
                    .build(),
                BiometricPrompt.CryptoObject(mac),
            )
            cont.invokeOnCancellation { prompt.cancelAuthentication() }
        }
    }
}
