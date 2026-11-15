package dev.procastinacioff.app.ui

import android.Manifest
import android.app.NotificationManager
import android.app.admin.DevicePolicyManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.provider.Settings
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
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateMapOf
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
import androidx.lifecycle.compose.LifecycleResumeEffect
import dev.procastinacioff.app.block.AdminReceiver
import dev.procastinacioff.app.block.AppIdentity
import dev.procastinacioff.app.core.CoreHolder
import dev.procastinacioff.app.core.userMessage
import dev.procastinacioff.app.sync.isAccessibilityEnabled
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.procastinacioff_core.Config
import uniffi.procastinacioff_core.Goal
import uniffi.procastinacioff_core.GoalKind
import uniffi.procastinacioff_core.Schedule
import uniffi.procastinacioff_core.TimeOfDay

private const val ALL_DAYS: UByte = 0x7Fu
internal val DAY_LABELS = listOf("L", "M", "X", "J", "V", "S", "D")

@Composable
private fun Page(content: @Composable () -> Unit) {
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) { content() }
}

@Composable
fun WelcomeScreen(onNext: () -> Unit) = Page {
    Text("Tus metas primero", style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.Bold)
    Text("ProcastinaciOFF bloquea lo que te distrae hasta que cumples lo que tú decides cada mañana.")
    listOf(
        "Sin cuentas: tu identidad es una clave que vive en este teléfono.",
        "Tus metas viajan cifradas: ni el servidor ni nosotros podemos leerlas.",
        "Sin anuncios, sin analítica y sin leer tus mensajes.",
        "Las llamadas y las emergencias siempre funcionan.",
    ).forEach { Text("• $it", color = MaterialTheme.colorScheme.onSurfaceVariant) }
    PrimaryButton("Empezar", onClick = onNext)
}

/** Cada permiso se explica antes de pedirlo. Nada se activa sin que el usuario lo vea. */
@Composable
fun PermissionsScreen(onNext: () -> Unit) {
    val ctx = LocalContext.current
    val dpm = ctx.getSystemService(DevicePolicyManager::class.java)
    val nm = ctx.getSystemService(NotificationManager::class.java)
    val admin = ComponentName(ctx, AdminReceiver::class.java)
    var a11y by remember { mutableStateOf(false) }
    var dnd by remember { mutableStateOf(false) }
    var isAdmin by remember { mutableStateOf(false) }
    val notifications = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {}

    LifecycleResumeEffect(Unit) { // al volver de Ajustes se recalcula
        a11y = isAccessibilityEnabled(ctx)
        dnd = nm.isNotificationPolicyAccessGranted
        isAdmin = dpm.isAdminActive(admin)
        onPauseOrDispose { }
    }

    Page {
        Text("Permisos necesarios", style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
        PermissionRow(
            "Accesibilidad",
            "Para saber qué app está abierta (solo su nombre) y mostrar la pantalla de bloqueo. No leemos la pantalla ni lo que escribes.",
            a11y,
        ) { ctx.startActivity(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS)) }
        PermissionRow("No molestar", "Para silenciar todo excepto las llamadas a la hora de dormir.", dnd) {
            ctx.startActivity(Intent(Settings.ACTION_NOTIFICATION_POLICY_ACCESS_SETTINGS))
        }
        PermissionRow(
            "Protección contra desinstalación",
            "Para que no puedas desinstalar la app en un impulso. Puedes desactivarla, pero quedará registrado.",
            isAdmin,
        ) {
            ctx.startActivity(
                Intent(DevicePolicyManager.ACTION_ADD_DEVICE_ADMIN)
                    .putExtra(DevicePolicyManager.EXTRA_DEVICE_ADMIN, admin)
                    .putExtra(DevicePolicyManager.EXTRA_ADD_EXPLANATION, "Evita desinstalaciones impulsivas."),
            )
        }
        PermissionRow("Notificaciones", "Para avisarte antes de bloquear y cuando tu otro dispositivo te necesita.", false) {
            notifications.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
        PrimaryButton("Continuar", enabled = a11y && dnd && isAdmin, onClick = onNext)
    }
}

@Composable
private fun PermissionRow(title: String, why: String, granted: Boolean, onGrant: () -> Unit) {
    Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface)) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(if (granted) "$title · concedido" else title, style = MaterialTheme.typography.titleMedium)
            Text(why, color = MaterialTheme.colorScheme.onSurfaceVariant)
            if (!granted) TextButton(onClick = onGrant) { Text("Conceder") }
        }
    }
}

