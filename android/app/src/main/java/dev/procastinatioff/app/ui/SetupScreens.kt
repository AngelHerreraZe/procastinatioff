package dev.procastinatioff.app.ui

import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import dev.procastinatioff.app.R
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
import androidx.compose.foundation.Image
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
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshots.SnapshotStateList
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.core.app.NotificationManagerCompat
import androidx.fragment.app.FragmentActivity
import androidx.lifecycle.compose.LifecycleResumeEffect
import dev.procastinatioff.app.auth.LocalAuth
import dev.procastinatioff.app.block.AdminReceiver
import dev.procastinatioff.app.block.AppIdentity
import dev.procastinatioff.app.core.CoreHolder
import dev.procastinatioff.app.core.userMessage
import dev.procastinatioff.app.goals.GoalSuggest
import dev.procastinatioff.app.sync.isAccessibilityEnabled
import dev.procastinatioff.app.verify.ensureVerifier
import dev.procastinatioff.app.verify.verifierIsGenuine
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import uniffi.procastinatioff_core.AuthPolicy
import uniffi.procastinatioff_core.Check
import uniffi.procastinatioff_core.Config
import uniffi.procastinatioff_core.Goal
import uniffi.procastinatioff_core.GoalKind
import uniffi.procastinatioff_core.LocalFactor
import uniffi.procastinatioff_core.Schedule
import uniffi.procastinatioff_core.SecretHolder
import uniffi.procastinatioff_core.TimeOfDay
import uniffi.procastinatioff_core.checkOf
import uniffi.procastinatioff_core.checkSecret
import uniffi.procastinatioff_core.checkUsesMinutes
import uniffi.procastinatioff_core.newPlaceAnchor

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

/**
 * Step 1: the camera verifier is a second app, installed BEFORE the permissions. Once Accessibility is
 * on, the blocker stands between the user and the system installer (and its "unknown apps" setting).
 */
@Composable
fun VerifierScreen(onNext: () -> Unit, onBack: () -> Unit) {
    val ctx = LocalContext.current
    var installed by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    LifecycleResumeEffect(Unit) { // recomputed when coming back from the system installer
        installed = verifierIsGenuine(ctx)
        onPauseOrDispose { }
    }

    Screen(
        title = stringResource(R.string.verifier_setup_title),
        lead = stringResource(R.string.verifier_setup_lead),
        onBack = onBack,
        step = stringResource(R.string.step_of, 1, 3),
        bottom = {
            ErrorText(error)
            if (installed) {
                PrimaryButton(stringResource(R.string.action_continue)) { onNext() }
            } else {
                PrimaryButton(stringResource(R.string.verifier_install)) {
                    installed = ensureVerifier(ctx)
                    error = if (installed) null else ctx.getString(R.string.verifier_install_failed)
                }
                // Escape hatch (a phone that forbids installs); a goal's camera check retries it.
                TextButton(onClick = onNext) { Text(stringResource(R.string.setup_later)) }
            }
        },
    ) {
        StatusBadge(
            stringResource(if (installed) R.string.verifier_installed else R.string.perm_pending),
            if (installed) Tone.SUCCESS else Tone.WARNING,
        )
        Feature(PcoIcons.Camera, R.string.verifier_why_camera_title, R.string.verifier_why_camera_body)
        Feature(PcoIcons.Smartphone, R.string.verifier_why_app_title, R.string.verifier_why_app_body)
        Feature(PcoIcons.Lock, R.string.verifier_why_now_title, R.string.verifier_why_now_body)
    }
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
        step = if (onboarding) stringResource(R.string.step_of, 2, 3) else null,
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
    step = stringResource(R.string.step_of, 3, 3),
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

@OptIn(ExperimentalLayoutApi::class)
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
            Goal("read-30", readTitle, GoalKind.KEY, 30u, 1u, methodsFor(Check.FOCUS, 30), ALL_DAYS, emptyList(), emptyList()),
            Goal("eat-healthy", eatTitle, GoalKind.HABIT, 0u, 2u, methodsFor(Check.MARK, 0), ALL_DAYS, emptyList(), emptyList()),
        )
    }
    var factor by remember { mutableStateOf<LocalFactor?>(null) }
    var secret by remember { mutableStateOf("") }
    var biometric by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val activity = ctx as FragmentActivity

    val wakeMin = parseTime(wake)
    val bedMin = parseTime(bedtime)
    val ready = wakeMin != null && bedMin != null && goals.any { it.kind == GoalKind.KEY } && (!family || child.isNotBlank()) &&
        (factor == null || secret.isNotEmpty())

    Screen(
        title = stringResource(if (family) R.string.config_minor_title else R.string.config_title),
        onBack = onBack,
        bottom = {
            ErrorText(error)
            PrimaryButton(stringResource(R.string.create_profile), enabled = ready) {
                val f = factor
                // Checked before the profile exists: a weak secret must not leave it half set up.
                if (f != null) {
                    error = withContext(Dispatchers.IO) { runCatching { checkSecret(f, secret) }.exceptionOrNull()?.userMessage(ctx) }
                    if (error != null) return@PrimaryButton
                    if (biometric) {
                        // Used once before relying on it, as in "How changes are authorized".
                        if (Build.VERSION.SDK_INT >= 30) runCatching { LocalAuth.ensureKey() }
                        val title = ctx.getString(R.string.auth_use_biometric)
                        if (!LocalAuth.confirm(activity, title, title)) return@PrimaryButton
                    }
                }
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
                    // Phone, SMS and WhatsApp are always open without being listed (AppIdentity.systemAllowed);
                    // everything else only during the focus of a goal that lists it.
                    essentialApps = emptyList(),
                    focusApps = emptyList(),
                    webAllowlist = emptyList(),
                    weeklyBonusPermits = if (family) (bonus.toIntOrNull() ?: 0).coerceIn(0, 20).toUByte() else 0u,
                    // The method chosen here is added by setSecret right after (each one tightens).
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
                        if (f != null) {
                            core.setSecret(clock, f, secret, SecretHolder.OWNER)
                            if (biometric) core.activeConfig(clock)?.let { cfg ->
                                core.setConfig(clock, cfg.copy(auth = cfg.auth.copy(local = cfg.auth.local + LocalFactor.BIOMETRIC)))
                            }
                        }
                    }.exceptionOrNull()?.userMessage(ctx)
                }
                secret = ""
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

        // A minor's profile: the guardian sets their own PIN or pattern on the minor's phone, not here.
        if (!family) Section(stringResource(R.string.auth_title)) {
            Text(stringResource(R.string.auth_setup_hint), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                FilterChip(selected = factor == null, onClick = { factor = null; secret = "" }, label = { Text(stringResource(R.string.auth_none)) })
                listOf(LocalFactor.PIN, LocalFactor.PASSWORD, LocalFactor.PATTERN).forEach { f ->
                    FilterChip(selected = factor == f, onClick = { factor = f; secret = "" }, label = { Text(stringResource(factorName(f))) })
                }
            }
            factor?.let { f ->
                Text(stringResource(secretHint(f)), style = MaterialTheme.typography.bodySmall)
                SecretInput(f, secret) { secret = it }
                if (LocalAuth.available(activity)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(stringResource(R.string.auth_use_biometric), Modifier.weight(1f))
                        Switch(checked = biometric, onCheckedChange = { biometric = it })
                    }
                }
            }
        }

        Section(stringResource(R.string.fixed_goals)) { FixedGoalsEditor(goals, guardian = family) }
    }
}

