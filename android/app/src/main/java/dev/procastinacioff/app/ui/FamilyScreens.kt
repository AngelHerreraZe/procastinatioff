package dev.procastinacioff.app.ui

import android.app.Activity
import android.content.Intent
import android.net.VpnService
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.fragment.app.FragmentActivity
import dev.procastinacioff.app.core.CoreHolder
import dev.procastinacioff.app.core.userMessage
import dev.procastinacioff.app.family.DnsVpnService
import dev.procastinacioff.app.family.confirmAsGuardian
import dev.procastinacioff.app.sync.SyncScheduler
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.procastinacioff_core.Core
import uniffi.procastinacioff_core.DayState
import uniffi.procastinacioff_core.DeviceSeen
import uniffi.procastinacioff_core.FamilyAlert
import uniffi.procastinacioff_core.FamilyAlertView
import uniffi.procastinacioff_core.PendingRequest
import uniffi.procastinacioff_core.RequestKind
import uniffi.procastinacioff_core.Role
import uniffi.procastinacioff_core.Status
import uniffi.procastinacioff_core.TamperKind
import uniffi.procastinacioff_core.TimeOfDay

// Modo familiar. Regla de producto: el tutor no tiene pantallas con datos que el menor no vea.
// Ambos ven el mismo `MirrorPanel`; el tutor además aprueba solicitudes y cambia ajustes.

@Composable
private fun Screen(content: @Composable () -> Unit) =
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        content()
    }

@Composable
private fun Heading(text: String) = Text(text, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)

@Composable
private fun Section(title: String, content: @Composable () -> Unit) =
    Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface), modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(title, fontWeight = FontWeight.SemiBold)
            content()
        }
    }

private val WHEN = SimpleDateFormat("EEE d MMM, HH:mm", Locale("es"))
private fun whenText(ms: Long) = WHEN.format(Date(ms)).replaceFirstChar { it.uppercase() }

fun describe(what: RequestKind): String = when (what) {
    is RequestKind.ExtraPermit -> "${what.minutes} min más hoy"
    is RequestKind.AddSite -> "Permitir ${what.domain} en el enfoque"
    is RequestKind.LaterBedtime -> "Dormir ${what.minutes} min más tarde (cambia el horario)"
    is RequestKind.RestDay -> "Día de descanso hoy"
}

private fun alertText(a: FamilyAlert): String = when (a) {
    is FamilyAlert.Emergency -> "Se usó la emergencia"
    is FamilyAlert.BlockedSites -> "${a.count} ${if (a.count.toInt() == 1) "sitio bloqueado" else "sitios bloqueados"} por categoría (nunca cuáles)"
    is FamilyAlert.Tamper -> when (a.what) {
        TamperKind.PERMISSION_REVOKED -> "Se desactivó un permiso de protección"
        TamperKind.SERVICE_STOPPED -> "La protección estuvo detenida"
        TamperKind.CLOCK_CHANGED -> "Se cambió la hora del dispositivo"
        TamperKind.UNINSTALL_ATTEMPT -> "Se intentó desinstalar la app"
        TamperKind.SAFE_MODE -> "Se arrancó en modo seguro"
        TamperKind.DEVICE_SILENT -> "Un dispositivo dejó de reportar"
    }
}

private fun stateText(st: Status, now: Long): String = when (val s = st.state) {
    is DayState.Sleep -> "Durmiendo"
    is DayState.Locked -> if (st.needsPlan) "Aún sin plan del día" else "Bloqueado"
    is DayState.Pass -> "Permiso activo · ${minutesUntil(s.untilMs, now)} min"
    is DayState.Focus -> "En enfoque"
    is DayState.Free -> if (st.freeDay) "Día libre" else "Libre: metas cumplidas"
    is DayState.Emergency -> "Emergencia · ${minutesUntil(s.untilMs, now)} min"
}

// ── Estado de un perfil (el del menor), consultado cada 2 s ──

data class FamilySnapshot(
    val status: Status? = null,
    val pending: List<PendingRequest> = emptyList(),
    val alerts: List<FamilyAlertView> = emptyList(),
    val devices: List<DeviceSeen> = emptyList(),
    val nowMs: Long = System.currentTimeMillis(),
)

/** `sync`: el teléfono del tutor sincroniza cada 30 s mientras mira el panel. */
@Composable
fun rememberFamilySnapshot(core: Core, sync: Boolean, refresh: Int = 0): FamilySnapshot {
    var snap by remember(core) { mutableStateOf(FamilySnapshot()) }
    LaunchedEffect(core, refresh) {
        var lastSync = 0L
        while (true) {
            snap = withContext(Dispatchers.IO) {
                val clock = CoreHolder.clock()
                if (sync && System.currentTimeMillis() - lastSync > 30_000) {
                    lastSync = System.currentTimeMillis()
                    runCatching { core.sync(clock) }
                }
                runCatching {
                    FamilySnapshot(core.status(clock), core.pendingRequests(), core.familyAlerts(clock), core.devicesSeen(), clock.nowMs)
                }.getOrDefault(FamilySnapshot())
            }
            delay(2_000)
        }
    }
    return snap
}

