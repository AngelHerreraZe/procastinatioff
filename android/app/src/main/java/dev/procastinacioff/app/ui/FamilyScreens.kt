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

// Family mode. Product rule: the guardian has no screens with data the minor does not see.
// Both see the same `MirrorPanel`; the guardian also approves requests and changes settings.

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

private fun whenText(ms: Long): String {
    val pattern = android.text.format.DateFormat.getBestDateTimePattern(Locale.getDefault(), "EEEdMMMjmm")
    return SimpleDateFormat(pattern, Locale.getDefault()).format(Date(ms)).replaceFirstChar { it.uppercase() }
}

fun describe(ctx: Context, what: RequestKind): String = when (what) {
    is RequestKind.ExtraPermit -> ctx.getString(R.string.req_extra_today, what.minutes.toInt())
    is RequestKind.AddSite -> ctx.getString(R.string.req_site, what.domain)
    is RequestKind.LaterBedtime -> ctx.getString(R.string.req_bedtime_permanent, what.minutes.toInt())
    is RequestKind.RestDay -> ctx.getString(R.string.req_rest_today)
}

private fun alertText(ctx: Context, a: FamilyAlert): String = when (a) {
    is FamilyAlert.Emergency -> ctx.getString(R.string.alert_emergency_used)
    is FamilyAlert.BlockedSites -> ctx.resources.getQuantityString(R.plurals.blocked_sites, a.count.toInt(), a.count.toInt())
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

/** What the guardian and the minor both see: state, goals, passes, devices, requests and alerts. */
@Composable
fun MirrorPanel(snap: FamilySnapshot) {
    val ctx = LocalContext.current
    val st = snap.status
    Section(stringResource(R.string.today)) {
        if (st == null) {
            Text(stringResource(R.string.mirror_no_data))
            return@Section
        }
        Text(stateText(ctx, st, snap.nowMs), style = MaterialTheme.typography.titleMedium)
        val doneLabel = stringResource(R.string.goal_done)
        st.goals.forEach { g ->
            val line = "${g.title} (${g.doneCount}/${g.targetCount})"
            Text(if (g.completed) "$line · $doneLabel" else line)
        }
        if (!st.freeDay) Text(pluralStringResource(R.plurals.passes_available, st.permitsLeft.toInt(), st.permitsLeft.toInt()))
    }
    if (snap.devices.isNotEmpty()) {
        Section(stringResource(R.string.devices)) {
            snap.devices.forEach { d ->
                val seen = if (d.lastSeenMs > 0) stringResource(R.string.device_seen, hoursAgo(ctx, d.lastSeenMs, snap.nowMs))
                else stringResource(R.string.device_not_reported)
                Text("${d.label} · $seen")
            }
        }
    }
    if (snap.pending.isNotEmpty()) {
        Section(stringResource(R.string.unanswered_requests)) {
            snap.pending.forEach { r -> Text("${describe(ctx, r.what)} · ${whenText(r.atMs)}") }
        }
    }
    Section(stringResource(R.string.alerts_7_days)) {
        if (snap.alerts.isEmpty()) Text(stringResource(R.string.none))
        snap.alerts.forEach { a -> Text("${alertText(ctx, a.alert)} · ${whenText(a.atMs)} · ${a.deviceLabel}") }
    }
    Text(
        stringResource(R.string.mirror_never_seen),
        style = MaterialTheme.typography.bodySmall,
    )
}

// ───────────────────────── Guardian ─────────────────────────

/** Guardian consent. LEGAL: validate with legal counsel before the pilot. */
@Composable
fun FamilyConsentScreen(onAccept: () -> Unit, onBack: () -> Unit) {
    val activity = LocalContext.current as FragmentActivity
    val scope = rememberCoroutineScope()
    Screen {
        Heading(stringResource(R.string.consent_title))
        Section(stringResource(R.string.consent_does_title)) {
            Text("• " + stringResource(R.string.consent_does_block))
            Text("• " + stringResource(R.string.consent_does_camera))
        }
        Section(stringResource(R.string.consent_sees_title)) {
            Text("• " + stringResource(R.string.consent_sees_goals))
            Text("• " + stringResource(R.string.consent_sees_sites))
        }
        Section(stringResource(R.string.consent_never_title)) {
            Text("• " + stringResource(R.string.consent_never_list))
        }
        Text(stringResource(R.string.consent_mirror))
        Text(stringResource(R.string.consent_declare), fontWeight = FontWeight.SemiBold)
        val reason = stringResource(R.string.consent_confirm_reason)
        PrimaryButton(stringResource(R.string.confirm_biometric_pin)) {
            scope.launch { if (confirmAsGuardian(activity, reason)) onAccept() }
        }
        TextButton(onClick = onBack) { Text(stringResource(R.string.back)) }
    }
}

/** List of the minors' profiles I manage, with their day state. */
@Composable
fun FamilyListScreen(onOpen: (String) -> Unit, onNew: () -> Unit) {
    val ctx = LocalContext.current
    val profiles = remember { CoreHolder.familyProfiles(ctx) }
    Screen {
        Heading(stringResource(R.string.family_short))
        if (profiles.isEmpty()) Text(stringResource(R.string.family_empty))
        profiles.forEach { (id, name) ->
            val snap = rememberFamilySnapshot(CoreHolder.family(ctx, id), sync = false)
            Card(onClick = { onOpen(id) }, modifier = Modifier.fillMaxWidth()) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(name, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
                    snap.status?.let { st ->
                        val done = st.goals.count { it.completed }
                        Text(pluralStringResource(R.plurals.family_row_state, st.goals.size, stateText(ctx, st, snap.nowMs), done, st.goals.size))
                    }
                    if (snap.pending.isNotEmpty()) {
                        Text(pluralStringResource(R.plurals.requests_waiting, snap.pending.size, snap.pending.size), color = MaterialTheme.colorScheme.primary)
                    }
                }
            }
        }
        SecondaryButton(stringResource(R.string.add_minor), onClick = onNew)
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
    val scope = rememberCoroutineScope()
    var error by remember { mutableStateOf<String?>(null) }

    fun answer(r: PendingRequest, approved: Boolean) = scope.launch {
        val reason = ctx.getString(if (approved) R.string.approve_what else R.string.reject_what, describe(ctx, r.what))
        if (!confirmAsGuardian(activity, reason)) return@launch
        error = withContext(Dispatchers.IO) {
            runCatching { core.decide(CoreHolder.clock(), r.requestId, approved); core.sync(CoreHolder.clock()) }
                .exceptionOrNull()?.userMessage(ctx)
        }
        refresh++
    }

    Screen {
        Heading(name)
        snap.pending.forEach { r ->
            Section("${describe(ctx, r.what)} · ${whenText(r.atMs)}") {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    PrimaryButton(stringResource(R.string.approve)) { answer(r, true) }
                    SecondaryButton(stringResource(R.string.reject)) { answer(r, false) }
                }
            }
        }
        ErrorText(error)
        Text(stringResource(R.string.child_mirror_note, name), style = MaterialTheme.typography.bodySmall)
        MirrorPanel(snap)
        SecondaryButton(stringResource(R.string.child_pair_device, name)) { onInvite(Role.MINOR) }
        SecondaryButton(stringResource(R.string.add_guardian)) { onInvite(Role.GUARDIAN) }
        SecondaryButton(stringResource(R.string.child_settings, name), onClick = onSettings)
        TextButton(onClick = onBack) { Text(stringResource(R.string.back)) }
    }
}