/**
 * How a goal is checked. While the person has not tapped a chip, it follows the goal's name (the phone's own
 * AI or the core's catalog, see GoalSuggest) and says so; `suggest` is false when editing an existing goal.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun CheckPicker(title: String, check: Check, offered: List<Check>, suggest: Boolean, onCheck: (Check) -> Unit) {
    var picked by remember { mutableStateOf(false) }
    var byAi by remember { mutableStateOf<Boolean?>(null) }
    LaunchedEffect(title, suggest) {
        if (title.isBlank()) { picked = false; byAi = null }
        if (!suggest || picked || title.isBlank()) return@LaunchedEffect
        delay(600) // typing: only the name it stops at
        val s = GoalSuggest.suggest(title, offered) ?: return@LaunchedEffect
        if (!picked) { byAi = s.byAi; onCheck(s.check) }
    }
    Text(stringResource(R.string.how_checked), style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        offered.forEach { c -> FilterChip(selected = check == c, onClick = { picked = true; onCheck(c) }, label = { Text(stringResource(c.label)) }) }
    }
    if (suggest && !picked) byAi?.let { Footnote(stringResource(if (it) R.string.suggested_by_ai else R.string.suggested_by_name)) }
}

/** A new place QR: shown once (the configuration only keeps its hash), to print or photograph now. */
@Composable
internal fun PlaceQrCard(code: String, onSaved: () -> Unit) {
    val bitmap = remember(code) { qrBitmap(code) }
    AppCard(padding = 16.dp) {
        Text(stringResource(R.string.place_qr_title), style = MaterialTheme.typography.titleMedium)
        Image(bitmap.asImageBitmap(), contentDescription = stringResource(R.string.place_qr_title), modifier = Modifier.fillMaxWidth())
        Footnote(stringResource(R.string.place_qr_note))
        SecondaryButton(stringResource(R.string.place_qr_saved)) { onSaved() }
    }
}

