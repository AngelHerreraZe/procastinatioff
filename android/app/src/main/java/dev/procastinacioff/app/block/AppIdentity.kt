package dev.procastinacioff.app.block

import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.provider.Settings
import android.provider.Telephony
import android.telecom.TelecomManager
import java.security.MessageDigest

/** App identity by package + certificate: a fake APK with the same package does not pass. */
object AppIdentity {
    private val certCache = HashMap<String, String?>()

    /** SHA-256 (hex) of the package's current signing certificate. */
    fun certSha256(context: Context, pkg: String): String? = synchronized(certCache) {
        certCache.getOrPut(pkg) {
            try {
                val info = context.packageManager.getPackageInfo(pkg, PackageManager.GET_SIGNING_CERTIFICATES)
                val signers = info.signingInfo?.apkContentsSigners ?: return@getOrPut null
                val md = MessageDigest.getInstance("SHA-256")
                signers.firstOrNull()?.toByteArray()?.let { md.digest(it).joinToString("") { b -> "%02x".format(b) } }
            } catch (e: PackageManager.NameNotFoundException) {
                null
            }
        }
    }

    /** Identifier in the Config format: "android:<package>@<sha256>". */
    fun idOf(context: Context, pkg: String): String? = certSha256(context, pkg)?.let { "android:$pkg@$it" }

    fun invalidate(pkg: String) = synchronized(certCache) { certCache.remove(pkg) }

    /** System packages that are never blocked (without them the phone cannot be used). */
    fun systemAllowed(context: Context, sleeping: Boolean): Set<String> {
        val pm = context.packageManager
        val set = mutableSetOf(context.packageName, "com.android.systemui")
        context.getSystemService(TelecomManager::class.java)?.defaultDialerPackage?.let { set += it }
        set += listOf("com.android.server.telecom", "com.android.phone", "com.android.emergency")
        pm.resolveActivity(Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_HOME), 0)
            ?.activityInfo?.packageName?.let { set += it }                       // launcher
        Settings.Secure.getString(context.contentResolver, Settings.Secure.DEFAULT_INPUT_METHOD)
            ?.substringBefore('/')?.let { set += it }                            // keyboard
        set += listOf("com.google.android.permissioncontroller", "com.android.permissioncontroller")
        if (!sleeping) {
            Telephony.Sms.getDefaultSmsPackage(context)?.let { set += it }       // SMS (during the day)
            set += "com.android.settings"                                        // settings (during the day)
        }
        return set
    }
}
