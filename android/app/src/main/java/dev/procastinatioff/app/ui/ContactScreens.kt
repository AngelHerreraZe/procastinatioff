package dev.procastinatioff.app.ui

import android.content.Context
import android.os.Build
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.fragment.app.FragmentActivity
import dev.procastinatioff.app.R
import dev.procastinatioff.app.core.CoreHolder
import dev.procastinatioff.app.core.userMessage
import dev.procastinatioff.app.family.confirmAsGuardian
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.procastinatioff_core.ContactAlert
import uniffi.procastinatioff_core.ContactAlertView
import uniffi.procastinatioff_core.Effective
import uniffi.procastinatioff_core.PendingRequest
import uniffi.procastinatioff_core.RequestKind
import uniffi.procastinatioff_core.TamperKind

/** Invite my trusted contact. Explains, first of all, what they will and will not see. */
@Composable
fun ContactScreen(onInvite: () -> Unit, onBack: () -> Unit) {
    val ctx = LocalContext.current
    var linked by remember { mutableStateOf<Int?>(null) } // paired contacts (null = no channel)
    var error by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(Unit) {
        linked = withContext(Dispatchers.IO) { CoreHolder.contactChannelOrNull(ctx)?.members()?.size?.minus(1) }
    }

    Screen(
        title = stringResource(R.string.contact_title),
        lead = stringResource(R.string.contact_lead),
        onBack = onBack,
        bottom = {
            ErrorText(error)
            PrimaryButton(stringResource(if (linked == null) R.string.contact_invite else R.string.contact_invite_other)) {
                // Creates the channel (once); the contact pairs with it and syncs directly with this phone.
                error = withContext(Dispatchers.IO) {
                    runCatching {
                        val channel = CoreHolder.contactChannel(ctx)
                        if (channel.members().isEmpty()) channel.createContactChannel(CoreHolder.clock(), Build.MODEL.take(64))
                    }.exceptionOrNull()?.userMessage(ctx)
                }
                if (error == null) onInvite()
            }
        },
    ) {
        linked?.let { n ->
            StatusBadge(if (n > 0) pluralStringResource(R.plurals.contacts_linked, n, n) else stringResource(R.string.contact_nobody), if (n > 0) Tone.SUCCESS else Tone.NEUTRAL)
        }
        Section(stringResource(R.string.contact_will_see)) {
            listOf(R.string.contact_sees_tamper, R.string.contact_sees_emergency, R.string.contact_sees_count)
                .forEach { IconText(PcoIcons.Check, stringResource(it)) }
        }
        Section(stringResource(R.string.contact_never_sees)) {
            listOf(R.string.contact_never_goals, R.string.contact_never_apps).forEach { IconText(PcoIcons.EyeOff, stringResource(it)) }
        }
        Footnote(stringResource(R.string.contact_from_today))
    }
}

private fun format(skeleton: String, ms: Long): String =
    SimpleDateFormat(android.text.format.DateFormat.getBestDateTimePattern(Locale.getDefault(), skeleton), Locale.getDefault()).format(Date(ms))

