package dev.procastinacioff.app.core

import androidx.annotation.StringRes
import dev.procastinacioff.app.R
import android.content.Context
import android.os.SystemClock
import java.io.File
import java.util.TimeZone
import uniffi.procastinacioff_core.Clock
import uniffi.procastinacioff_core.Core
import uniffi.procastinacioff_core.CoreException

/**
 * Single access point to the Rust cores. They all share the device keys (same keystore):
 * - core.db: this device's profile (its own, or the minor's if this is their phone).
 * - partner.db: mailbox with MY accountability partner (alerts only).
 * - watching.db: mailbox of the person I support.
 * - family-<id>.db: (guardian) one profile per minor I manage. They never block THIS phone.
 */
object CoreHolder {
    @Volatile private var core: Core? = null
    @Volatile private var partner: Core? = null
    @Volatile private var watching: Core? = null
    private val families = HashMap<String, Core>()

    /** Minors' profiles (id → name). Only on this phone; the name never leaves it. */
    fun familyProfiles(context: Context): Map<String, String> =
        context.getSharedPreferences("family", Context.MODE_PRIVATE).all
            .mapNotNull { (k, v) -> (v as? String)?.let { k to it } }.toMap().toSortedMap()

    fun family(context: Context, id: String): Core = synchronized(families) {
        require(id.matches(Regex("^[0-9a-f]{16}$"))) { "invalid profile id" }
        families.getOrPut(id) { open(context, "family-$id.db") }
    }

    fun newFamilyId(): String =
        ByteArray(8).also(java.security.SecureRandom()::nextBytes).joinToString("") { "%02x".format(it) }

    /** Registers the profile (only once it was created in the core). */
    fun saveFamilyProfile(context: Context, id: String, childName: String) {
        context.getSharedPreferences("family", Context.MODE_PRIVATE).edit().putString(id, childName.take(64)).apply()
    }

    private fun open(context: Context, name: String): Core = Core.open(
        File(context.applicationContext.noBackupFilesDir, name).path,
        KeystoreSecretStore(context.applicationContext),
    )

    fun get(context: Context): Core =
        core ?: synchronized(this) { core ?: open(context, "core.db").also { core = it } }

    fun partnerBox(context: Context): Core =
        partner ?: synchronized(this) { partner ?: open(context, "partner.db").also { partner = it } }

    fun watchingBox(context: Context): Core =
        watching ?: synchronized(this) { watching ?: open(context, "watching.db").also { watching = it } }

    /** The mailbox only exists if it was created or joined (an empty database is never opened out of curiosity). */
    fun partnerBoxOrNull(context: Context): Core? =
        if (File(context.noBackupFilesDir, "partner.db").exists()) partnerBox(context).takeIf { it.members().isNotEmpty() } else null

    fun watchingBoxOrNull(context: Context): Core? =
        if (File(context.noBackupFilesDir, "watching.db").exists()) watchingBox(context).takeIf { it.members().isNotEmpty() } else null

    /**
     * Time for the rules: relay signed time + monotonic clock (the user cannot change it).
     * Without an anchor yet (no network since the process started), the system clock.
     */
    fun clock(): Clock {
        val now = core?.trustedNowMs(System.currentTimeMillis(), SystemClock.elapsedRealtime()) ?: System.currentTimeMillis()
        return Clock(nowMs = now, utcOffsetMin = TimeZone.getDefault().getOffset(now) / 60_000)
    }

    /** Anchors the time with the relay. Called before every sync. */
    fun anchor(core: Core) {
        runCatching { core.anchorTime(System.currentTimeMillis(), SystemClock.elapsedRealtime()) }
    }
}

/**
 * Text for the user. The core never sends sentences: `Denied.reason` is a stable code
 * (`Denied::code` in core/src/rules.rs) translated here with the app's string resources.
 */
fun Throwable.userMessage(ctx: Context): String = when (this) {
    is CoreException.Denied ->
        if (reason == "cooldown") ctx.getString(R.string.denied_cooldown, cooldownMinutes.toInt()) else ctx.getString(deniedText(reason))
    is CoreException.Network -> ctx.getString(R.string.error_network)
    is CoreException.Storage -> ctx.getString(R.string.error_storage)
    is CoreException.Secret -> ctx.getString(R.string.error_secret)
    else -> ctx.getString(R.string.error_generic)
}

@StringRes
private fun deniedText(code: String): Int = when (code) {
    "not_configured" -> R.string.denied_not_configured
    "wrong_state" -> R.string.denied_wrong_state
    "no_permits_left" -> R.string.denied_no_permits_left
    "unknown_goal" -> R.string.denied_unknown_goal
    "method_not_allowed" -> R.string.denied_method_not_allowed
    "already_completed" -> R.string.denied_already_completed
    "invalid_plan" -> R.string.denied_invalid_plan
    "plan_laxer" -> R.string.denied_plan_laxer
    "no_key_goal" -> R.string.denied_no_key_goal
    "guardian_only" -> R.string.denied_guardian_only
    "invalid_domain" -> R.string.denied_invalid_domain
    "too_many_requests" -> R.string.denied_too_many_requests
    else -> R.string.error_generic
}