/** The minor's settings (guardian). They take effect immediately: a guardian signs them. */
@Composable
fun ChildSettingsScreen(id: String, onBack: () -> Unit) {
    val ctx = LocalContext.current
    val activity = ctx as FragmentActivity
    val core = remember(id) { CoreHolder.family(ctx, id) }
    val cfg = remember(id) { core.activeConfig(CoreHolder.clock()) }
    val scope = rememberCoroutineScope()
    if (cfg == null) {
        Screen { Text(stringResource(R.string.no_config_yet)); TextButton(onClick = onBack) { Text(stringResource(R.string.back)) } }
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
        Heading(stringResource(R.string.settings))
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            OutlinedTextField(wake, { wake = it.take(5) }, label = { Text(stringResource(R.string.wake)) }, isError = parseTime(wake) == null, modifier = Modifier.width(140.dp))
            OutlinedTextField(bedtime, { bedtime = it.take(5) }, label = { Text(stringResource(R.string.bedtime)) }, isError = parseTime(bedtime) == null, modifier = Modifier.width(140.dp))
        }
        Text(stringResource(R.string.free_days), fontWeight = FontWeight.SemiBold)
        Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            dayLabels().forEachIndexed { i, d ->
                FilterChip(selected = freeDays and (1 shl i) != 0, onClick = { freeDays = freeDays xor (1 shl i) }, label = { Text(d) })
            }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
            OutlinedTextField(permits, { permits = it.filter(Char::isDigit).take(2) }, label = { Text(stringResource(R.string.passes_per_day_short)) }, keyboardOptions = number, modifier = Modifier.width(130.dp))
            OutlinedTextField(minutes, { minutes = it.filter(Char::isDigit).take(2) }, label = { Text(stringResource(R.string.minutes)) }, keyboardOptions = number, modifier = Modifier.width(110.dp))
            OutlinedTextField(bonus, { bonus = it.filter(Char::isDigit).take(2) }, label = { Text(stringResource(R.string.extra_per_week)) }, keyboardOptions = number, modifier = Modifier.width(130.dp))
        }
        OutlinedTextField(web, { web = it }, label = { Text(stringResource(R.string.focus_sites)) }, modifier = Modifier.fillMaxWidth())
        ErrorText(error)
        val wakeMin = parseTime(wake)
        val bedMin = parseTime(bedtime)
        val reason = stringResource(R.string.change_settings)
        PrimaryButton(stringResource(R.string.save_now), enabled = wakeMin != null && bedMin != null) {
            val next = cfg.copy(
                schedule = cfg.schedule.copy(wake = TimeOfDay(wakeMin!!.toUShort()), bedtime = TimeOfDay(bedMin!!.toUShort())),
                permitsPerDay = (permits.toIntOrNull() ?: 0).coerceIn(0, 10).toUByte(),
                permitMinutes = (minutes.toIntOrNull() ?: 5).coerceIn(1, 60).toUShort(),
                weeklyBonusPermits = (bonus.toIntOrNull() ?: 0).coerceIn(0, 20).toUByte(),
                freeDays = freeDays.toUByte(),
                webAllowlist = web.split(',', ' ', '\n').map { it.trim().lowercase() }.filter { it.isNotEmpty() },
            )
            scope.launch {
                if (!confirmAsGuardian(activity, reason)) return@launch
                error = withContext(Dispatchers.IO) {
                    runCatching { core.setConfig(CoreHolder.clock(), next); core.sync(CoreHolder.clock()) }.exceptionOrNull()?.userMessage(ctx)
                }
                if (error == null) onBack()
            }
        }
        TextButton(onClick = onBack) { Text(stringResource(R.string.back)) }
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
            runCatching { core.request(CoreHolder.clock(), what) }.exceptionOrNull()?.userMessage(ctx)
        }
        if (error == null) {
            message = ctx.getString(R.string.request_sent, describe(ctx, what))
            site = ""
            SyncScheduler.syncNow(ctx)
            refresh++
        }
    }

    Screen {
        Heading(stringResource(R.string.your_guardian))
        if (!vpnReady) {
            Section(stringResource(R.string.vpn_missing_title)) {
                Text(stringResource(R.string.vpn_missing_body))
                PrimaryButton(stringResource(R.string.enable)) { VpnService.prepare(ctx)?.let { askVpn.launch(it) } }
            }
        }
        Section(stringResource(R.string.ask_something)) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                SecondaryButton(stringResource(R.string.more_15_min)) { ask(RequestKind.ExtraPermit(15u)) }
                SecondaryButton(stringResource(R.string.rest_day)) { ask(RequestKind.RestDay) }
            }
            SecondaryButton(stringResource(R.string.sleep_30_later)) { ask(RequestKind.LaterBedtime(30u)) }
            OutlinedTextField(site, { site = it.take(253) }, label = { Text(stringResource(R.string.site_for_homework)) }, placeholder = { Text("khanacademy.org") }, modifier = Modifier.fillMaxWidth())
            SecondaryButton(stringResource(R.string.ask_access), enabled = site.isNotBlank()) { ask(RequestKind.AddSite(site.trim())) }
            message?.let { Text(it) }
            ErrorText(error)
        }
        Text(stringResource(R.string.what_guardian_sees), style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
        Text(stringResource(R.string.guardian_sees_exactly), style = MaterialTheme.typography.bodySmall)
        MirrorPanel(snap)
        TextButton(onClick = onBack) { Text(stringResource(R.string.back)) }
    }
}
