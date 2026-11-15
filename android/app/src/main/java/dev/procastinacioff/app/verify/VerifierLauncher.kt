package dev.procastinacioff.app.verify

import dev.procastinacioff.app.R
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import dev.procastinacioff.app.core.CoreHolder
import dev.procastinacioff.app.core.userMessage
import uniffi.procastinacioff_core.Method

/**
 * SECURITY: The camera and the microphone live in another APK (dev.procastinacioff.verifier) WITHOUT the
 * Internet permission: Android denies it the network. This app only receives VerificationResult's 5 fields.
 */
const val VERIFIER_PKG = "dev.procastinacioff.verifier"

/** Only a verifier signed with our same key is trusted. */
fun verifierIsGenuine(ctx: Context): Boolean = try {
    ctx.packageManager.getPackageInfo(VERIFIER_PKG, 0)
    ctx.packageManager.checkSignatures(ctx.packageName, VERIFIER_PKG) == PackageManager.SIGNATURE_MATCH
} catch (e: PackageManager.NameNotFoundException) {
    false
}

/** Method name as the verifier expects it (the one in the `verify` bindings). */
private fun Method.verifierName(): String? = when (this) {
    Method.READING_PRESENCE -> "READING_PRESENCE"
    Method.POSE_REPS -> "POSE_REPS"
    Method.FOOD_PHOTO -> "FOOD_PHOTO"
    Method.PAGE_OCR -> "PAGE_OCR"
    Method.SUMMARY -> "SUMMARY"
    else -> null
}

/** Reps per exercise session and minimum pages per reading. ponytail: fixed (knobs). */
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
 * Processes the verifier's result. Returns null if the goal was recorded, or a message to show.
 * Anything other than the 5 expected fields is ignored.
 */
fun onVerifierResult(ctx: Context, expectedGoal: String, resultCode: Int, data: Intent?): String? {
    if (resultCode != Activity.RESULT_OK || data == null) return ctx.getString(R.string.verify_no_result)
    if (!verifierIsGenuine(ctx)) return ctx.getString(R.string.verify_not_genuine)
    if (!data.getBooleanExtra("approved", false)) return ctx.getString(R.string.verify_not_confirmed)
    val method = when (data.getStringExtra("method")) {
        "READING_PRESENCE" -> Method.READING_PRESENCE
        "POSE_REPS" -> Method.POSE_REPS
        "FOOD_PHOTO" -> Method.FOOD_PHOTO
        "PAGE_OCR" -> Method.PAGE_OCR
        "SUMMARY" -> Method.SUMMARY
        else -> return ctx.getString(R.string.verify_unexpected)
    }
    val goalId = data.getStringExtra("goal_id")
    if (goalId != expectedGoal) return ctx.getString(R.string.verify_unexpected)
    return runCatching {
        val core = CoreHolder.get(ctx)
        core.completeGoal(
            CoreHolder.clock(),
            goalId,
            method,
            data.getIntExtra("score", 0).coerceIn(0, 100).toUByte(),
            data.getIntExtra("minutes", 0).coerceIn(0, 1440).toUShort(),
        )
    }.exceptionOrNull()?.userMessage(ctx)
}