private fun describe(ctx: Context, a: ContactAlertView): Pair<String, String> = when (val alert = a.alert) {
    is ContactAlert.Emergency -> ctx.getString(R.string.contact_alert_emergency) to format("EEEdMMMjmm", a.happenedAtMs)
    is ContactAlert.Tamper -> ctx.getString(
        when (alert.what) {
            TamperKind.PERMISSION_REVOKED -> R.string.tamper_permission_revoked
            TamperKind.SERVICE_STOPPED -> R.string.tamper_service_stopped
            TamperKind.CLOCK_CHANGED -> R.string.tamper_clock_changed
            TamperKind.UNINSTALL_ATTEMPT -> R.string.tamper_uninstall_attempt
            TamperKind.SAFE_MODE -> R.string.tamper_safe_mode
            TamperKind.DEVICE_SILENT -> R.string.tamper_device_silent
        },
    ) to format("EEEdMMMjmm", a.happenedAtMs)
    is ContactAlert.DailySummary ->
        ctx.resources.getQuantityString(R.plurals.contact_alert_summary, alert.total.toInt(), alert.completed.toInt(), alert.total.toInt()) to format("EEEEdMMMM", a.happenedAtMs)
    is ContactAlert.CodeGrant -> ctx.getString(R.string.contact_alert_code) to format("EEEdMMMjmm", a.happenedAtMs)
    is ContactAlert.LocalReset -> ctx.getString(R.string.contact_alert_reset) to format("EEEdMMMjmm", a.happenedAtMs)
    is ContactAlert.ExitScheduled -> ctx.getString(R.string.contact_alert_exit) to format("EEEdMMMjmm", a.happenedAtMs)
}

/**
 * Contact mode (nothing is blocked on this phone): the changes the person asks me to approve,
 * answering their offline codes, and their alerts. Never their goals, apps or history.
 */
@Composable
fun WatchScreen() {
    val ctx = LocalContext.current
    val activity = ctx as FragmentActivity
    val scope = rememberCoroutineScope()
    val box = remember { CoreHolder.watchingBox(ctx) }
    var alerts by remember { mutableStateOf<List<ContactAlertView>>(emptyList()) }
    var requests by remember { mutableStateOf<List<PendingRequest>>(emptyList()) }
    var refresh by remember { mutableIntStateOf(0) }
    var error by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(refresh) {
        while (true) {
            withContext(Dispatchers.IO) {
                runCatching { CoreHolder.syncNow(box) }
                alerts = runCatching { box.contactAlerts() }.getOrDefault(emptyList())
                requests = runCatching { box.pendingRequests() }.getOrDefault(emptyList())
            }
            delay(60_000)
        }
    }

    fun answer(r: PendingRequest, approved: Boolean, effective: Effective) = scope.launch {
        val what = ctx.getString(if (approved) R.string.approve_what else R.string.reject_what, ctx.getString(R.string.req_config_change))
        if (!confirmAsGuardian(activity, what, ctx.getString(R.string.confirm_identity))) return@launch
        error = withContext(Dispatchers.IO) {
            runCatching { box.contactDecide(CoreHolder.clock(), r.requestId, approved, effective); CoreHolder.syncNow(box) }
                .exceptionOrNull()?.userMessage(ctx)
        }
        refresh++
    }

    Screen(title = stringResource(R.string.watch_title), lead = stringResource(R.string.watch_lead)) {
        requests.forEach { r ->
            val changes = (r.what as? RequestKind.ConfigChange)?.changes ?: return@forEach
            Section(stringResource(R.string.contact_requests)) {
                changes.forEach { IconText(PcoIcons.ChevronRight, changeText(ctx, it)) }
                PrimaryButton(stringResource(R.string.approve_today)) { answer(r, true, Effective.NOW) }
                SecondaryButton(stringResource(R.string.approve_tomorrow)) { answer(r, true, Effective.NEXT_WAKE) }
                TextButton(onClick = { answer(r, false, Effective.NEXT_WAKE) }, modifier = Modifier.align(Alignment.CenterHorizontally)) {
                    Text(stringResource(R.string.reject))
                }
            }
        }
        ErrorText(error)
        AnswerCodeSection(box)
        if (alerts.isEmpty()) EmptyState(PcoIcons.Bell, stringResource(R.string.watch_empty))
        alerts.forEach { a ->
            val (what, `when`) = describe(ctx, a)
            AppCard(padding = 16.dp) {
                Column(Modifier.semantics(mergeDescendants = true) {}, verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(what, style = MaterialTheme.typography.titleMedium)
                    Text("${`when`.replaceFirstChar { it.uppercase() }} · ${a.deviceLabel}", style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
        }
    }
}
