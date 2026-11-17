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
import uniffi.procastinacioff_core.SyncReport

/**
 * Single access point to the Rust cores. They all share the device keys (same keystore):
 * - core.db: this device's profile (its own, or the minor's if this is their phone).
 * - contact.db: channel with MY trusted contacts (requests, their answers, alerts).
 * - watching.db: channel of the person I support as their trusted contact.
 * - family-<id>.db: (guardian) one profile per minor I manage. They never block THIS phone.
 */
object CoreHolder {
    @Volatile private var core: Core? = null
    @Volatile private var contact: Core? = null
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
        core ?: synchronized(this) {
            core ?: open(context, "core.db").also {
                // A minor's phone with Device Owner: the minor cannot change its clock (DISALLOW_CONFIG_DATE_TIME).
                runCatching { it.setTimeSource(dev.procastinacioff.app.family.DeviceOwnerPolicies.isDeviceOwner(context)) }
                core = it
            }
        }

    fun contactChannel(context: Context): Core =
        contact ?: synchronized(this) { contact ?: open(context, "contact.db").also { contact = it } }

    fun watchingBox(context: Context): Core =
        watching ?: synchronized(this) { watching ?: open(context, "watching.db").also { watching = it } }

    /** The channel only exists if it was created or joined (an empty database is never opened out of curiosity). */
    fun contactChannelOrNull(context: Context): Core? =
        if (File(context.noBackupFilesDir, "contact.db").exists()) contactChannel(context).takeIf { it.members().isNotEmpty() } else null

    fun watchingBoxOrNull(context: Context): Core? =
        if (File(context.noBackupFilesDir, "watching.db").exists()) watchingBox(context).takeIf { it.members().isNotEmpty() } else null

    /**
     * Time for the rules: the paired devices' time (anchored on each sync) + monotonic clock (the
     * user cannot change it). Without an anchor yet (no sync since the process started), the system clock.
     */
    fun clock(): Clock {
        val now = core?.trustedNowMs(System.currentTimeMillis(), SystemClock.elapsedRealtime()) ?: System.currentTimeMillis()
        return Clock(nowMs = now, utcOffsetMin = TimeZone.getDefault().getOffset(now) / 60_000)
    }

    /** Syncs with the profile's devices whose LAN address is known (blocking: background thread only). */
    fun syncNow(core: Core): SyncReport = core.syncNow(System.currentTimeMillis(), SystemClock.elapsedRealtime())

    /**
     * "Delete all my data" on this phone (privacy contract, point 9). This phone's profile goes first:
     * it needs what relaxing needs, and if the core refuses, nothing else is touched. Then the contact
     * channels and the minors' profiles, then the app's own preferences. Every Core goes on as a new,
     * empty device. Blocking: background thread only.
     */
    fun eraseAll(context: Context) {
        val clock = clock()
        // Opened before the profile is erased: a database from before per-database keys adopts the
        // shared key on open, and the profile's erase replaces that key.
        val others = listOfNotNull(contactChannelOrNull(context), watchingBoxOrNull(context)) +
            familyProfiles(context).keys.map { family(context, it) }
        get(context).eraseAll(clock)
        others.forEach { it.eraseAll(clock) }
        listOf("family", "tamper").forEach { context.getSharedPreferences(it, Context.MODE_PRIVATE).edit().clear().commit() }
    }

    @Volatile private var listLoaded = false

    /**
     * (A minor's phone) The signed category list ships in the APK (`assets/lists/`): updating it means
     * updating the app. Loaded once per process; without a valid signature it is ignored.
     */
    fun loadBundledList(context: Context, core: Core) {
        if (listLoaded) return
        runCatching {
            val assets = context.assets
            val data = assets.open("lists/categories.txt").use { it.readBytes() }
            val sig = assets.open("lists/categories.txt.minisig").use { it.readBytes().decodeToString() }
            core.loadCategoryList(data, sig)
        }
        listLoaded = true
    }
}

/**
 * Text for the user. The core never sends sentences: `Denied.reason` is a stable code
 * (`Denied::code` in core/src/rules.rs) translated here with the app's string resources.
 */
fun Throwable.userMessage(ctx: Context): String = when (this) {
    is CoreException.Denied ->
        when (reason) {
            "cooldown" -> ctx.getString(R.string.denied_cooldown, cooldownMinutes.toInt())
            "locked" -> ctx.getString(R.string.denied_locked, cooldownMinutes.toInt())
            else -> ctx.getString(deniedText(reason))
        }
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
    "auth_required" -> R.string.denied_auth_required
    "wrong_secret" -> R.string.denied_wrong_secret
    "method_not_set" -> R.string.denied_method_not_set
    "weak_pin" -> R.string.denied_weak_pin
    "easy_pin" -> R.string.denied_easy_pin
    "weak_password" -> R.string.denied_weak_password
    "weak_pattern" -> R.string.denied_weak_pattern
    "needs_contact" -> R.string.denied_needs_contact
    "no_contact" -> R.string.denied_no_contact
    "invalid_code" -> R.string.denied_invalid_code
    "too_many_grants" -> R.string.denied_too_many_grants
    else -> R.string.error_generic
}
