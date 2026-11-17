package dev.procastinacioff.app.ui

import android.content.Context
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import dev.procastinacioff.app.R
import android.app.Activity
import android.content.Intent
import android.net.VpnService
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.toMutableStateList
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.semantics
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

// Family mode. Product rule: the guardian has no screens with data the minor does not see.
// Both see the same `MirrorPanel`; the guardian also approves requests and changes settings.

internal fun whenText(ms: Long): String {
    val pattern = android.text.format.DateFormat.getBestDateTimePattern(Locale.getDefault(), "EEEdMMMjmm")
    return SimpleDateFormat(pattern, Locale.getDefault()).format(Date(ms)).replaceFirstChar { it.uppercase() }
}

fun describe(ctx: Context, what: RequestKind): String = when (what) {
    is RequestKind.ExtraPermit -> ctx.getString(R.string.req_extra_today, what.minutes.toInt())
    is RequestKind.AddSite -> ctx.getString(R.string.req_site, what.domain)
    is RequestKind.LaterBedtime -> ctx.getString(R.string.req_bedtime_permanent, what.minutes.toInt())
    is RequestKind.RestDay -> ctx.getString(R.string.req_rest_today)
    is RequestKind.ConfigChange -> ctx.getString(R.string.req_config_change)
}

private fun alertText(ctx: Context, a: FamilyAlert): String = when (a) {
    is FamilyAlert.Emergency -> ctx.getString(R.string.alert_emergency_used)
    is FamilyAlert.BlockedSites -> ctx.resources.getQuantityString(R.plurals.blocked_sites, a.count.toInt(), a.count.toInt())
    is FamilyAlert.Granted -> ctx.getString(R.string.alert_granted, describeAsk(ctx, a.what))
    is FamilyAlert.Tamper -> ctx.getString(
        when (a.what) {
            TamperKind.PERMISSION_REVOKED -> R.string.tamper_permission_revoked
            TamperKind.SERVICE_STOPPED -> R.string.tamper_service_stopped
            TamperKind.CLOCK_CHANGED -> R.string.tamper_clock_changed
            TamperKind.UNINSTALL_ATTEMPT -> R.string.tamper_uninstall_attempt
            TamperKind.SAFE_MODE -> R.string.tamper_safe_mode
            TamperKind.DEVICE_SILENT -> R.string.tamper_device_silent
        },
    )
}

private fun stateText(ctx: Context, st: Status, now: Long): String = when (val s = st.state) {
    is DayState.Sleep -> ctx.getString(R.string.state_sleeping)
    is DayState.Locked -> ctx.getString(if (st.needsPlan) R.string.state_no_plan else R.string.state_locked)
    is DayState.Pass -> ctx.getString(R.string.state_pass, minutesUntil(s.untilMs, now))
    is DayState.Focus -> ctx.getString(R.string.state_focus)
    is DayState.Free -> ctx.getString(if (st.freeDay) R.string.title_free_day else R.string.state_free_done)
    is DayState.Emergency -> ctx.getString(R.string.state_emergency, minutesUntil(s.untilMs, now))
}

// ── State of a profile (the minor's), polled every 2 s ──

data class FamilySnapshot(
    val status: Status? = null,
    val pending: List<PendingRequest> = emptyList(),
    val alerts: List<FamilyAlertView> = emptyList(),
    val devices: List<DeviceSeen> = emptyList(),
    val nowMs: Long = System.currentTimeMillis(),
)

/** `sync`: the guardian's phone syncs every 30 s while looking at the panel. */
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
                    runCatching { CoreHolder.syncNow(core) }
                }
                runCatching {
                    FamilySnapshot(core.status(clock), core.pendingRequests(), core.familyAlerts(clock), core.devicesSeen(clock.nowMs), clock.nowMs)
                }.getOrDefault(FamilySnapshot())
            }
            delay(2_000)
        }
    }
    return snap
}