/**
 * Fixed goals: add, edit and remove, with the apps and sites each one's focus opens. Used when creating
 * the profile and in the settings; the core decides when a change applies (relaxing: next wake-up).
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun FixedGoalsEditor(goals: SnapshotStateList<Goal>, guardian: Boolean = false) {
    val ctx = LocalContext.current
    var apps by remember { mutableStateOf<List<LaunchableApp>?>(null) }
    var editing by remember { mutableStateOf<Int?>(null) } // null: a new goal
    var title by remember { mutableStateOf("") }
    var minutes by remember { mutableStateOf("30") }
    var check by remember { mutableStateOf(Check.FOCUS) }
    val picked = remember { mutableStateListOf<String>() }
    var sites by remember { mutableStateOf("") }
    var pickApps by remember { mutableStateOf(false) }
    var newQr by remember { mutableStateOf<String?>(null) } // a place QR to print, shown once
    val offered = remember(guardian) { offeredChecks(guardian) }
    fun reset() {
        editing = null; title = ""; minutes = "30"; check = Check.FOCUS; picked.clear(); sites = ""; pickApps = false
    }
    /** Place goals keep their anchor; a goal that becomes one gets a new QR (and shows it). */
    fun withAnchor(g: Goal): Goal = when {
        check != Check.PLACE -> g.copy(anchor = "")
        g.anchor.isNotEmpty() -> g
        else -> newPlaceAnchor().let { a -> newQr = a.code; g.copy(anchor = a.hash) }
    }
    /** The form as a goal: a new one, or `old` with the form's changes. */
    fun fromForm(old: Goal?): Goal {
        val m = if (checkUsesMinutes(check)) (minutes.toIntOrNull() ?: 0).coerceIn(0, 600) else 0
        val siteList = sites.split(',', ' ', '\n').map { it.trim().lowercase() }.filter { it.isNotEmpty() }.distinct()
        return withAnchor(
            old?.copy(
                title = title.trim().ifEmpty { old.title }, targetMinutes = m.toUShort(), methods = methodsFor(check, m),
                apps = picked.toList(), sites = siteList,
            ) ?: Goal(slug(title, goals.map { it.id }), title.trim(), GoalKind.KEY, m.toUShort(), 1u, methodsFor(check, m), ALL_DAYS, picked.toList(), siteList),
        )
    }
    // Editing writes straight into the list: a change is never lost for not pressing a button.
    fun changed() { editing?.let { goals[it] = fromForm(goals[it]) } }

    // Phone, SMS, WhatsApp, launcher…: always open, so they are not offered.
    LaunchedEffect(Unit) {
        apps = withContext(Dispatchers.IO) {
            val always = AppIdentity.systemAllowed(ctx, sleeping = false)
            launchableApps(ctx).filter { it.pkg !in always }
        }
    }

    Text(stringResource(R.string.fixed_goals_hint), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
    goals.forEachIndexed { i, g ->
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Column(Modifier.weight(1f)) {
                Text(g.title, style = MaterialTheme.typography.titleMedium)
                val target = if (g.targetMinutes > 0u) duration(g.targetMinutes.toInt())
                else pluralStringResource(R.plurals.times, g.targetCount.toInt(), g.targetCount.toInt())
                Text("$target · ${stringResource(checkOf(g.methods).label)}", style = MaterialTheme.typography.bodyMedium)
                // What its focus opens: app names, then sites
                val opens = g.apps.mapNotNull { id -> apps?.firstOrNull { it.id == id }?.label } + g.sites
                if (opens.isNotEmpty()) {
                    Text(opens.joinToString(", "), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
            KindChip(g.kind)
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            TextButton(onClick = {
                editing = i; title = g.title; minutes = g.targetMinutes.toString(); check = checkOf(g.methods)
                picked.clear(); picked += g.apps; sites = g.sites.joinToString(", "); pickApps = false
            }) { Text(stringResource(R.string.edit)) }
            TextButton(onClick = { goals.removeAt(i); reset() }) { Text(stringResource(R.string.remove)) }
        }
    }
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        OutlinedTextField(
            title, { title = it.take(80); changed() }, label = { Text(stringResource(if (editing == null) R.string.new_goal else R.string.goal)) },
            singleLine = true, modifier = Modifier.weight(1f),
        )
        if (checkUsesMinutes(check)) OutlinedTextField(
            minutes, { minutes = it.filter(Char::isDigit).take(3); changed() }, label = { Text(stringResource(R.string.min_short)) }, singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number), modifier = Modifier.width(96.dp),
        )
    }
    CheckPicker(title, check, offered, suggest = editing == null) { check = it; changed() }
    if (check == Check.HEALTH) Footnote(stringResource(R.string.health_minutes_note))
    newQr?.let { PlaceQrCard(it) { newQr = null } }
    LinkRow(stringResource(R.string.goal_apps, picked.size)) { pickApps = !pickApps }
    if (pickApps) {
        val list = apps
        if (list == null) LoadingState()
        else FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            list.forEach { app ->
                val on = app.id in picked
                FilterChip(selected = on, onClick = { if (on) picked.remove(app.id) else picked.add(app.id); changed() }, label = { Text(app.label) })
            }
        }
    }
    OutlinedTextField(sites, { sites = it; changed() }, label = { Text(stringResource(R.string.goal_sites_pc)) }, modifier = Modifier.fillMaxWidth())
    if (editing == null) {
        SecondaryButton(stringResource(R.string.add_goal), enabled = title.isNotBlank()) { goals += fromForm(null); reset() }
    } else {
        SecondaryButton(stringResource(R.string.done)) { reset() }
    }
}
