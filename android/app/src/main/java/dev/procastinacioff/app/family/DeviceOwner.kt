package dev.procastinacioff.app.family

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
import dev.procastinacioff.app.block.AdminReceiver
import dev.procastinacioff.app.core.CoreHolder
import uniffi.procastinacioff_core.Role

/**
 * 🔒 Teléfono del menor como Device Owner (07 · 3). Se configura desde cero con el QR de
 * aprovisionamiento: el menor no puede restablecerlo, arrancar en modo seguro, cambiar la hora,
 * añadir VPN ni usuarios, depurar por USB ni desinstalar la app.
 */
object DeviceOwnerPolicies {
    fun admin(ctx: Context) = ComponentName(ctx, AdminReceiver::class.java)

    fun isDeviceOwner(ctx: Context) =
        ctx.getSystemService(DevicePolicyManager::class.java).isDeviceOwnerApp(ctx.packageName)

    /** Se aplica al aprovisionar y en cada arranque (idempotente). */
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
            UserManager.DISALLOW_DEBUGGING_FEATURES, // sin ADB para el menor
            UserManager.DISALLOW_USB_FILE_TRANSFER,
        ).forEach { dpm.addUserRestriction(admin, it) }
        // Hora de la red, no editable
        if (Build.VERSION.SDK_INT >= 30) dpm.setAutoTimeEnabled(admin, true) else @Suppress("DEPRECATION") dpm.setAutoTimeRequired(admin, true)
        dpm.setUninstallBlocked(admin, ctx.packageName, true)
        dpm.setUninstallBlocked(admin, "dev.procastinacioff.verifier", true)
        // VPN local de DNS siempre activa, sin "lockdown": solo enruta el DNS y el resto del tráfico
        // sale por la red normal. DISALLOW_CONFIG_VPN impide quitarla.
        runCatching { dpm.setAlwaysOnVpnPackage(admin, ctx.packageName, false) }
    }

    /**
     * Chrome gestionado (Device Owner): sin incógnito ni DNS cifrado; en ENFOQUE, solo la lista blanca.
     * ⚠️ El tipo de cada clave lo fija el esquema de Chrome: comprobarlo en dispositivo con
     * RestrictionsManager.getManifestRestrictions("com.android.chrome") (pruebas de la Fase 3).
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

    /** En el dispositivo de un menor: protección por suspensión (Device Owner) y filtro DNS. */
    fun startMinorProtection(ctx: Context) {
        val role = runCatching { CoreHolder.get(ctx).myRole() }.getOrNull()
        if (role != Role.MINOR) return
        applyBaseline(ctx)
        if (isDeviceOwner(ctx)) ctx.startForegroundService(Intent(ctx, EnforcerService::class.java))
        // Sin Device Owner la VPN necesita el consentimiento (la pantalla del menor lo pide).
        if (VpnService.prepare(ctx) == null) ctx.startService(Intent(ctx, DnsVpnService::class.java))
    }
}

/** Android 10+: el asistente de aprovisionamiento pregunta el modo; siempre "dispositivo administrado". */
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

/** Al encender el teléfono: políticas, suspensión de apps y filtro DNS. */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action == Intent.ACTION_BOOT_COMPLETED) DeviceOwnerPolicies.startMinorProtection(context)
    }
}
