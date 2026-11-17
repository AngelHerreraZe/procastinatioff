package dev.procastinatioff.app.verify

import dev.procastinatioff.app.R
import android.app.Activity
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageInstaller
import android.content.pm.PackageManager
import android.os.Build
import android.os.SystemClock
import androidx.core.content.ContextCompat
import androidx.core.content.IntentCompat
import dev.procastinatioff.app.core.CoreHolder
import dev.procastinatioff.app.core.userMessage
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import uniffi.procastinatioff_core.Method
import kotlin.coroutines.resume

/**
 * SECURITY: The camera and the microphone live in another APK (dev.procastinatioff.verifier) WITHOUT the
 * Internet permission: Android denies it the network. This app only receives VerificationResult's 5 fields.
 */
const val VERIFIER_PKG = "dev.procastinatioff.verifier"

/** Only a verifier signed with our same key is trusted. */
fun verifierIsGenuine(ctx: Context): Boolean = try {
    ctx.packageManager.getPackageInfo(VERIFIER_PKG, 0)
    ctx.packageManager.checkSignatures(ctx.packageName, VERIFIER_PKG) == PackageManager.SIGNATURE_MATCH
} catch (e: PackageManager.NameNotFoundException) {
    false
}

/**
 * The verifier ships inside this APK (assets/verifier.apk). Installs it when it is missing or older than
 * this app; Android asks the user to confirm (the first time, also to allow installs from this app).
 * Returns whether a genuine verifier is ready.
 */
suspend fun ensureVerifier(ctx: Context): Boolean {
    val pm = ctx.packageManager
    val installedAt = try {
        pm.getPackageInfo(VERIFIER_PKG, 0).lastUpdateTime
    } catch (e: PackageManager.NameNotFoundException) {
        0L
    }
    // ponytail: wall-clock order, so it reinstalls after every app update (silent on Android 12+).
    // Compare version codes if the prompt on Android 10/11 bothers anyone.
    if (installedAt < pm.getPackageInfo(ctx.packageName, 0).lastUpdateTime) withContext(Dispatchers.IO) { install(ctx.applicationContext) }
    return verifierIsGenuine(ctx)
}

/**
 * SECURITY: The system installer gets through the blocker only while OUR verifier install waits for the
 * user, at most 2 min. ponytail: by package, so "uninstall" (same package) is also reachable in that window;
 * filter by the window's class name if that ever matters.
 */
@Volatile private var installerUntil: Pair<String, Long>? = null

fun installerForVerifier(): String? = installerUntil?.takeIf { SystemClock.elapsedRealtime() < it.second }?.first

private suspend fun install(ctx: Context): Boolean = suspendCancellableCoroutine { cont ->
    val action = "${ctx.packageName}.VERIFIER_INSTALL"
    val receiver = object : BroadcastReceiver() {
        override fun onReceive(c: Context, i: Intent) {
            val status = i.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)
            if (status == PackageInstaller.STATUS_PENDING_USER_ACTION) {
                // From the system installer, through our own non-exported receiver: its confirmation screen
                IntentCompat.getParcelableExtra(i, Intent.EXTRA_INTENT, Intent::class.java)?.let { confirm ->
                    (confirm.`package` ?: confirm.resolveActivity(ctx.packageManager)?.packageName)
                        ?.let { installerUntil = it to SystemClock.elapsedRealtime() + 120_000 }
                    ctx.startActivity(confirm.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
                }
                return
            }
            installerUntil = null
            runCatching { ctx.unregisterReceiver(this) }
            if (cont.isActive) cont.resume(status == PackageInstaller.STATUS_SUCCESS)
        }
    }
    ContextCompat.registerReceiver(ctx, receiver, IntentFilter(action), ContextCompat.RECEIVER_NOT_EXPORTED)
    cont.invokeOnCancellation { runCatching { ctx.unregisterReceiver(receiver) } }
    try {
        val installer = ctx.packageManager.packageInstaller
        val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL).apply {
            setAppPackageName(VERIFIER_PKG)
            if (Build.VERSION.SDK_INT >= 31) setRequireUserAction(PackageInstaller.SessionParams.USER_ACTION_NOT_REQUIRED)
        }
        val id = installer.createSession(params)
        installer.openSession(id).use { session ->
            ctx.assets.open("verifier.apk").use { input ->
                session.openWrite("verifier.apk", 0, -1).use { out ->
                    input.copyTo(out)
                    session.fsync(out)
                }
            }
            val mutable = if (Build.VERSION.SDK_INT >= 31) PendingIntent.FLAG_MUTABLE else 0 // the system adds the status
            val done = PendingIntent.getBroadcast(ctx, id, Intent(action).setPackage(ctx.packageName), mutable or PendingIntent.FLAG_UPDATE_CURRENT)
            session.commit(done.intentSender)
        }
    } catch (e: Exception) {
        runCatching { ctx.unregisterReceiver(receiver) }
        if (cont.isActive) cont.resume(false)
    }
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

/** `night`: the verifier cannot see the core, so the app tells it which look to use (DESIGN.md). */
fun verifierIntent(goalId: String, method: Method, targetMinutes: Int, night: Boolean): Intent? {
    val name = method.verifierName() ?: return null
    return Intent().setClassName(VERIFIER_PKG, "$VERIFIER_PKG.VerifyActivity")
        .putExtra("goal_id", goalId).putExtra("method", name)
        .putExtra("target_minutes", targetMinutes.coerceIn(1, 600))
        .putExtra("target_reps", POSE_REPS)
        .putExtra("min_pages", MIN_PAGES)
        .putExtra("night", night)
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