/** Lo que ven por igual el tutor y el menor: estado, metas, permisos, dispositivos, solicitudes y avisos. */
@Composable
fun MirrorPanel(snap: FamilySnapshot) {
    val st = snap.status
    Section("Hoy") {
        if (st == null) {
            Text("Sin datos todavía. Se actualiza al sincronizar.")
            return@Section
        }
        Text(stateText(st, snap.nowMs), style = MaterialTheme.typography.titleMedium)
        st.goals.forEach { g -> Text("${if (g.completed) "✓" else "○"} ${g.title} (${g.doneCount}/${g.targetCount})") }
        if (!st.freeDay) Text("${st.permitsLeft} ${if (st.permitsLeft.toInt() == 1) "permiso disponible" else "permisos disponibles"}")
    }
    if (snap.devices.isNotEmpty()) {
        Section("Dispositivos") {
            snap.devices.forEach { d ->
                Text("${d.label} · ${if (d.lastSeenMs > 0) "visto ${hoursAgo(d.lastSeenMs, snap.nowMs)}" else "aún sin reportar"}")
            }
        }
    }
    if (snap.pending.isNotEmpty()) {
        Section("Solicitudes sin responder") {
            snap.pending.forEach { r -> Text("${describe(r.what)} · ${whenText(r.atMs)}") }
        }
    }
    Section("Avisos (últimos 7 días)") {
        if (snap.alerts.isEmpty()) Text("Ninguno.")
        snap.alerts.forEach { a -> Text("${alertText(a.alert)} · ${whenText(a.atMs)} · ${a.deviceLabel}") }
    }
    Text(
        "Nunca se ven mensajes, contactos, fotos, historial de navegación ni ubicación: no existen en el sistema.",
        style = MaterialTheme.typography.bodySmall,
    )
}

// ───────────────────────── Tutor ─────────────────────────

/** Consentimiento del tutor (07b · 5.1). ⚖️ Validar con la asesoría legal antes del piloto. */
@Composable
fun FamilyConsentScreen(onAccept: () -> Unit, onBack: () -> Unit) {
    val activity = LocalContext.current as FragmentActivity
    val scope = rememberCoroutineScope()
    Screen {
        Heading("Vas a crear el perfil de un menor")
        Section("Qué hace la app en el dispositivo del menor") {
            Text("• Bloquea apps y sitios web según el horario y las metas que tú definas.")
            Text("• Puede pedir pruebas de metas (por ejemplo, minutos de lectura). La cámara está APAGADA por defecto; solo funciona si tú la activas y el menor la acepta. Nunca se graba nada.")
        }
        Section("Qué verás tú") {
            Text("• Metas cumplidas, permisos usados, solicitudes, alertas de manipulación y emergencias.")
            Text("• El NÚMERO de sitios bloqueados por categoría en el día (nunca cuáles).")
        }
        Section("Qué NO verás nunca (no existe en el sistema)") {
            Text("• Mensajes, contactos, fotos, historial de navegación ni ubicación.")
        }
        Text("El menor ve exactamente lo mismo que tú. Puedes borrar el perfil y todos sus datos en cualquier momento.")
        Text("Declaro ser padre, madre o tutor legal del menor.", fontWeight = FontWeight.SemiBold)
        PrimaryButton("Confirmar con huella / PIN") {
            scope.launch { if (confirmAsGuardian(activity, "Crear el perfil de un menor")) onAccept() }
        }
        TextButton(onClick = onBack) { Text("Volver") }
    }
}

/** Lista de perfiles de menores que administro, con su estado del día. */
@Composable
fun FamilyListScreen(onOpen: (String) -> Unit, onNew: () -> Unit) {
    val ctx = LocalContext.current
    val profiles = remember { CoreHolder.familyProfiles(ctx) }
    Screen {
        Heading("Familia")
        if (profiles.isEmpty()) Text("Aún no hay perfiles de menores.")
        profiles.forEach { (id, name) ->
            val snap = rememberFamilySnapshot(CoreHolder.family(ctx, id), sync = false)
            Card(onClick = { onOpen(id) }, modifier = Modifier.fillMaxWidth()) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(name, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
                    snap.status?.let { st ->
                        val done = st.goals.count { it.completed }
                        Text("${stateText(st, snap.nowMs)} · $done de ${st.goals.size} metas")
                    }
                    if (snap.pending.isNotEmpty()) Text("${snap.pending.size} solicitudes esperando", color = MaterialTheme.colorScheme.primary)
                }
            }
        }
        SecondaryButton("Añadir un menor", onClick = onNew)
    }
}

