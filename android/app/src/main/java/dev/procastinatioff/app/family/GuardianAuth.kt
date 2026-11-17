package dev.procastinatioff.app.family

import dev.procastinatioff.app.R
import android.os.Build
import androidx.biometric.BiometricManager
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_STRONG
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_WEAK
import androidx.biometric.BiometricManager.Authenticators.DEVICE_CREDENTIAL
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import kotlin.coroutines.resume
import kotlinx.coroutines.suspendCancellableCoroutine

/**
 * The guardian (or a trusted contact) confirms with fingerprint or the phone's PIN before approving
 * requests or changing settings: someone else holding the phone cannot approve anything.
 * If the phone has no screen lock there is nothing to ask for (its owner decided that).
 */
suspend fun confirmAsGuardian(
    activity: FragmentActivity,
    what: String,
    title: String = activity.getString(R.string.confirm_as_guardian),
): Boolean {
    // On Android 10, BIOMETRIC_STRONG + DEVICE_CREDENTIAL is not supported
    val allowed = if (Build.VERSION.SDK_INT >= 30) BIOMETRIC_STRONG or DEVICE_CREDENTIAL else BIOMETRIC_WEAK or DEVICE_CREDENTIAL
    if (BiometricManager.from(activity).canAuthenticate(allowed) != BiometricManager.BIOMETRIC_SUCCESS) return true
    return suspendCancellableCoroutine { cont ->
        val prompt = BiometricPrompt(activity, ContextCompat.getMainExecutor(activity), object : BiometricPrompt.AuthenticationCallback() {
            override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                if (cont.isActive) cont.resume(true)
            }
            override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
                if (cont.isActive) cont.resume(false)
            }
        })
        prompt.authenticate(
            BiometricPrompt.PromptInfo.Builder()
                .setTitle(title)
                .setSubtitle(what)
                .setAllowedAuthenticators(allowed)
                .build(),
        )
        cont.invokeOnCancellation { prompt.cancelAuthentication() }
    }
}
