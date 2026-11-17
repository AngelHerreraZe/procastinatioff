package dev.procastinatioff.app.family

import android.app.Activity
import android.app.admin.DevicePolicyManager
import android.content.BroadcastReceiver
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.net.VpnService
import android.os.Build
import android.os.Bundle
import android.os.UserManager
import dev.procastinatioff.app.block.AdminReceiver
import dev.procastinatioff.app.core.CoreHolder
import uniffi.procastinatioff_core.Role

/**
 * SECURITY: The minor's phone as Device Owner. It is set up from scratch with the provisioning
 * QR: the minor cannot factory-reset it, boot into safe mode, change the time,
 * add a VPN or users, debug over USB or uninstall the app.
 */
object DeviceOwnerPolicies {
    fun admin(ctx: Context) = ComponentName(ctx, AdminReceiver::class.java)

    fun isDeviceOwner(ctx: Context) =
        ctx.getSystemService(DevicePolicyManager::class.java).isDeviceOwnerApp(ctx.packageName)

    /** Applied at provisioning and on every boot (idempotent). */
    fun applyBaseline(ctx: Context) {
        if (!isDeviceOwner(ctx)) return
        val dpm = ctx.getSystemService(DevicePolicyManager::class.java)
        val admin = admin(ctx)
        listOf(
            UserManager.DISALLOW_FACTORY_RESET,
            UserManager.DISALLOW_SAFE_BOOT,
            UserManager.DISALLOW_CONFIG_DATE_TIME,
            UserManager.DISALLOW_ADD_USER,
            UserManager.DISALLOW_CONFIG_VPN,
            UserManager.DISALLOW_CONFIG_PRIVATE_DNS,
            UserManager.DISALLOW_INSTALL_UNKNOWN_SOURCES,
            UserManager.DISALLOW_DEBUGGING_FEATURES, // no ADB for the minor
            UserManager.DISALLOW_USB_FILE_TRANSFER,
        ).forEach { dpm.addUserRestriction(admin, it) }
        // Network time, not editable
        if (Build.VERSION.SDK_INT >= 30) dpm.setAutoTimeEnabled(admin, true) else @Suppress("DEPRECATION") dpm.setAutoTimeRequired(admin, true)
        dpm.setUninstallBlocked(admin, ctx.packageName, true)
        dpm.setUninstallBlocked(admin, "dev.procastinatioff.verifier", true)
        // Local DNS VPN always on, without "lockdown": it only routes DNS and the rest of the traffic
        // goes through the normal network. DISALLOW_CONFIG_VPN prevents removing it.
        runCatching { dpm.setAlwaysOnVpnPackage(admin, ctx.packageName, false) }
    }

    /**
     * Managed Chrome (Device Owner): no incognito nor encrypted DNS; in FOCUS, only the allowlist.
     * VERIFY: Each key's type is set by Chrome's schema: check it on device with
     * RestrictionsManager.getManifestRestrictions("com.android.chrome") (family-mode tests).
     */
    fun applyChromePolicy(ctx: Context, allowlist: List<String>?) {
        if (!isDeviceOwner(ctx)) return
        val b = Bundle().apply {
            putInt("IncognitoModeAvailability", 1)
            putString("DnsOverHttpsMode", "off")
            if (allowlist != null) {
                putStringArray("URLBlocklist", arrayOf("*"))
                putStringArray("URLAllowlist", allowlist.toTypedArray())
            }
        }
        ctx.getSystemService(DevicePolicyManager::class.java).setApplicationRestrictions(admin(ctx), "com.android.chrome", b)
    }

    /** On a minor's device: protection by suspension (Device Owner) and DNS filter. */
    fun startMinorProtection(ctx: Context) {
        val role = runCatching { CoreHolder.get(ctx).myRole() }.getOrNull()
        if (role != Role.MINOR) return
        applyBaseline(ctx)
        if (isDeviceOwner(ctx)) ctx.startForegroundService(Intent(ctx, EnforcerService::class.java))
        // Without Device Owner the VPN needs consent (the minor's screen asks for it).
        if (VpnService.prepare(ctx) == null) ctx.startService(Intent(ctx, DnsVpnService::class.java))
    }
}

/** Android 10+: the provisioning wizard asks for the mode; always "fully managed device". */
class ProvisioningModeActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setResult(
            RESULT_OK,
            Intent().putExtra(DevicePolicyManager.EXTRA_PROVISIONING_MODE, DevicePolicyManager.PROVISIONING_MODE_FULLY_MANAGED_DEVICE),
        )
        finish()
    }
}

class PolicyComplianceActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        DeviceOwnerPolicies.applyBaseline(this)
        setResult(RESULT_OK)
        finish()
    }
}

/** When the phone boots: policies, app suspension and DNS filter. */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action == Intent.ACTION_BOOT_COMPLETED) DeviceOwnerPolicies.startMinorProtection(context)
    }
}