/** Panel de un menor (tutor): el panel espejo + aprobar solicitudes, vincular y ajustes. */
@Composable
fun ChildScreen(id: String, onInvite: (Role) -> Unit, onSettings: () -> Unit, onBack: () -> Unit) {
    val ctx = LocalContext.current
    val activity = ctx as FragmentActivity
    val core = remember(id) { CoreHolder.family(ctx, id) }
    val name = remember(id) { CoreHolder.familyProfiles(ctx)[id].orEmpty() }
    var refresh by remember { mutableIntStateOf(0) }
    val snap = rememberFamilySnapshot(core, sync = true, refresh = refresh)
    val scope = rememberCoroutineScope()
    var error by remember { mutableStateOf<String?>(null) }

    fun answer(r: PendingRequest, approved: Boolean) = scope.launch {
        val verb = if (approved) "Aprobar" else "Rechazar"
        if (!confirmAsGuardian(activity, "$verb: ${describe(r.what)}")) return@launch
        error = withContext(Dispatchers.IO) {
            runCatching { core.decide(CoreHolder.clock(), r.requestId, approved); core.sync(CoreHolder.clock()) }
                .exceptionOrNull()?.userMessage()
        }
        refresh++
    }

    Screen {
        Heading(name)
        snap.pending.forEach { r ->
            Section("${describe(r.what)} · ${whenText(r.atMs)}") {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    PrimaryButton("Aprobar") { answer(r, true) }
                    SecondaryButton("Rechazar") { answer(r, false) }
                }
            }
        }
        ErrorText(error)
        Text("Lo que ves aquí es exactamente lo que ve $name en su app.", style = MaterialTheme.typography.bodySmall)
        MirrorPanel(snap)
        SecondaryButton("Vincular un dispositivo de $name") { onInvite(Role.MINOR) }
        SecondaryButton("Añadir otro tutor") { onInvite(Role.GUARDIAN) }
        SecondaryButton("Ajustes de $name", onClick = onSettings)
        TextButton(onClick = onBack) { Text("Volver") }
    }
}

/** Ajustes del menor (tutor). Rigen al momento: los firma un tutor. */
@Composable
fun ChildSettingsScreen(id: String, onBack: () -> Unit) {
    val ctx = LocalContext.current
    val activity = ctx as FragmentActivity
    val core = remember(id) { CoreHolder.family(ctx, id) }
    val cfg = remember(id) { core.activeConfig(CoreHolder.clock()) }
    val scope = rememberCoroutineScope()
    if (cfg == null) {
        Screen { Text("Sin configuración todavía."); TextButton(onClick = onBack) { Text("Volver") } }
        return
    }
    fun hm(t: TimeOfDay) = "%02d:%02d".format(t.minutes.toInt() / 60, t.minutes.toInt() % 60)
    var wake by remember { mutableStateOf(hm(cfg.schedule.wake)) }
    var bedtime by remember { mutableStateOf(hm(cfg.schedule.bedtime)) }
    var permits by remember { mutableStateOf(cfg.permitsPerDay.toString()) }
    var minutes by remember { mutableStateOf(cfg.permitMinutes.toString()) }
    var bonus by remember { mutableStateOf(cfg.weeklyBonusPermits.toString()) }
    var freeDays by remember { mutableIntStateOf(cfg.freeDays.toInt()) }
    var web by remember { mutableStateOf(cfg.webAllowlist.joinToString(", ")) }
    var error by remember { mutableStateOf<String?>(null) }
    val number = KeyboardOptions(keyboardType = KeyboardType.Number)

    Screen {
        Heading("Ajustes")
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            OutlinedTextField(wake, { wake = it.take(5) }, label = { Text("Despertar") }, isError = parseTime(wake) == null, modifier = Modifier.width(140.dp))
            OutlinedTextField(bedtime, { bedtime = it.take(5) }, label = { Text("Dormir") }, isError = parseTime(bedtime) == null, modifier = Modifier.width(140.dp))
        }
        Text("Días libres", fontWeight = FontWeight.SemiBold)
        Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            DAY_LABELS.forEachIndexed { i, d ->
                FilterChip(selected = freeDays and (1 shl i) != 0, onClick = { freeDays = freeDays xor (1 shl i) }, label = { Text(d) })
            }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
            OutlinedTextField(permits, { permits = it.filter(Char::isDigit).take(2) }, label = { Text("Permisos/día") }, keyboardOptions = number, modifier = Modifier.width(130.dp))
            OutlinedTextField(minutes, { minutes = it.filter(Char::isDigit).take(2) }, label = { Text("Minutos") }, keyboardOptions = number, modifier = Modifier.width(110.dp))
            OutlinedTextField(bonus, { bonus = it.filter(Char::isDigit).take(2) }, label = { Text("Extra/semana") }, keyboardOptions = number, modifier = Modifier.width(130.dp))
        }
        OutlinedTextField(web, { web = it }, label = { Text("Sitios permitidos en el enfoque") }, modifier = Modifier.fillMaxWidth())
        ErrorText(error)
        val wakeMin = parseTime(wake)
        val bedMin = parseTime(bedtime)
        PrimaryButton("Guardar (rige al momento)", enabled = wakeMin != null && bedMin != null) {
            val next = cfg.copy(
                schedule = cfg.schedule.copy(wake = TimeOfDay(wakeMin!!.toUShort()), bedtime = TimeOfDay(bedMin!!.toUShort())),
                permitsPerDay = (permits.toIntOrNull() ?: 0).coerceIn(0, 10).toUByte(),
                permitMinutes = (minutes.toIntOrNull() ?: 5).coerceIn(1, 60).toUShort(),
                weeklyBonusPermits = (bonus.toIntOrNull() ?: 0).coerceIn(0, 20).toUByte(),
                freeDays = freeDays.toUByte(),
                webAllowlist = web.split(',', ' ', '\n').map { it.trim().lowercase() }.filter { it.isNotEmpty() },
            )
            scope.launch {
                if (!confirmAsGuardian(activity, "Cambiar los ajustes")) return@launch
                error = withContext(Dispatchers.IO) {
                    runCatching { core.setConfig(CoreHolder.clock(), next); core.sync(CoreHolder.clock()) }.exceptionOrNull()?.userMessage()
                }
                if (error == null) onBack()
            }
        }
        TextButton(onClick = onBack) { Text("Volver") }
    }
}