@Composable
fun ChooseScreen(onNew: () -> Unit, onJoin: () -> Unit, onWatch: () -> Unit, onFamily: () -> Unit) = Page {
    Text("¿Cómo quieres empezar?", style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
    PrimaryButton("Crear mi perfil", onClick = onNew)
    SecondaryButton("Unirme a un perfil existente (otro dispositivo o el de un menor)", onClick = onJoin)
    SecondaryButton("Soy compañero de alguien", onClick = onWatch)
    SecondaryButton("Soy madre, padre o tutor", onClick = onFamily)
}

data class LaunchableApp(val label: String, val pkg: String, val id: String)

/** Apps con icono en el lanzador, con su identificador "android:<pkg>@<cert>". */
fun launchableApps(ctx: Context): List<LaunchableApp> {
    val pm = ctx.packageManager
    val intent = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER)
    return pm.queryIntentActivities(intent, 0)
        .mapNotNull { ri ->
            val pkg = ri.activityInfo.packageName
            if (pkg == ctx.packageName) null
            else AppIdentity.idOf(ctx, pkg)?.let { LaunchableApp(ri.loadLabel(pm).toString(), pkg, it) }
        }
        .distinctBy { it.pkg }
        .sortedBy { it.label.lowercase() }
}

internal fun parseTime(s: String): Int? {
    val m = Regex("^(\\d{1,2}):(\\d{2})$").find(s.trim()) ?: return null
    val (h, min) = m.destructured
    return if (h.toInt() in 0..23 && min.toInt() in 0..59) h.toInt() * 60 + min.toInt() else null
}

private enum class AppRole { NONE, ESSENTIAL, FOCUS }

