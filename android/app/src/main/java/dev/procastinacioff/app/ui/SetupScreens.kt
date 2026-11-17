package dev.procastinacioff.app.ui

import androidx.compose.ui.res.stringResource
import dev.procastinacioff.app.R
import android.Manifest
import android.app.NotificationManager
import android.app.admin.DevicePolicyManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.os.Build
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
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
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.core.app.NotificationManagerCompat
import androidx.lifecycle.compose.LifecycleResumeEffect
import dev.procastinacioff.app.block.AdminReceiver
import dev.procastinacioff.app.block.AppIdentity
import dev.procastinacioff.app.core.CoreHolder
import dev.procastinacioff.app.core.userMessage
import dev.procastinacioff.app.sync.isAccessibilityEnabled
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.procastinacioff_core.AuthPolicy
import uniffi.procastinacioff_core.Config
import uniffi.procastinacioff_core.Goal
import uniffi.procastinacioff_core.GoalKind
import uniffi.procastinacioff_core.Schedule
import uniffi.procastinacioff_core.TimeOfDay

private const val ALL_DAYS: UByte = 0x7Fu

@Composable
fun WelcomeScreen(onNext: () -> Unit) = Screen(
    bottom = {
        PrimaryButton(stringResource(R.string.action_start)) { onNext() }
        Text(
            stringResource(R.string.welcome_tagline), Modifier.padding(top = 8.dp),
            style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    },
) {
    Spacer(Modifier.height(16.dp))
    Wordmark()
    Text(stringResource(R.string.welcome_title), Modifier.semantics { heading() }, style = MaterialTheme.typography.displaySmall)
    Text(stringResource(R.string.welcome_lead), style = MaterialTheme.typography.bodyLarge)
    Spacer(Modifier.height(8.dp))
    Feature(PcoIcons.User, R.string.welcome_identity_title, R.string.welcome_identity_body)
    Feature(PcoIcons.ShieldLock, R.string.welcome_encrypted_title, R.string.welcome_encrypted_body)
    Feature(PcoIcons.EyeOff, R.string.welcome_privacy_title, R.string.welcome_privacy_body)
    Feature(PcoIcons.Phone, R.string.welcome_calls_title, R.string.welcome_calls_body)
}

@Composable
private fun Feature(icon: ImageVector, title: Int, body: Int) {
    Row(Modifier.semantics(mergeDescendants = true) {}, horizontalArrangement = Arrangement.spacedBy(16.dp), verticalAlignment = Alignment.CenterVertically) {
        IconCircle(icon, 56.dp)
        Column(Modifier.weight(1f)) {
            Text(stringResource(title), style = MaterialTheme.typography.titleMedium)
            Text(stringResource(body), style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

/**
 * Each permission is explained before asking for it. Nothing is enabled without the user seeing it.
 * `onboarding`: shows the step; from Home (blocking paused) the same screen is a checklist.
 */
@Composable
fun PermissionsScreen(onboarding: Boolean, onNext: () -> Unit, onBack: () -> Unit) {
    val ctx = LocalContext.current
    val dpm = ctx.getSystemService(DevicePolicyManager::class.java)
    val nm = ctx.getSystemService(NotificationManager::class.java)
    val admin = ComponentName(ctx, AdminReceiver::class.java)
    var a11y by remember { mutableStateOf(false) }
    var dnd by remember { mutableStateOf(false) }
    var isAdmin by remember { mutableStateOf(false) }
    var notify by remember { mutableStateOf(false) }
    val notifications = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { notify = it }

    LifecycleResumeEffect(Unit) { // recomputed when coming back from Settings
        a11y = isAccessibilityEnabled(ctx)
        dnd = nm.isNotificationPolicyAccessGranted
        isAdmin = dpm.isAdminActive(admin)
        notify = NotificationManagerCompat.from(ctx).areNotificationsEnabled()
        onPauseOrDispose { }
    }
    val ready = a11y && dnd && isAdmin

    Screen(
        title = stringResource(R.string.permissions_title),
        lead = stringResource(R.string.permissions_lead),
        onBack = onBack,
        step = if (onboarding) stringResource(R.string.step_of, 1, 2) else null,
        bottom = {
            PrimaryButton(stringResource(R.string.action_continue), enabled = ready) { onNext() }
            // Blocking stays paused until Accessibility is on; Home says so and links back here.
            if (!ready) TextButton(onClick = onNext) { Text(stringResource(R.string.setup_later)) }
            Footnote(stringResource(R.string.calls_stay_available))
        },
    ) {
        PermissionCard(PcoIcons.Accessibility, stringResource(R.string.perm_accessibility), stringResource(R.string.perm_accessibility_why), a11y) {
            ctx.startActivity(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS))
        }
        PermissionCard(PcoIcons.BellOff, stringResource(R.string.perm_dnd), stringResource(R.string.perm_dnd_why), dnd) {
            ctx.startActivity(Intent(Settings.ACTION_NOTIFICATION_POLICY_ACCESS_SETTINGS))
        }
        val adminExplanation = stringResource(R.string.perm_admin_explanation)
        PermissionCard(PcoIcons.ShieldLock, stringResource(R.string.perm_admin), stringResource(R.string.perm_admin_why), isAdmin) {
            ctx.startActivity(
                Intent(DevicePolicyManager.ACTION_ADD_DEVICE_ADMIN)
                    .putExtra(DevicePolicyManager.EXTRA_DEVICE_ADMIN, admin)
                    .putExtra(DevicePolicyManager.EXTRA_ADD_EXPLANATION, adminExplanation),
            )
        }
        PermissionCard(PcoIcons.Bell, stringResource(R.string.perm_notifications), stringResource(R.string.perm_notifications_why), notify) {
            if (Build.VERSION.SDK_INT >= 33) notifications.launch(Manifest.permission.POST_NOTIFICATIONS)
            else ctx.startActivity(Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS).putExtra(Settings.EXTRA_APP_PACKAGE, ctx.packageName))
        }
    }
}

@Composable
private fun PermissionCard(icon: ImageVector, title: String, why: String, granted: Boolean, onGrant: () -> Unit) {
    AppCard(padding = 16.dp) {
        Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
            IconCircle(icon)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Row(Modifier.semantics(mergeDescendants = true) {}, horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(title, Modifier.weight(1f), style = MaterialTheme.typography.titleMedium)
                    StatusBadge(
                        stringResource(if (granted) R.string.perm_granted else R.string.perm_pending),
                        if (granted) Tone.SUCCESS else Tone.WARNING,
                    )
                }
                Text(why, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                if (!granted) LinkRow(stringResource(R.string.perm_grant), onGrant)
            }
        }
    }
}

@Composable
fun ChooseScreen(onNew: () -> Unit, onJoin: () -> Unit, onWatch: () -> Unit, onFamily: () -> Unit, onBack: () -> Unit) = Screen(
    title = stringResource(R.string.choose_title),
    onBack = onBack,
    step = stringResource(R.string.step_of, 2, 2),
) {
    ChoiceCard(PcoIcons.User, stringResource(R.string.choose_create), onNew)
    ChoiceCard(PcoIcons.Smartphone, stringResource(R.string.choose_join), onJoin)
    ChoiceCard(PcoIcons.UserCheck, stringResource(R.string.choose_contact), onWatch)
    ChoiceCard(PcoIcons.Users, stringResource(R.string.choose_guardian), onFamily)
}

@Composable
private fun ChoiceCard(icon: ImageVector, title: String, onClick: () -> Unit) = AppCard(onClick = onClick, padding = 16.dp) {
    Row(horizontalArrangement = Arrangement.spacedBy(16.dp), verticalAlignment = Alignment.CenterVertically) {
        IconCircle(icon)
        Text(title, Modifier.weight(1f), style = MaterialTheme.typography.titleMedium)
        Icon(PcoIcons.ChevronRight, null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
    }
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

/** Wake and bedtime side by side, half the width each (no fixed widths: they grow with the text size). */
@Composable
internal fun TimeFields(wake: String, onWake: (String) -> Unit, bedtime: String, onBedtime: (String) -> Unit) {
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        OutlinedTextField(wake, { onWake(it.take(5)) }, label = { Text(stringResource(R.string.wake)) }, isError = parseTime(wake) == null, singleLine = true, modifier = Modifier.weight(1f))
        OutlinedTextField(bedtime, { onBedtime(it.take(5)) }, label = { Text(stringResource(R.string.bedtime)) }, isError = parseTime(bedtime) == null, singleLine = true, modifier = Modifier.weight(1f))
    }
}

/** Number field for small counts (passes, minutes). */
@Composable
internal fun NumberField(value: String, onChange: (String) -> Unit, label: String, modifier: Modifier = Modifier) =
    OutlinedTextField(
        value, { onChange(it.filter(Char::isDigit).take(2)) }, label = { Text(label) }, singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number), modifier = modifier,
    )

/** Monday-first day toggles; they wrap to a second line at large text sizes. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun DayChips(mask: Int, onChange: (Int) -> Unit) {
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        dayLabels().forEachIndexed { i, d ->
            FilterChip(selected = mask and (1 shl i) != 0, onClick = { onChange(mask xor (1 shl i)) }, label = { Text(d) })
        }
    }
}

private enum class AppRole { NONE, ESSENTIAL, FOCUS }

@Composable
fun ConfigScreen(family: Boolean = false, onBack: () -> Unit, onDone: () -> Unit) {
    val ctx = LocalContext.current
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
    var apps by remember { mutableStateOf<List<LaunchableApp>?>(null) }
    val roles = remember { mutableStateMapOf<String, AppRole>() }
    var error by remember { mutableStateOf<String?>(null) }

    LaunchedEffect(Unit) {
        apps = withContext(Dispatchers.IO) { launchableApps(ctx) }
        apps?.firstOrNull { it.pkg == "com.whatsapp" }?.let { roles[it.id] = AppRole.ESSENTIAL } // preselected
    }
    val wakeMin = parseTime(wake)
    val bedMin = parseTime(bedtime)
    val ready = wakeMin != null && bedMin != null && goals.any { it.kind == GoalKind.KEY } && (!family || child.isNotBlank())

    Screen(
        title = stringResource(if (family) R.string.config_minor_title else R.string.config_title),
        onBack = onBack,
        bottom = {
            ErrorText(error)
            PrimaryButton(stringResource(R.string.create_profile), enabled = ready) {
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
                    // Methods are added from "How changes are authorized" (each one tightens).
                    auth = AuthPolicy(local = emptyList(), contactRequired = false),
                )
                error = withContext(Dispatchers.IO) {
                    runCatching {
                        val clock = CoreHolder.clock()
                        val familyId = if (family) CoreHolder.newFamilyId() else null
                        val core = familyId?.let { CoreHolder.family(ctx, it) } ?: CoreHolder.get(ctx)
                        if (familyId != null) core.createFamilyProfile(clock, label.ifBlank { defaultLabel }, config)
                        else core.createProfile(clock, label.ifBlank { defaultLabel }, config)
                        familyId?.let { CoreHolder.saveFamilyProfile(ctx, it, child.trim()) }
                    }.exceptionOrNull()?.userMessage(ctx)
                }
                if (error == null) onDone()
            }
        },
    ) {
        if (family) {
            OutlinedTextField(child, { child = it.take(64) }, label = { Text(stringResource(R.string.config_minor_name)) }, singleLine = true, modifier = Modifier.fillMaxWidth())
            OutlinedTextField(label, { label = it.take(64) }, label = { Text(stringResource(R.string.config_guardian_name)) }, singleLine = true, modifier = Modifier.fillMaxWidth())
        } else {
            OutlinedTextField(label, { label = it.take(64) }, label = { Text(stringResource(R.string.config_phone_name)) }, singleLine = true, modifier = Modifier.fillMaxWidth())
        }

        Section(stringResource(R.string.schedule)) {
            TimeFields(wake, { wake = it }, bedtime, { bedtime = it })
            Text(stringResource(R.string.free_days_hint), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            DayChips(freeDays) { freeDays = it }
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                NumberField(permits, { permits = it }, stringResource(R.string.passes_per_day), Modifier.weight(1f))
                NumberField(permitMinutes, { permitMinutes = it }, stringResource(R.string.minutes), Modifier.weight(1f))
            }
            if (family) {
                NumberField(bonus, { bonus = it }, stringResource(R.string.weekly_bonus), Modifier.fillMaxWidth())
                Footnote(stringResource(R.string.guardian_rules_note))
            } else {
                Footnote(stringResource(R.string.anti_impulse_note))
            }
        }

        Section(stringResource(R.string.fixed_goals)) {
            Text(stringResource(R.string.fixed_goals_hint), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            goals.forEachIndexed { i, g ->
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(g.title, Modifier.weight(1f), style = MaterialTheme.typography.titleMedium)
                    KindChip(g.kind)
                    TextButton(onClick = { goals.removeAt(i) }) { Text(stringResource(R.string.remove)) }
                }
            }
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                OutlinedTextField(newGoal, { newGoal = it.take(80) }, label = { Text(stringResource(R.string.new_goal)) }, singleLine = true, modifier = Modifier.weight(1f))
                OutlinedTextField(
                    newMinutes, { newMinutes = it.filter(Char::isDigit).take(3) }, label = { Text(stringResource(R.string.min_short)) }, singleLine = true,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number), modifier = Modifier.width(96.dp),
                )
            }
            SecondaryButton(stringResource(R.string.add_goal), enabled = newGoal.isNotBlank()) {
                val m = (newMinutes.toIntOrNull() ?: 0).coerceIn(0, 600)
                goals += Goal(slug(newGoal, goals.map { it.id }), newGoal.trim(), GoalKind.KEY, m.toUShort(), 1u, methodsFor(m), ALL_DAYS)
                newGoal = ""
            }
        }

        Section(stringResource(R.string.apps)) {
            Text(stringResource(R.string.apps_hint), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            val list = apps
            if (list == null) LoadingState()
            list?.forEach { app ->
                val role = roles[app.id] ?: AppRole.NONE
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(app.label, Modifier.weight(1f), style = MaterialTheme.typography.bodyLarge)
                    FilterChip(selected = role == AppRole.ESSENTIAL, onClick = { roles[app.id] = if (role == AppRole.ESSENTIAL) AppRole.NONE else AppRole.ESSENTIAL }, label = { Text(stringResource(R.string.app_essential)) })
                    FilterChip(selected = role == AppRole.FOCUS, onClick = { roles[app.id] = if (role == AppRole.FOCUS) AppRole.NONE else AppRole.FOCUS }, label = { Text(stringResource(R.string.check_focus)) })
                }
            }
            OutlinedTextField(web, { web = it }, label = { Text(stringResource(R.string.focus_sites_pc)) }, modifier = Modifier.fillMaxWidth())
        }
    }
}
