package dev.procastinacioff.app.verify

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import dev.procastinacioff.app.core.CoreHolder
import dev.procastinacioff.app.core.userMessage
import uniffi.procastinacioff_core.Method

/**
 * 🔒 La cámara y el micrófono viven en otro APK (dev.procastinacioff.verifier) SIN permiso de
 * Internet: Android le niega la red. Esta app solo recibe los 5 campos de VerificationResult.
 */
const val VERIFIER_PKG = "dev.procastinacioff.verifier"

/** Solo se confía en un verificador firmado con nuestra misma clave. */
fun verifierIsGenuine(ctx: Context): Boolean = try {
    ctx.packageManager.getPackageInfo(VERIFIER_PKG, 0)
    ctx.packageManager.checkSignatures(ctx.packageName, VERIFIER_PKG) == PackageManager.SIGNATURE_MATCH
} catch (e: PackageManager.NameNotFoundException) {
    false
}

/** Nombre del método tal como lo espera el verificador (el de los bindings de `verify`). */
private fun Method.verifierName(): String? = when (this) {
    Method.READING_PRESENCE -> "READING_PRESENCE"
    Method.POSE_REPS -> "POSE_REPS"
    Method.FOOD_PHOTO -> "FOOD_PHOTO"
    Method.PAGE_OCR -> "PAGE_OCR"
    Method.SUMMARY -> "SUMMARY"
    else -> null
}

/** Repeticiones por sesión de ejercicio y páginas mínimas por lectura. ponytail: fijos (perillas). */
const val POSE_REPS = 20
const val MIN_PAGES = 10

fun verifierIntent(goalId: String, method: Method, targetMinutes: Int): Intent? {
    val name = method.verifierName() ?: return null
    return Intent().setClassName(VERIFIER_PKG, "$VERIFIER_PKG.VerifyActivity")
        .putExtra("goal_id", goalId).putExtra("method", name)
        .putExtra("target_minutes", targetMinutes.coerceIn(1, 600))
        .putExtra("target_reps", POSE_REPS)
        .putExtra("min_pages", MIN_PAGES)
}

/**
 * Procesa el resultado del verificador. Devuelve null si se registró la meta, o un mensaje para mostrar.
 * Se ignora todo lo que no sean los 5 campos esperados.
 */
fun onVerifierResult(ctx: Context, expectedGoal: String, resultCode: Int, data: Intent?): String? {
    if (resultCode != Activity.RESULT_OK || data == null) return "La comprobación se detuvo sin resultado."
    if (!verifierIsGenuine(ctx)) return "El verificador instalado no es auténtico."
    if (!data.getBooleanExtra("approved", false)) return "Esta vez no se pudo comprobar. Puedes intentarlo de nuevo o usar otra opción."
    val method = when (data.getStringExtra("method")) {
        "READING_PRESENCE" -> Method.READING_PRESENCE
        "POSE_REPS" -> Method.POSE_REPS
        "FOOD_PHOTO" -> Method.FOOD_PHOTO
        "PAGE_OCR" -> Method.PAGE_OCR
        "SUMMARY" -> Method.SUMMARY
        else -> return "Resultado inválido."
    }
    val goalId = data.getStringExtra("goal_id")
    if (goalId != expectedGoal) return "Resultado de otra meta."
    return runCatching {
        val core = CoreHolder.get(ctx)
        core.completeGoal(
            CoreHolder.clock(),
            goalId,
            method,
            data.getIntExtra("score", 0).coerceIn(0, 100).toUByte(),
            data.getIntExtra("minutes", 0).coerceIn(0, 1440).toUShort(),
        )
    }.exceptionOrNull()?.userMessage()
}
