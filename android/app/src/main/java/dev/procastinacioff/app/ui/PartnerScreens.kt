package dev.procastinacioff.app.ui

import android.content.Context
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import dev.procastinacioff.app.R
import android.os.Build
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.procastinacioff.app.core.CoreHolder
import dev.procastinacioff.app.core.userMessage
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.procastinacioff_core.PartnerAlert
import uniffi.procastinacioff_core.PartnerAlertView
import uniffi.procastinacioff_core.TamperKind

@Composable
private fun Bullets(title: String, items: List<String>) {
    Text(title, fontWeight = FontWeight.SemiBold)
    items.forEach { Text("• $it", color = MaterialTheme.colorScheme.onSurfaceVariant) }
}

/** Invite my accountability partner. Explains, first of all, what they will and will not see. */
@Composable
fun PartnerScreen(onInvite: () -> Unit, onBack: () -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var linked by remember { mutableStateOf<Int?>(null) } // paired partners (null = no mailbox)
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        linked = withContext(Dispatchers.IO) { CoreHolder.partnerBoxOrNull(ctx)?.members()?.size?.minus(1) }
    }

    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Text(stringResource(R.string.partner_title), style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
        Text(stringResource(R.string.partner_lead))
        Bullets(stringResource(R.string.partner_will_see), listOf(
            stringResource(R.string.partner_sees_tamper),
            stringResource(R.string.partner_sees_emergency),
            stringResource(R.string.partner_sees_count),
        ))
        Bullets(stringResource(R.string.partner_never_sees), listOf(stringResource(R.string.partner_never_goals), stringResource(R.string.partner_never_apps)))
        Text(stringResource(R.string.partner_from_today), style = MaterialTheme.typography.bodySmall)
        ErrorText(error)
        linked?.let { n ->
            Text(if (n > 0) pluralStringResource(R.plurals.partners_linked, n, n) else stringResource(R.string.partner_nobody))
        }
        PrimaryButton(stringResource(if (linked == null) R.string.partner_invite else R.string.partner_invite_other), enabled = !busy) {
            busy = true
            scope.launch {
                // Creates the mailbox (once) and connects it to the profile's relay.
                error = withContext(Dispatchers.IO) {
                    val url = CoreHolder.get(ctx).relayUrl()
                    if (url == null) {
                        ctx.getString(R.string.partner_needs_sync)
                    } else {
                        runCatching {
                            val box = CoreHolder.partnerBox(ctx)
                            if (box.members().isEmpty()) box.createPartnerMailbox(CoreHolder.clock(), Build.MODEL.take(64))
                            if (box.relayUrl() == null) box.connectRelay(url)
                        }.exceptionOrNull()?.userMessage(ctx)
                    }
                }
                busy = false
                if (error == null) onInvite()
            }
        }
        TextButton(onClick = onBack) { Text(stringResource(R.string.back)) }
    }
}

private fun format(skeleton: String, ms: Long): String =
    SimpleDateFormat(android.text.format.DateFormat.getBestDateTimePattern(Locale.getDefault(), skeleton), Locale.getDefault()).format(Date(ms))

private fun describe(ctx: Context, a: PartnerAlertView): Pair<String, String> = when (val alert = a.alert) {
    is PartnerAlert.Emergency -> ctx.getString(R.string.partner_alert_emergency) to format("EEEdMMMjmm", a.happenedAtMs)
    is PartnerAlert.Tamper -> ctx.getString(
        when (alert.what) {
            TamperKind.PERMISSION_REVOKED -> R.string.tamper_permission_revoked
            TamperKind.SERVICE_STOPPED -> R.string.tamper_service_stopped
            TamperKind.CLOCK_CHANGED -> R.string.tamper_clock_changed
            TamperKind.UNINSTALL_ATTEMPT -> R.string.tamper_uninstall_attempt
            TamperKind.SAFE_MODE -> R.string.tamper_safe_mode
            TamperKind.DEVICE_SILENT -> R.string.tamper_device_silent
        },
    ) to format("EEEdMMMjmm", a.happenedAtMs)
    is PartnerAlert.DailySummary ->
        ctx.resources.getQuantityString(R.plurals.partner_alert_summary, alert.total.toInt(), alert.completed.toInt(), alert.total.toInt()) to format("EEEEdMMMM", a.happenedAtMs)
}

/** What the partner sees: only alerts and numbers, never goals or texts. */
@Composable
fun WatchScreen() {
    val ctx = LocalContext.current
    var alerts by remember { mutableStateOf<List<PartnerAlertView>>(emptyList()) }
    LaunchedEffect(Unit) {
        while (true) {
            alerts = withContext(Dispatchers.IO) {
                runCatching {
                    val box = CoreHolder.watchingBox(ctx)
                    runCatching { box.sync(CoreHolder.clock()) }
                    box.partnerAlerts()
                }.getOrDefault(emptyList())
            }
            delay(60_000)
        }
    }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(stringResource(R.string.watch_title), style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
        Text(
            stringResource(R.string.watch_lead),
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        if (alerts.isEmpty()) Text(stringResource(R.string.watch_empty))
        alerts.forEach { a ->
            val (what, `when`) = describe(ctx, a)
            Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface)) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(what, fontWeight = FontWeight.SemiBold)
                    Text("${`when`.replaceFirstChar { it.uppercase() }} · ${a.deviceLabel}", style = MaterialTheme.typography.bodySmall)
                }
            }
        }
    }
}
