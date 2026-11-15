package dev.procastinacioff.app.family

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
 * El tutor confirma con huella o PIN antes de aprobar solicitudes o cambiar ajustes: así un menor
 * con el teléfono del tutor en la mano no puede aprobarse nada.
 * Si el teléfono no tiene bloqueo de pantalla no hay nada que pedir (lo decidió el tutor).
 */
suspend fun confirmAsGuardian(activity: FragmentActivity, what: String): Boolean {
    // En Android 10, BIOMETRIC_STRONG + DEVICE_CREDENTIAL no está soportado
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
                .setTitle("Confirmar como tutor")
                .setSubtitle(what)
                .setAllowedAuthenticators(allowed)
                .build(),
        )
        cont.invokeOnCancellation { prompt.cancelAuthentication() }
    }
}