@Composable
fun ConfigScreen(family: Boolean = false, onDone: () -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var label by remember { mutableStateOf(if (family) "Tutor" else "Teléfono") }
    var child by remember { mutableStateOf("") }
    var bonus by remember { mutableStateOf("0") }
    var wake by remember { mutableStateOf("06:30") }
    var bedtime by remember { mutableStateOf("22:30") }
    var freeDays by remember { mutableIntStateOf(0b110_0000) } // sábado y domingo
    var permits by remember { mutableStateOf("3") }
    var permitMinutes by remember { mutableStateOf("5") }
    val goals = remember {
        mutableStateListOf(
            Goal("leer-30", "Leer 30 minutos", GoalKind.KEY, 30u, 1u, methodsFor(30), ALL_DAYS),
            Goal("comer-sano", "Comer sano", GoalKind.HABIT, 0u, 2u, methodsFor(0), ALL_DAYS),
        )
    }
    var newGoal by remember { mutableStateOf("") }
    var newMinutes by remember { mutableStateOf("30") }
    var web by remember { mutableStateOf("udemy.com, coursera.org") }
    var relay by remember { mutableStateOf("") }
    var apps by remember { mutableStateOf<List<LaunchableApp>>(emptyList()) }
    val roles = remember { mutableStateMapOf<String, AppRole>() }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }

    LaunchedEffect(Unit) {
        apps = withContext(Dispatchers.IO) { launchableApps(ctx) }
        apps.firstOrNull { it.pkg == "com.whatsapp" }?.let { roles[it.id] = AppRole.ESSENTIAL } // preseleccionada
    }

    Page {
        if (family) {
            Text("Perfil del menor", style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
            OutlinedTextField(child, { child = it.take(64) }, label = { Text("Nombre del menor (solo en este teléfono)") }, modifier = Modifier.fillMaxWidth())
            OutlinedTextField(label, { label = it.take(64) }, label = { Text("Tu nombre (lo verá el menor)") }, modifier = Modifier.fillMaxWidth())
        } else {
            Text("Tu perfil", style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
            OutlinedTextField(label, { label = it.take(64) }, label = { Text("Nombre de este teléfono") }, modifier = Modifier.fillMaxWidth())
        }

        Text("Horario", style = MaterialTheme.typography.titleMedium)
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            OutlinedTextField(wake, { wake = it.take(5) }, label = { Text("Despertar") }, isError = parseTime(wake) == null, modifier = Modifier.width(140.dp))
            OutlinedTextField(bedtime, { bedtime = it.take(5) }, label = { Text("Dormir") }, isError = parseTime(bedtime) == null, modifier = Modifier.width(140.dp))
        }
        Text("Días libres: sin metas ni bloqueo, y sin hora de dormir la noche anterior.", color = MaterialTheme.colorScheme.onSurfaceVariant)
        Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            DAY_LABELS.forEachIndexed { i, d ->
                FilterChip(selected = freeDays and (1 shl i) != 0, onClick = { freeDays = freeDays xor (1 shl i) }, label = { Text(d) })
            }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            OutlinedTextField(permits, { permits = it.filter(Char::isDigit).take(2) }, label = { Text("Permisos al día") },
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number), modifier = Modifier.width(150.dp))
            OutlinedTextField(permitMinutes, { permitMinutes = it.filter(Char::isDigit).take(2) }, label = { Text("Minutos") },
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number), modifier = Modifier.width(120.dp))
        }
        if (family) {
            OutlinedTextField(bonus, { bonus = it.filter(Char::isDigit).take(2) }, label = { Text("Permisos extra a la semana (los usa cuando quiera)") },
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number), modifier = Modifier.fillMaxWidth())
            Text("Tú podrás cambiar estas reglas cuando quieras; el menor solo puede endurecerlas o pedirte cambios.", style = MaterialTheme.typography.bodySmall)
        } else {
            Text("Relajar estas reglas después solo rige desde el día siguiente; endurecerlas, al momento.", style = MaterialTheme.typography.bodySmall)
        }

        Text("Metas fijas", style = MaterialTheme.typography.titleMedium)
        Text("Aparecen cada mañana en el plan del día. Allí podrás añadir más.", color = MaterialTheme.colorScheme.onSurfaceVariant)
        goals.forEachIndexed { i, g ->
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(g.title, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
                KindChip(g.kind)
                TextButton(onClick = { goals.removeAt(i) }) { Text("Quitar") }
            }
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedTextField(newGoal, { newGoal = it.take(80) }, label = { Text("Nueva meta") }, modifier = Modifier.weight(1f))
            OutlinedTextField(newMinutes, { newMinutes = it.filter(Char::isDigit).take(3) }, label = { Text("Min") },
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number), modifier = Modifier.width(90.dp))
        }
        SecondaryButton("Añadir meta", enabled = newGoal.isNotBlank()) {
            val m = (newMinutes.toIntOrNull() ?: 0).coerceIn(0, 600)
            goals += Goal(slug(newGoal, goals.map { it.id }), newGoal.trim(), GoalKind.KEY, m.toUShort(), 1u, methodsFor(m), ALL_DAYS)
            newGoal = ""
        }

        Text("Apps", style = MaterialTheme.typography.titleMedium)
        Text("Esenciales: siempre disponibles (salvo al dormir). Enfoque: solo durante una sesión de enfoque. Teléfono y SMS ya están incluidos.",
            color = MaterialTheme.colorScheme.onSurfaceVariant)
        apps.forEach { app ->
            val role = roles[app.id] ?: AppRole.NONE
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(app.label, modifier = Modifier.weight(1f))
                FilterChip(selected = role == AppRole.ESSENTIAL, onClick = { roles[app.id] = if (role == AppRole.ESSENTIAL) AppRole.NONE else AppRole.ESSENTIAL }, label = { Text("Esencial") })
                FilterChip(selected = role == AppRole.FOCUS, onClick = { roles[app.id] = if (role == AppRole.FOCUS) AppRole.NONE else AppRole.FOCUS }, label = { Text("Enfoque") })
            }
        }
        OutlinedTextField(web, { web = it }, label = { Text("Sitios permitidos en el enfoque (PC)") }, modifier = Modifier.fillMaxWidth())
        OutlinedTextField(relay, { relay = it.trim() }, label = { Text(if (family) "Servidor de sincronización" else "Servidor de sincronización (opcional)") },
            placeholder = { Text("https://…") }, modifier = Modifier.fillMaxWidth())

        ErrorText(error)
        val wakeMin = parseTime(wake)
        val bedMin = parseTime(bedtime)
        val ready = !busy && wakeMin != null && bedMin != null && goals.any { it.kind == GoalKind.KEY } &&
            (!family || (child.isNotBlank() && relay.isNotEmpty()))
        PrimaryButton("Crear perfil", enabled = ready) {
            busy = true
            val config = Config(
                schedule = Schedule(wake = TimeOfDay(wakeMin!!.toUShort()), bedtime = TimeOfDay(bedMin!!.toUShort())),
                permitsPerDay = (permits.toIntOrNull() ?: 3).coerceIn(0, 10).toUByte(),
                permitMinutes = (permitMinutes.toIntOrNull() ?: 5).coerceIn(1, 60).toUShort(),
                permitCooldownMinutes = 60u,
                emergencyMinutes = 15u,
                emergencyPenalty = 1u,
                habitPenalty = true,
                goals = goals.toList(),
                freeDays = freeDays.toUByte(),
                essentialApps = roles.filterValues { it == AppRole.ESSENTIAL }.keys.toList(),
                focusApps = roles.filterValues { it == AppRole.FOCUS }.keys.toList(),
                webAllowlist = web.split(',', ' ', '\n').map { it.trim().lowercase() }.filter { it.isNotEmpty() },
                weeklyBonusPermits = if (family) (bonus.toIntOrNull() ?: 0).coerceIn(0, 20).toUByte() else 0u,
            )
            scope.launch {
                error = withContext(Dispatchers.IO) {
                    runCatching {
                        val clock = CoreHolder.clock()
                        val familyId = if (family) CoreHolder.newFamilyId() else null
                        val core = familyId?.let { CoreHolder.family(ctx, it) } ?: CoreHolder.get(ctx)
                        if (familyId != null) core.createFamilyProfile(clock, label.ifBlank { "Tutor" }, config)
                        else core.createProfile(clock, label.ifBlank { "Teléfono" }, config)
                        familyId?.let { CoreHolder.saveFamilyProfile(ctx, it, child.trim()) }
                        if (relay.isNotEmpty()) {
                            core.connectRelay(relay)
                            core.sync(clock)
                            ctx.getSharedPreferences("push", Context.MODE_PRIVATE).getString("fcm", null)?.let { core.registerPush("fcm", it) }
                        }
                    }.exceptionOrNull()?.userMessage()
                }
                busy = false
                if (error == null) onDone()
            }
        }
    }
}
