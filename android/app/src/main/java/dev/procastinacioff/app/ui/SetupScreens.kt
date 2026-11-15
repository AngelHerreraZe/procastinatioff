package dev.procastinacioff.app.ui

import androidx.compose.ui.res.stringResource
import dev.procastinacioff.app.R
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

@Composable
private fun Page(content: @Composable () -> Unit) {
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) { content() }
}

@Composable
fun WelcomeScreen(onNext: () -> Unit) = Page {
    Text(stringResource(R.string.welcome_title), style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.Bold)
    Text(stringResource(R.string.welcome_lead))
    listOf(R.string.welcome_point_accounts, R.string.welcome_point_encrypted, R.string.welcome_point_no_ads, R.string.welcome_point_calls)
        .forEach { Text("• " + stringResource(it), color = MaterialTheme.colorScheme.onSurfaceVariant) }
    PrimaryButton(stringResource(R.string.action_start), onClick = onNext)
}

/** Each permission is explained before asking for it. Nothing is enabled without the user seeing it. */
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

    LifecycleResumeEffect(Unit) { // recomputed when coming back from Settings
        a11y = isAccessibilityEnabled(ctx)
        dnd = nm.isNotificationPolicyAccessGranted
        isAdmin = dpm.isAdminActive(admin)
        onPauseOrDispose { }
    }

    Page {
        Text(stringResource(R.string.permissions_title), style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
        PermissionRow(
            stringResource(R.string.perm_accessibility),
            stringResource(R.string.perm_accessibility_why),
            a11y,
        ) { ctx.startActivity(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS)) }
        PermissionRow(stringResource(R.string.perm_dnd), stringResource(R.string.perm_dnd_why), dnd) {
            ctx.startActivity(Intent(Settings.ACTION_NOTIFICATION_POLICY_ACCESS_SETTINGS))
        }
        val adminExplanation = stringResource(R.string.perm_admin_explanation)
        PermissionRow(
            stringResource(R.string.perm_admin),
            stringResource(R.string.perm_admin_why),
            isAdmin,
        ) {
            ctx.startActivity(
                Intent(DevicePolicyManager.ACTION_ADD_DEVICE_ADMIN)
                    .putExtra(DevicePolicyManager.EXTRA_DEVICE_ADMIN, admin)
                    .putExtra(DevicePolicyManager.EXTRA_ADD_EXPLANATION, adminExplanation),
            )
        }
        PermissionRow(stringResource(R.string.perm_notifications), stringResource(R.string.perm_notifications_why), false) {
            notifications.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
        PrimaryButton(stringResource(R.string.action_continue), enabled = a11y && dnd && isAdmin, onClick = onNext)
    }
}

@Composable
private fun PermissionRow(title: String, why: String, granted: Boolean, onGrant: () -> Unit) {
    Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface)) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(if (granted) stringResource(R.string.perm_granted, title) else title, style = MaterialTheme.typography.titleMedium)
            Text(why, color = MaterialTheme.colorScheme.onSurfaceVariant)
            if (!granted) TextButton(onClick = onGrant) { Text(stringResource(R.string.perm_grant)) }
        }
    }
}

