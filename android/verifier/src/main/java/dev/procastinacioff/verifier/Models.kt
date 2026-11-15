package dev.procastinacioff.verifier

import android.content.Context
import java.security.MessageDigest

/** Modelos empaquetados: se verifica su SHA-256 (fijado en models.lock) antes de cargarlos. */
object Models {
    private val expected: Map<String, String> = BuildConfig.MODEL_HASHES.split(';')
        .filter { it.contains('=') }
        .associate { it.substringBefore('=') to it.substringAfter('=') }

    class MissingModel(path: String) : Exception("Faltan los modelos de este método ($path). Ver spike E.")

    /** Devuelve la ruta del asset si coincide con el hash fijado; si no, falla. */
    fun verified(ctx: Context, name: String): String {
        val path = "models/$name"
        val sha = expected[name] ?: throw MissingModel(name)
        val digest = try {
            ctx.assets.open(path).use { input ->
                val md = MessageDigest.getInstance("SHA-256")
                val buf = ByteArray(64 * 1024)
                while (true) {
                    val n = input.read(buf)
                    if (n < 0) break
                    md.update(buf, 0, n)
                }
                md.digest().joinToString("") { "%02x".format(it) }
            }
        } catch (e: java.io.IOException) {
            throw MissingModel(name)
        }
        check(digest == sha) { "Modelo alterado: $name" }
        return path
    }
}