/** What the guardian and the minor both see: state, goals, passes, devices, requests and alerts. */
@Composable
fun MirrorPanel(snap: FamilySnapshot) {
    val ctx = LocalContext.current
    val st = snap.status
    Section(stringResource(R.string.today)) {
        if (st == null) {
            Footnote(stringResource(R.string.mirror_no_data))
            return@Section
        }
        Text(stateText(ctx, st, snap.nowMs), style = MaterialTheme.typography.titleLarge)
        st.goals.forEach { g ->
            Row(Modifier.semantics(mergeDescendants = true) {}, horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(g.title, Modifier.weight(1f), style = MaterialTheme.typography.bodyLarge)
                if (g.completed) StatusBadge(stringResource(R.string.goal_done), Tone.SUCCESS)
                else Text(stringResource(R.string.progress_count, g.doneCount.toInt(), g.targetCount.toInt()), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        if (!st.freeDay) InfoChip(PcoIcons.Ticket, pluralStringResource(R.plurals.passes_available, st.permitsLeft.toInt(), st.permitsLeft.toInt()))
    }
    if (snap.devices.isNotEmpty()) {
        Section(stringResource(R.string.devices)) {
            snap.devices.forEach { d ->
                val seen = if (d.lastSeenMs > 0) stringResource(R.string.device_seen, hoursAgo(ctx, d.lastSeenMs, snap.nowMs))
                else stringResource(R.string.device_not_reported)
                IconText(PcoIcons.Smartphone, "${d.label} · $seen")
            }
        }
    }
    if (snap.pending.isNotEmpty()) {
        Section(stringResource(R.string.unanswered_requests)) {
            snap.pending.forEach { r -> IconText(PcoIcons.Clock, "${describe(ctx, r.what)} · ${whenText(r.atMs)}") }
        }
    }
    Section(stringResource(R.string.alerts_7_days)) {
        if (snap.alerts.isEmpty()) Footnote(stringResource(R.string.none), PcoIcons.Check)
        snap.alerts.forEach { a -> IconText(PcoIcons.Alert, "${alertText(ctx, a.alert)} · ${whenText(a.atMs)} · ${a.deviceLabel}") }
    }
    Footnote(stringResource(R.string.mirror_never_seen), PcoIcons.EyeOff)
}

// ───────────────────────── Guardian ─────────────────────────

/** Guardian consent. LEGAL: validate with legal counsel before the pilot. */
@Composable
fun FamilyConsentScreen(onAccept: () -> Unit, onBack: () -> Unit) {
    val activity = LocalContext.current as FragmentActivity
    val reason = stringResource(R.string.consent_confirm_reason)
    Screen(
        title = stringResource(R.string.consent_title),
        onBack = onBack,
        bottom = {
            PrimaryButton(stringResource(R.string.confirm_biometric_pin)) { if (confirmAsGuardian(activity, reason)) onAccept() }
        },
    ) {
        Section(stringResource(R.string.consent_does_title)) {
            IconText(PcoIcons.Lock, stringResource(R.string.consent_does_block))
            IconText(PcoIcons.Camera, stringResource(R.string.consent_does_camera))
        }
        Section(stringResource(R.string.consent_sees_title)) {
            IconText(PcoIcons.Check, stringResource(R.string.consent_sees_goals))
            IconText(PcoIcons.Check, stringResource(R.string.consent_sees_sites))
        }
        Section(stringResource(R.string.consent_never_title)) {
            IconText(PcoIcons.EyeOff, stringResource(R.string.consent_never_list))
        }
        Footnote(stringResource(R.string.consent_mirror))
        Text(stringResource(R.string.consent_declare), style = MaterialTheme.typography.titleMedium)
    }
}

/** List of the minors' profiles I manage, with their day state. */
@Composable
fun FamilyListScreen(onOpen: (String) -> Unit, onNew: () -> Unit) {
    val ctx = LocalContext.current
    val profiles = remember { CoreHolder.familyProfiles(ctx) }
    Screen(
        title = stringResource(R.string.family_short),
        bottom = { SecondaryButton(stringResource(R.string.add_minor)) { onNew() } },
    ) {
        if (profiles.isEmpty()) EmptyState(PcoIcons.Users, stringResource(R.string.family_empty))
        profiles.forEach { (id, name) ->
            val snap = rememberFamilySnapshot(CoreHolder.family(ctx, id), sync = false)
            AppCard(onClick = { onOpen(id) }, padding = 16.dp) {
                Row(horizontalArrangement = Arrangement.spacedBy(16.dp), verticalAlignment = Alignment.CenterVertically) {
                    IconCircle(PcoIcons.User)
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text(name, style = MaterialTheme.typography.titleMedium)
                        snap.status?.let { st ->
                            val done = st.goals.count { it.completed }
                            Text(
                                pluralStringResource(R.plurals.family_row_state, st.goals.size, stateText(ctx, st, snap.nowMs), done, st.goals.size),
                                style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        if (snap.pending.isNotEmpty()) {
                            StatusBadge(pluralStringResource(R.plurals.requests_waiting, snap.pending.size, snap.pending.size), Tone.WARNING)
                        }
                    }
                    Icon(PcoIcons.ChevronRight, null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
        }
    }
}

/** A minor's panel (guardian): the mirror panel + approve requests, pair and settings. */
@Composable
fun ChildScreen(id: String, onInvite: (Role) -> Unit, onSettings: () -> Unit, onBack: () -> Unit) {
    val ctx = LocalContext.current
    val activity = ctx as FragmentActivity
    val core = remember(id) { CoreHolder.family(ctx, id) }
    val name = remember(id) { CoreHolder.familyProfiles(ctx)[id].orEmpty() }
    var refresh by remember { mutableIntStateOf(0) }
    val snap = rememberFamilySnapshot(core, sync = true, refresh = refresh)
    var error by remember { mutableStateOf<String?>(null) }

    suspend fun answer(r: PendingRequest, approved: Boolean) {
        val reason = ctx.getString(if (approved) R.string.approve_what else R.string.reject_what, describe(ctx, r.what))
        if (!confirmAsGuardian(activity, reason)) return
        error = withContext(Dispatchers.IO) {
            runCatching { core.decide(CoreHolder.clock(), r.requestId, approved); CoreHolder.syncNow(core) }
                .exceptionOrNull()?.userMessage(ctx)
        }
        refresh++
    }

    Screen(title = name, onBack = onBack) {
        snap.pending.forEach { r ->
            Section("${describe(ctx, r.what)} · ${whenText(r.atMs)}") {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    PrimaryButton(stringResource(R.string.approve), Modifier.weight(1f)) { answer(r, true) }
                    SecondaryButton(stringResource(R.string.reject), Modifier.weight(1f)) { answer(r, false) }
                }
            }
        }
        ErrorText(error)
        Footnote(stringResource(R.string.child_mirror_note, name))
        MirrorPanel(snap)
        AnswerCodeSection(core)
        GuardianSecretSection(core, name)
        Section(stringResource(R.string.devices)) {
            SecondaryButton(stringResource(R.string.child_pair_device, name)) { onInvite(Role.MINOR) }
            SecondaryButton(stringResource(R.string.add_guardian)) { onInvite(Role.GUARDIAN) }
            SecondaryButton(stringResource(R.string.child_settings, name)) { onSettings() }
        }
    }
}

/** The minor's settings (guardian). They take effect immediately: a guardian signs them. */
@Composable
fun ChildSettingsScreen(id: String, onBack: () -> Unit) {
    val ctx = LocalContext.current
    val activity = ctx as FragmentActivity
    val core = remember(id) { CoreHolder.family(ctx, id) }
    val cfg = remember(id) { core.activeConfig(CoreHolder.clock()) }
    if (cfg == null) {
        Screen(title = stringResource(R.string.settings), onBack = onBack) { EmptyState(PcoIcons.Settings, stringResource(R.string.no_config_yet)) }
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
    val goals = remember { cfg.goals.toMutableStateList() }
    var error by remember { mutableStateOf<String?>(null) }
    val wakeMin = parseTime(wake)
    val bedMin = parseTime(bedtime)
    val reason = stringResource(R.string.change_settings)

    Screen(
        title = stringResource(R.string.settings),
        onBack = onBack,
        bottom = {
            ErrorText(error)
            PrimaryButton(stringResource(R.string.save_now), enabled = wakeMin != null && bedMin != null) {
                val next = cfg.copy(
                    schedule = cfg.schedule.copy(wake = TimeOfDay(wakeMin!!.toUShort()), bedtime = TimeOfDay(bedMin!!.toUShort())),
                    permitsPerDay = (permits.toIntOrNull() ?: 0).coerceIn(0, 10).toUByte(),
                    permitMinutes = (minutes.toIntOrNull() ?: 5).coerceIn(1, 60).toUShort(),
                    weeklyBonusPermits = (bonus.toIntOrNull() ?: 0).coerceIn(0, 20).toUByte(),
                    freeDays = freeDays.toUByte(),
                    webAllowlist = web.split(',', ' ', '\n').map { it.trim().lowercase() }.filter { it.isNotEmpty() },
                    goals = goals.toList(),
                )
                if (!confirmAsGuardian(activity, reason)) return@PrimaryButton
                error = withContext(Dispatchers.IO) {
                    runCatching { core.setConfig(CoreHolder.clock(), next); CoreHolder.syncNow(core) }.exceptionOrNull()?.userMessage(ctx)
                }
                if (error == null) onBack()
            }
        },
    ) {
        Section(stringResource(R.string.schedule)) {
            TimeFields(wake, { wake = it }, bedtime, { bedtime = it })
            Text(stringResource(R.string.free_days), style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            DayChips(freeDays) { freeDays = it }
        }
        Section(stringResource(R.string.passes_per_day)) {
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                NumberField(permits, { permits = it }, stringResource(R.string.passes_per_day_short), Modifier.weight(1f))
                NumberField(minutes, { minutes = it }, stringResource(R.string.minutes), Modifier.weight(1f))
            }
            NumberField(bonus, { bonus = it }, stringResource(R.string.extra_per_week), Modifier.fillMaxWidth())
        }
        Section(stringResource(R.string.fixed_goals)) { FixedGoalsEditor(goals) }
        OutlinedTextField(web, { web = it }, label = { Text(stringResource(R.string.focus_sites)) }, modifier = Modifier.fillMaxWidth())
    }
}

// ───────────────────────── Minor ─────────────────────────

/** On the minor's device: ask the guardian, enable the web filter and "What your guardian sees". */
@Composable
fun TutorScreen(onBack: () -> Unit) {
    val ctx = LocalContext.current
    val core = remember { CoreHolder.get(ctx) }
    var refresh by remember { mutableIntStateOf(0) }
    val snap = rememberFamilySnapshot(core, sync = false, refresh = refresh)
    var site by remember { mutableStateOf("") }
    var message by remember { mutableStateOf<String?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var vpnReady by remember { mutableStateOf(VpnService.prepare(ctx) == null) }
    val askVpn = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { r ->
        vpnReady = r.resultCode == Activity.RESULT_OK
        if (vpnReady) ctx.startService(Intent(ctx, DnsVpnService::class.java))
    }
    LaunchedEffect(vpnReady) { if (vpnReady) ctx.startService(Intent(ctx, DnsVpnService::class.java)) }

    suspend fun ask(what: RequestKind) {
        error = withContext(Dispatchers.IO) {
            runCatching { core.request(CoreHolder.clock(), what) }.exceptionOrNull()?.userMessage(ctx)
        }
        if (error == null) {
            message = ctx.getString(R.string.request_sent, describe(ctx, what))
            site = ""
            SyncScheduler.syncNow(ctx)
            refresh++
        }
    }

    Screen(title = stringResource(R.string.your_guardian), onBack = onBack) {
        if (!vpnReady) {
            Section(stringResource(R.string.vpn_missing_title)) {
                Text(stringResource(R.string.vpn_missing_body), style = MaterialTheme.typography.bodyMedium)
                PrimaryButton(stringResource(R.string.enable)) { VpnService.prepare(ctx)?.let { askVpn.launch(it) } }
            }
        }
        Section(stringResource(R.string.ask_something)) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                SecondaryButton(stringResource(R.string.more_15_min), Modifier.weight(1f)) { ask(RequestKind.ExtraPermit(15u)) }
                SecondaryButton(stringResource(R.string.rest_day), Modifier.weight(1f)) { ask(RequestKind.RestDay) }
            }
            SecondaryButton(stringResource(R.string.sleep_30_later)) { ask(RequestKind.LaterBedtime(30u)) }
            OutlinedTextField(site, { site = it.take(253) }, label = { Text(stringResource(R.string.site_for_homework)) }, placeholder = { Text("khanacademy.org") }, singleLine = true, modifier = Modifier.fillMaxWidth())
            SecondaryButton(stringResource(R.string.ask_access), enabled = site.isNotBlank()) { ask(RequestKind.AddSite(site.trim())) }
            message?.let { IconText(PcoIcons.Check, it) }
            ErrorText(error)
        }
        Section(stringResource(R.string.offline_title)) {
            OfflineAskSection(core, channel = { null }) { message = it; refresh++ }
        }
        Section(stringResource(R.string.guardian_here)) {
            GuardianHereSection(core) { message = it; refresh++ }
        }
        SectionHeader(stringResource(R.string.what_guardian_sees))
        Footnote(stringResource(R.string.guardian_sees_exactly))
        MirrorPanel(snap)
    }
}