@Composable
fun ChooseScreen(onNew: () -> Unit, onJoin: () -> Unit, onWatch: () -> Unit, onFamily: () -> Unit) = Page {
    Text(stringResource(R.string.choose_title), style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
    PrimaryButton(stringResource(R.string.choose_create), onClick = onNew)
    SecondaryButton(stringResource(R.string.choose_join), onClick = onJoin)
    SecondaryButton(stringResource(R.string.choose_partner), onClick = onWatch)
    SecondaryButton(stringResource(R.string.choose_guardian), onClick = onFamily)
}

data class LaunchableApp(val label: String, val pkg: String, val id: String)

/** Apps with a launcher icon, with their "android:<pkg>@<cert>" identifier. */
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
    val defaultLabel = stringResource(if (family) R.string.default_label_guardian else R.string.default_label_phone)
    var label by remember { mutableStateOf(defaultLabel) }
    var child by remember { mutableStateOf("") }
    var bonus by remember { mutableStateOf("0") }
    var wake by remember { mutableStateOf("06:30") }
    var bedtime by remember { mutableStateOf("22:30") }
    var freeDays by remember { mutableIntStateOf(0b110_0000) } // Saturday and Sunday
    var permits by remember { mutableStateOf("3") }
    var permitMinutes by remember { mutableStateOf("5") }
    val readTitle = stringResource(R.string.example_goal_read)
    val eatTitle = stringResource(R.string.example_goal_eat)
    val goals = remember {
        mutableStateListOf(
            Goal("read-30", readTitle, GoalKind.KEY, 30u, 1u, methodsFor(30), ALL_DAYS),
            Goal("eat-healthy", eatTitle, GoalKind.HABIT, 0u, 2u, methodsFor(0), ALL_DAYS),
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
        apps.firstOrNull { it.pkg == "com.whatsapp" }?.let { roles[it.id] = AppRole.ESSENTIAL } // preselected
    }

    Page {
        if (family) {
            Text(stringResource(R.string.config_minor_title), style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
            OutlinedTextField(child, { child = it.take(64) }, label = { Text(stringResource(R.string.config_minor_name)) }, modifier = Modifier.fillMaxWidth())
            OutlinedTextField(label, { label = it.take(64) }, label = { Text(stringResource(R.string.config_guardian_name)) }, modifier = Modifier.fillMaxWidth())
        } else {
            Text(stringResource(R.string.config_title), style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
            OutlinedTextField(label, { label = it.take(64) }, label = { Text(stringResource(R.string.config_phone_name)) }, modifier = Modifier.fillMaxWidth())
        }

        Text(stringResource(R.string.schedule), style = MaterialTheme.typography.titleMedium)
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            OutlinedTextField(wake, { wake = it.take(5) }, label = { Text(stringResource(R.string.wake)) }, isError = parseTime(wake) == null, modifier = Modifier.width(140.dp))
            OutlinedTextField(bedtime, { bedtime = it.take(5) }, label = { Text(stringResource(R.string.bedtime)) }, isError = parseTime(bedtime) == null, modifier = Modifier.width(140.dp))
        }
        Text(stringResource(R.string.free_days_hint), color = MaterialTheme.colorScheme.onSurfaceVariant)
        Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            dayLabels().forEachIndexed { i, d ->
                FilterChip(selected = freeDays and (1 shl i) != 0, onClick = { freeDays = freeDays xor (1 shl i) }, label = { Text(d) })
            }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            OutlinedTextField(permits, { permits = it.filter(Char::isDigit).take(2) }, label = { Text(stringResource(R.string.passes_per_day)) },
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number), modifier = Modifier.width(150.dp))
            OutlinedTextField(permitMinutes, { permitMinutes = it.filter(Char::isDigit).take(2) }, label = { Text(stringResource(R.string.minutes)) },
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number), modifier = Modifier.width(120.dp))
        }
        if (family) {
            OutlinedTextField(bonus, { bonus = it.filter(Char::isDigit).take(2) }, label = { Text(stringResource(R.string.weekly_bonus)) },
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number), modifier = Modifier.fillMaxWidth())
            Text(stringResource(R.string.guardian_rules_note), style = MaterialTheme.typography.bodySmall)
        } else {
            Text(stringResource(R.string.anti_impulse_note), style = MaterialTheme.typography.bodySmall)
        }

        Text(stringResource(R.string.fixed_goals), style = MaterialTheme.typography.titleMedium)
        Text(stringResource(R.string.fixed_goals_hint), color = MaterialTheme.colorScheme.onSurfaceVariant)
        goals.forEachIndexed { i, g ->
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(g.title, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
                KindChip(g.kind)
                TextButton(onClick = { goals.removeAt(i) }) { Text(stringResource(R.string.remove)) }
            }
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedTextField(newGoal, { newGoal = it.take(80) }, label = { Text(stringResource(R.string.new_goal)) }, modifier = Modifier.weight(1f))
            OutlinedTextField(newMinutes, { newMinutes = it.filter(Char::isDigit).take(3) }, label = { Text(stringResource(R.string.min_short)) },
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number), modifier = Modifier.width(90.dp))
        }
        SecondaryButton(stringResource(R.string.add_goal), enabled = newGoal.isNotBlank()) {
            val m = (newMinutes.toIntOrNull() ?: 0).coerceIn(0, 600)
            goals += Goal(slug(newGoal, goals.map { it.id }), newGoal.trim(), GoalKind.KEY, m.toUShort(), 1u, methodsFor(m), ALL_DAYS)
            newGoal = ""
        }

        Text(stringResource(R.string.apps), style = MaterialTheme.typography.titleMedium)
        Text(stringResource(R.string.apps_hint),
            color = MaterialTheme.colorScheme.onSurfaceVariant)
        apps.forEach { app ->
            val role = roles[app.id] ?: AppRole.NONE
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(app.label, modifier = Modifier.weight(1f))
                FilterChip(selected = role == AppRole.ESSENTIAL, onClick = { roles[app.id] = if (role == AppRole.ESSENTIAL) AppRole.NONE else AppRole.ESSENTIAL }, label = { Text(stringResource(R.string.app_essential)) })
                FilterChip(selected = role == AppRole.FOCUS, onClick = { roles[app.id] = if (role == AppRole.FOCUS) AppRole.NONE else AppRole.FOCUS }, label = { Text(stringResource(R.string.check_focus)) })
            }
        }
        OutlinedTextField(web, { web = it }, label = { Text(stringResource(R.string.focus_sites_pc)) }, modifier = Modifier.fillMaxWidth())
        OutlinedTextField(relay, { relay = it.trim() }, label = { Text(stringResource(if (family) R.string.sync_server else R.string.sync_server_optional)) },
            placeholder = { Text("https://…") }, modifier = Modifier.fillMaxWidth())

        ErrorText(error)
        val wakeMin = parseTime(wake)
        val bedMin = parseTime(bedtime)
        val ready = !busy && wakeMin != null && bedMin != null && goals.any { it.kind == GoalKind.KEY } &&
            (!family || (child.isNotBlank() && relay.isNotEmpty()))
        PrimaryButton(stringResource(R.string.create_profile), enabled = ready) {
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
                        if (familyId != null) core.createFamilyProfile(clock, label.ifBlank { defaultLabel }, config)
                        else core.createProfile(clock, label.ifBlank { defaultLabel }, config)
                        familyId?.let { CoreHolder.saveFamilyProfile(ctx, it, child.trim()) }
                        if (relay.isNotEmpty()) {
                            core.connectRelay(relay)
                            core.sync(clock)
                            ctx.getSharedPreferences("push", Context.MODE_PRIVATE).getString("fcm", null)?.let { core.registerPush("fcm", it) }
                        }
                    }.exceptionOrNull()?.userMessage(ctx)
                }
                busy = false
                if (error == null) onDone()
            }
        }
    }
}
