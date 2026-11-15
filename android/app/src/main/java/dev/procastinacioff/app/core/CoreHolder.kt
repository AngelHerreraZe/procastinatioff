package dev.procastinacioff.app.core

import android.content.Context
import android.os.SystemClock
import java.io.File
import java.util.TimeZone
import uniffi.procastinacioff_core.Clock
import uniffi.procastinacioff_core.Core
import uniffi.procastinacioff_core.CoreException

/**
 * Acceso único a los núcleos Rust. Todos comparten las claves del dispositivo (mismo llavero):
 * - core.db: el perfil de este dispositivo (propio, o el del menor si este es su teléfono).
 * - partner.db: buzón con MI compañero de rendición de cuentas (solo avisos).
 * - watching.db: buzón de la persona a la que YO acompaño.
 * - family-<id>.db: (tutor) un perfil por cada menor que administro. Nunca bloquean ESTE teléfono.
 */
object CoreHolder {
    @Volatile private var core: Core? = null
    @Volatile private var partner: Core? = null
    @Volatile private var watching: Core? = null
    private val families = HashMap<String, Core>()

    /** Perfiles de menores (id → nombre). Solo en este teléfono; el nombre nunca sale de aquí. */
    fun familyProfiles(context: Context): Map<String, String> =
        context.getSharedPreferences("family", Context.MODE_PRIVATE).all
            .mapNotNull { (k, v) -> (v as? String)?.let { k to it } }.toMap().toSortedMap()

    fun family(context: Context, id: String): Core = synchronized(families) {
        require(id.matches(Regex("^[0-9a-f]{16}$"))) { "id de perfil inválido" }
        families.getOrPut(id) { open(context, "family-$id.db") }
    }

    fun newFamilyId(): String =
        ByteArray(8).also(java.security.SecureRandom()::nextBytes).joinToString("") { "%02x".format(it) }

    /** Registra el perfil (solo cuando ya se creó en el núcleo). */
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

    /** El buzón solo existe si ya se creó o se unió (no se abre una base vacía por curiosidad). */
    fun partnerBoxOrNull(context: Context): Core? =
        if (File(context.noBackupFilesDir, "partner.db").exists()) partnerBox(context).takeIf { it.members().isNotEmpty() } else null

    fun watchingBoxOrNull(context: Context): Core? =
        if (File(context.noBackupFilesDir, "watching.db").exists()) watchingBox(context).takeIf { it.members().isNotEmpty() } else null

    /**
     * Hora para las reglas: hora firmada del relay + reloj monotónico (el usuario no puede cambiarlo).
     * Sin ancla todavía (sin red desde que arrancó el proceso), el reloj del sistema.
     */
    fun clock(): Clock {
        val now = core?.trustedNowMs(System.currentTimeMillis(), SystemClock.elapsedRealtime()) ?: System.currentTimeMillis()
        return Clock(nowMs = now, utcOffsetMin = TimeZone.getDefault().getOffset(now) / 60_000)
    }

    /** Ancla la hora con el relay. Se llama antes de cada sincronización. */
    fun anchor(core: Core) {
        runCatching { core.anchorTime(System.currentTimeMillis(), SystemClock.elapsedRealtime()) }
    }
}

/** Texto para la persona usuaria: el motivo del núcleo (en español, sin datos personales) o uno genérico. */
fun Throwable.userMessage(): String = when (this) {
    is CoreException.Denied -> reason
    is CoreException.Network -> "Sin conexión con el servidor. Se reintentará."
    is CoreException -> "Algo falló. Inténtalo de nuevo."
    else -> message ?: "Algo falló. Inténtalo de nuevo."
}