// ───────────────────────── Menor ─────────────────────────

/** En el dispositivo del menor: pedir al tutor, activar el filtro web y "Lo que ve tu tutor". */
@Composable
fun TutorScreen(onBack: () -> Unit) {
    val ctx = LocalContext.current
    val core = remember { CoreHolder.get(ctx) }
    var refresh by remember { mutableIntStateOf(0) }
    val snap = rememberFamilySnapshot(core, sync = false, refresh = refresh)
    val scope = rememberCoroutineScope()
    var site by remember { mutableStateOf("") }
    var message by remember { mutableStateOf<String?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var vpnReady by remember { mutableStateOf(VpnService.prepare(ctx) == null) }
    val askVpn = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { r ->
        vpnReady = r.resultCode == Activity.RESULT_OK
        if (vpnReady) ctx.startService(Intent(ctx, DnsVpnService::class.java))
    }
    LaunchedEffect(vpnReady) { if (vpnReady) ctx.startService(Intent(ctx, DnsVpnService::class.java)) }

    fun ask(what: RequestKind) = scope.launch {
        error = withContext(Dispatchers.IO) {
            runCatching { core.request(CoreHolder.clock(), what) }.exceptionOrNull()?.userMessage()
        }
        if (error == null) {
            message = "Enviado: ${describe(what)}. Tu tutor recibe un aviso."
            site = ""
            SyncScheduler.syncNow(ctx)
            refresh++
        }
    }

    Screen {
        Heading("Tu tutor")
        if (!vpnReady) {
            Section("Falta activar el filtro web") {
                Text("Tu tutor lo configuró para este dispositivo. Solo revisa a qué sitios se conecta; no ve ni guarda nada.")
                PrimaryButton("Activar") { VpnService.prepare(ctx)?.let { askVpn.launch(it) } }
            }
        }
        Section("Pedir algo") {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                SecondaryButton("15 min más") { ask(RequestKind.ExtraPermit(15u)) }
                SecondaryButton("Día de descanso") { ask(RequestKind.RestDay) }
            }
            SecondaryButton("Dormir 30 min más tarde") { ask(RequestKind.LaterBedtime(30u)) }
            OutlinedTextField(site, { site = it.take(253) }, label = { Text("Un sitio para tus tareas") }, placeholder = { Text("khanacademy.org") }, modifier = Modifier.fillMaxWidth())
            SecondaryButton("Pedir acceso", enabled = site.isNotBlank()) { ask(RequestKind.AddSite(site.trim())) }
            message?.let { Text(it) }
            ErrorText(error)
        }
        Text("Lo que ve tu tutor", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
        Text("Tu tutor ve exactamente esto, ni más ni menos.", style = MaterialTheme.typography.bodySmall)
        MirrorPanel(snap)
        TextButton(onClick = onBack) { Text("Volver") }
    }
}
