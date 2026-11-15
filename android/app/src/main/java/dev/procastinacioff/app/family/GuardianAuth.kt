package dev.procastinacioff.app.family

import dev.procastinacioff.app.R
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
 * The guardian confirms with fingerprint or PIN before approving requests or changing settings: a minor
 * holding the guardian's phone cannot approve anything for themselves.
 * If the phone has no screen lock there is nothing to ask for (the guardian decided that).
 */
suspend fun confirmAsGuardian(activity: FragmentActivity, what: String): Boolean {
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
                .setTitle(activity.getString(R.string.confirm_as_guardian))
                .setSubtitle(what)
                .setAllowedAuthenticators(allowed)
                .build(),
        )
        cont.invokeOnCancellation { prompt.cancelAuthentication() }
    }
}
