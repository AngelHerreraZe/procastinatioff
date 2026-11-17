package dev.procastinatioff.verifier

import android.content.Context
import java.security.MessageDigest

/** Bundled models: their SHA-256 (pinned in models.lock) is verified before loading them. */
object Models {
    private val expected: Map<String, String> = BuildConfig.MODEL_HASHES.split(';')
        .filter { it.contains('=') }
        .associate { it.substringBefore('=') to it.substringAfter('=') }

    class MissingModel(path: String) : Exception("missing models for this method ($path); see spike E")

    /** Returns the asset path if it matches the pinned hash; otherwise fails. */
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
        check(digest == sha) { "tampered model: $name" }
        return path
    }
}
