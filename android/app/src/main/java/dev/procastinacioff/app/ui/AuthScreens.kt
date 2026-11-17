package dev.procastinacioff.app.ui

import android.content.Context
import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.toMutableStateList
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.fragment.app.FragmentActivity
import dev.procastinacioff.app.R
import dev.procastinacioff.app.auth.LocalAuth
import dev.procastinacioff.app.core.CoreHolder
import dev.procastinacioff.app.core.userMessage
import dev.procastinacioff.app.family.confirmAsGuardian
import dev.procastinacioff.app.sync.SyncScheduler
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.procastinacioff_core.AppList
import uniffi.procastinacioff_core.AuthState
import uniffi.procastinacioff_core.Change
import uniffi.procastinacioff_core.Core
import uniffi.procastinacioff_core.CoreException
import uniffi.procastinacioff_core.LocalFactor
import uniffi.procastinacioff_core.OfflineAsk
import uniffi.procastinacioff_core.SecretHolder
import uniffi.procastinacioff_core.TimeOfDay

// Change authorization (ADR 0013): the local gate (PIN, password, pattern, biometrics), the
// trusted contact and the small concessions without network. Secrets go straight to the core
// (Argon2id there); they are never stored or logged here.

/** Runs a core call off the main thread; returns the translated error or null. */
private suspend fun call(ctx: Context, block: () -> Unit): String? =
    withContext(Dispatchers.IO) { runCatching(block).exceptionOrNull()?.userMessage(ctx) }

@StringRes
fun factorName(f: LocalFactor): Int = when (f) {
    LocalFactor.PIN -> R.string.factor_pin
    LocalFactor.PASSWORD -> R.string.factor_password
    LocalFactor.PATTERN -> R.string.factor_pattern
    LocalFactor.BIOMETRIC -> R.string.factor_biometric
}

/** What a new PIN, password or pattern needs. */
@StringRes
fun secretHint(f: LocalFactor): Int = when (f) {
    LocalFactor.PIN -> R.string.auth_pin_hint
    LocalFactor.PASSWORD -> R.string.auth_password_hint
    else -> R.string.auth_pattern_hint
}

/** Pattern: tap the dots in order (cells 0..8 of a 3×3 grid); the core receives "0-4-8-5". */
@Composable
fun PatternPad(value: String, onChange: (String) -> Unit) {
    val cells = value.split('-').filter { it.isNotEmpty() }.map { it.toInt() }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        for (r in 0 until 3) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                for (c in 0 until 3) {
                    val i = r * 3 + c
                    val pos = cells.indexOf(i)
                    FilterChip(
                        selected = pos >= 0,
                        onClick = { if (pos < 0) onChange((cells + i).joinToString("-")) },
                        label = { Text(if (pos >= 0) "${pos + 1}" else "·") },
                        modifier = Modifier.size(64.dp),
                    )
                }
            }
        }
        TextButton(onClick = { onChange("") }) { Text(stringResource(R.string.clear)) }
    }
}

@Composable
fun SecretInput(factor: LocalFactor, value: String, onChange: (String) -> Unit) {
    if (factor == LocalFactor.PATTERN) return PatternPad(value, onChange)
    OutlinedTextField(
        value,
        { onChange(it.take(128)) },
        label = { Text(stringResource(factorName(factor))) },
        visualTransformation = PasswordVisualTransformation(),
        keyboardOptions = KeyboardOptions(keyboardType = if (factor == LocalFactor.PIN) KeyboardType.NumberPassword else KeyboardType.Password),
        singleLine = true,
        modifier = Modifier.fillMaxWidth(),
    )
}

/** The local gate: biometrics (class 3, CryptoObject) or a secret. */
@Composable
fun UnlockCard(core: Core, usable: List<LocalFactor>, onUnlocked: () -> Unit) {
    val ctx = LocalContext.current
    val activity = ctx as FragmentActivity
    val scope = rememberCoroutineScope()
    val secrets = usable.filter { it != LocalFactor.BIOMETRIC }
    var factor by remember { mutableStateOf(secrets.firstOrNull()) }
    var secret by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    Section(stringResource(R.string.unlock_title)) {
        Text(stringResource(R.string.unlock_lead))
        val biometricValid = remember { LocalAuth.keyValid() }
        if (LocalFactor.BIOMETRIC in usable && LocalAuth.available(activity)) {
            if (!biometricValid) Text(stringResource(R.string.biometric_invalidated), style = MaterialTheme.typography.bodySmall)
            else PrimaryButton(stringResource(R.string.unlock_biometric)) {
                scope.launch {
                    val title = ctx.getString(R.string.unlock_title)
                    if (LocalAuth.confirm(activity, title, ctx.getString(R.string.unlock_lead))) {
                        error = call(ctx) { core.authorizeBiometric(CoreHolder.clock()) }
                        if (error == null) onUnlocked()
                    }
                }
            }
        }
        if (secrets.size > 1) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                secrets.forEach { f -> FilterChip(selected = factor == f, onClick = { factor = f; secret = "" }, label = { Text(stringResource(factorName(f))) }) }
            }
        }
        factor?.let { f ->
            SecretInput(f, secret) { secret = it }
            SecondaryButton(stringResource(R.string.unlock), enabled = secret.isNotEmpty()) {
                scope.launch {
                    error = call(ctx) { core.authorizeLocal(CoreHolder.clock(), f, secret) }
                    secret = ""
                    if (error == null) onUnlocked()
                }
            }
        }
        ErrorText(error)
    }
}

/** Forgot the secret: the recovery code resets the local methods in 72 h (the contact is told). */
@Composable
private fun RecoverySection(core: Core, state: AuthState, onDone: () -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var open by remember { mutableStateOf(false) }
    var code by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    state.resetAtMs?.let { Text(stringResource(R.string.recovery_started, whenText(it))) }
    if (!state.hasRecoveryCode || state.resetAtMs != null) return
    if (!open) return TextButton(onClick = { open = true }) { Text(stringResource(R.string.recovery_forgot)) }
    OutlinedTextField(code, { code = it.take(32) }, label = { Text(stringResource(R.string.recovery_title)) }, singleLine = true, modifier = Modifier.fillMaxWidth())
    SecondaryButton(stringResource(R.string.unlock), enabled = code.length >= 20) {
        scope.launch {
            error = call(ctx) { core.useRecoveryCode(CoreHolder.clock(), code) }
            if (error == null) onDone()
        }
    }
    ErrorText(error)
}

/** Adult: "How changes are authorized" + their basic rules. Locks again when leaving. */
@Composable
fun AuthSettingsScreen(onContact: () -> Unit, onErased: () -> Unit, onBack: () -> Unit) {
    val ctx = LocalContext.current
    val activity = ctx as FragmentActivity
    val scope = rememberCoroutineScope()
    val core = remember { CoreHolder.get(ctx) }
    var refresh by remember { mutableIntStateOf(0) }
    var state by remember { mutableStateOf<AuthState?>(null) }
    LaunchedEffect(refresh) {
        state = withContext(Dispatchers.IO) { runCatching { core.authState(CoreHolder.clock()) }.getOrNull() }
    }
    DisposableEffect(Unit) { onDispose { core.lockSettings() } }
    val title = stringResource(R.string.auth_title)
    val st = state ?: return Screen(title = title, onBack = onBack) { LoadingState() }
    Screen(title = title, onBack = onBack) {
        if (!st.unlocked) {
            UnlockCard(core, st.usable) { refresh++ }
            RecoverySection(core, st) { refresh++ }
            return@Screen
        }
        var error by remember { mutableStateOf<String?>(null) }
        var message by remember { mutableStateOf<String?>(null) }

        // ── Methods ──
        Section(stringResource(R.string.auth_methods)) {
            if (st.methods.isEmpty()) Text(stringResource(R.string.auth_no_methods), color = MaterialTheme.colorScheme.error)
            else Text(st.methods.joinToString(" · ") { ctx.getString(factorName(it)) })
            var editing by remember { mutableStateOf<LocalFactor?>(null) }
            var secret by remember { mutableStateOf("") }
            listOf(LocalFactor.PIN, LocalFactor.PASSWORD, LocalFactor.PATTERN).forEach { f ->
                val name = ctx.getString(factorName(f))
                TextButton(onClick = { editing = f; secret = "" }) {
                    Text(stringResource(if (f in st.usable) R.string.auth_change else R.string.auth_set, name))
                }
            }
            editing?.let { f ->
                Text(stringResource(secretHint(f)), style = MaterialTheme.typography.bodySmall)
                SecretInput(f, secret) { secret = it }
                PrimaryButton(stringResource(R.string.save), enabled = secret.isNotEmpty()) {
                    scope.launch {
                        error = call(ctx) { core.setSecret(CoreHolder.clock(), f, secret, SecretHolder.OWNER) }
                        secret = ""
                        if (error == null) { editing = null; refresh++ }
                    }
                }
            }
            // Also after a new fingerprint invalidated the key. SECURITY: this screen is only shown with
            // the local gate open, and an invalid key cannot open it: the PIN, password or pattern did.
            val biometricValid = remember(refresh) { LocalAuth.keyValid() }
            if ((LocalFactor.BIOMETRIC !in st.methods || !biometricValid) && LocalAuth.available(activity)) {
                SecondaryButton(stringResource(R.string.auth_use_biometric)) {
                    scope.launch {
                        // The key is created, then used once: proof that biometrics works before relying on it.
                        if (android.os.Build.VERSION.SDK_INT >= 30) runCatching { LocalAuth.ensureKey() }
                        val title = ctx.getString(R.string.auth_use_biometric)
                        if (!LocalAuth.confirm(activity, title, title)) return@launch
                        if (LocalFactor.BIOMETRIC in st.methods) {
                            refresh++
                            return@launch
                        }
                        error = call(ctx) {
                            val clock = CoreHolder.clock()
                            val cfg = core.activeConfig(clock) ?: return@call
                            core.setConfig(clock, cfg.copy(auth = cfg.auth.copy(local = cfg.auth.local + LocalFactor.BIOMETRIC)))
                        }
                        refresh++
                    }
                }
            }
        }

        // ── Recovery code ──
        if (st.methods.any { it != LocalFactor.BIOMETRIC }) {
            Section(stringResource(R.string.recovery_title)) {
                Text(stringResource(R.string.recovery_lead), style = MaterialTheme.typography.bodySmall)
                var shown by remember { mutableStateOf<String?>(null) }
                shown?.let { Text(stringResource(R.string.recovery_shown_once, it), fontWeight = FontWeight.Bold) }
                SecondaryButton(stringResource(if (st.hasRecoveryCode) R.string.recovery_new else R.string.recovery_create)) {
                    scope.launch {
                        var code: String? = null
                        error = call(ctx) { code = core.createRecoveryCode(CoreHolder.clock()) }
                        shown = code
                        refresh++
                    }
                }
            }
        }

        // ── Trusted contact ──
        Section(stringResource(R.string.contact_title)) {
            Text(if (st.contacts > 0u) pluralStringResource(R.plurals.contacts_linked, st.contacts.toInt(), st.contacts.toInt()) else stringResource(R.string.contact_nobody))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(stringResource(R.string.contact_required), Modifier.weight(1f))
                Switch(
                    checked = st.contactRequired,
                    enabled = st.contacts > 0u || st.contactRequired,
                    onCheckedChange = { on ->
                        scope.launch {
                            var from = 0L
                            error = call(ctx) {
                                val clock = CoreHolder.clock()
                                val cfg = core.activeConfig(clock) ?: return@call
                                from = core.setConfig(clock, cfg.copy(auth = cfg.auth.copy(contactRequired = on)))
                            }
                            if (error == null) message = appliesText(ctx, from)
                            refresh++
                        }
                    },
                )
            }
            Text(stringResource(R.string.contact_required_hint), style = MaterialTheme.typography.bodySmall)
            SecondaryButton(stringResource(R.string.contact_invite), onClick = onContact)
        }

        RulesSection(core, st) { message = it; refresh++ }
        if (st.contactRequired) {
            Section(stringResource(R.string.offline_title)) {
                OfflineAskSection(core, channel = { CoreHolder.contactChannelOrNull(ctx) }) { message = it }
            }
        }
        message?.let { IconText(PcoIcons.Check, it) }
        ErrorText(error)
        SupportReportSection(core)
        // Destructive actions apart, last
        Spacer(Modifier.height(16.dp))
        EraseSection(st.contactRequired, onErased)
    }
}

/**
 * Technical report for support (plan 10 · 4): versions, states and counts only. The user reads it
 * here and decides whether to share it (nothing is sent automatically).
 */
@Composable
private fun SupportReportSection(core: Core) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var report by remember { mutableStateOf<String?>(null) }
    Section(stringResource(R.string.report_title)) {
        Text(stringResource(R.string.report_lead), style = MaterialTheme.typography.bodySmall)
        report?.let { Text(it, style = MaterialTheme.typography.bodySmall, fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace) }
        if (report == null) {
            SecondaryButton(stringResource(R.string.report_create)) {
                scope.launch {
                    report = withContext(Dispatchers.IO) {
                        val version = runCatching { ctx.packageManager.getPackageInfo(ctx.packageName, 0).versionName }.getOrNull()
                        val notify = androidx.core.content.ContextCompat.checkSelfPermission(ctx, android.Manifest.permission.POST_NOTIFICATIONS) ==
                            android.content.pm.PackageManager.PERMISSION_GRANTED
                        val platform = listOf(
                            "app=$version android=${android.os.Build.VERSION.SDK_INT}",
                            "accessibility=${dev.procastinacioff.app.sync.isAccessibilityEnabled(ctx)} notifications=$notify",
                        )
                        runCatching { core.diagnosticReport(CoreHolder.clock(), platform) }.getOrElse { platform.joinToString("\n") }
                    }
                }
            }
        } else {
            PrimaryButton(stringResource(R.string.report_share)) {
                val send = android.content.Intent(android.content.Intent.ACTION_SEND).setType("text/plain").putExtra(android.content.Intent.EXTRA_TEXT, report)
                ctx.startActivity(android.content.Intent.createChooser(send, ctx.getString(R.string.report_share)))
            }
        }
    }
}

/** "Delete all my data". Here the local gate is already open; the core checks it again. */
@Composable
private fun EraseSection(contactRequired: Boolean, onErased: () -> Unit) {
    val ctx = LocalContext.current
    var confirming by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    Section(stringResource(R.string.erase_title)) {
        Text(stringResource(R.string.erase_lead), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        when {
            contactRequired -> Footnote(stringResource(R.string.erase_needs_contact), PcoIcons.UserCheck)
            !confirming -> SecondaryButton(stringResource(R.string.erase_button), danger = true) { confirming = true }
            else -> {
                ErrorText(stringResource(R.string.erase_confirm))
                // The confirmation step uses the normal primary button (danger is text, never a fill)
                PrimaryButton(stringResource(R.string.erase_confirm_button)) {
                    error = withContext(Dispatchers.IO) { runCatching { CoreHolder.eraseAll(ctx) }.exceptionOrNull()?.userMessage(ctx) }
                    if (error == null) onErased()
                }
                TextButton(onClick = { confirming = false }, modifier = Modifier.align(Alignment.CenterHorizontally)) { Text(stringResource(R.string.cancel)) }
            }
        }
        ErrorText(error)
    }
}

private fun appliesText(ctx: Context, fromMs: Long): String =
    if (fromMs <= System.currentTimeMillis() + 60_000) ctx.getString(R.string.applies_now) else ctx.getString(R.string.applies_from, whenText(fromMs))

/** The adult's basic rules. Relaxing applies tomorrow, or asks the trusted contact if required. */
@Composable
private fun RulesSection(core: Core, st: AuthState, onResult: (String) -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val cfg = remember(st) { core.activeConfig(CoreHolder.clock()) } ?: return
    fun hm(t: TimeOfDay) = "%02d:%02d".format(t.minutes.toInt() / 60, t.minutes.toInt() % 60)
    var wake by remember(cfg) { mutableStateOf(hm(cfg.schedule.wake)) }
    var bedtime by remember(cfg) { mutableStateOf(hm(cfg.schedule.bedtime)) }
    var permits by remember(cfg) { mutableStateOf(cfg.permitsPerDay.toString()) }
    var minutes by remember(cfg) { mutableStateOf(cfg.permitMinutes.toString()) }
    val goals = remember(cfg) { cfg.goals.toMutableStateList() }
    var error by remember { mutableStateOf<String?>(null) }
    var pending by remember { mutableStateOf<uniffi.procastinacioff_core.Config?>(null) }
    Section(stringResource(R.string.your_rules)) {
        TimeFields(wake, { wake = it }, bedtime, { bedtime = it })
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            NumberField(permits, { permits = it }, stringResource(R.string.passes_per_day_short), Modifier.weight(1f))
            NumberField(minutes, { minutes = it }, stringResource(R.string.minutes), Modifier.weight(1f))
        }
        Text(stringResource(R.string.fixed_goals), style = MaterialTheme.typography.titleMedium)
        FixedGoalsEditor(goals)
        val wakeMin = parseTime(wake)
        val bedMin = parseTime(bedtime)
        PrimaryButton(stringResource(R.string.save), enabled = wakeMin != null && bedMin != null) {
            val next = cfg.copy(
                schedule = cfg.schedule.copy(wake = TimeOfDay(wakeMin!!.toUShort()), bedtime = TimeOfDay(bedMin!!.toUShort())),
                permitsPerDay = (permits.toIntOrNull() ?: 0).coerceIn(0, 10).toUByte(),
                permitMinutes = (minutes.toIntOrNull() ?: 5).coerceIn(1, 60).toUShort(),
                goals = goals.toList(),
            )
            scope.launch {
                var from = 0L
                val failure = withContext(Dispatchers.IO) { runCatching { from = core.setConfig(CoreHolder.clock(), next) }.exceptionOrNull() }
                pending = next.takeIf { failure is CoreException.Denied && failure.reason == "needs_contact" }
                error = failure?.takeIf { pending == null }?.userMessage(ctx)
                if (failure == null) { onResult(appliesText(ctx, from)); SyncScheduler.syncNow(ctx) }
            }
        }
        pending?.let { next ->
            Text(stringResource(R.string.denied_needs_contact))
            SecondaryButton(stringResource(R.string.send_to_contact)) {
                scope.launch {
                    error = call(ctx) {
                        val channel = CoreHolder.contactChannelOrNull(ctx) ?: throw IllegalStateException("no contact channel")
                        core.requestChange(CoreHolder.clock(), next, channel)
                    }
                    if (error == null) { pending = null; onResult(ctx.getString(R.string.sent_to_contact)); SyncScheduler.syncNow(ctx) }
                }
            }
        }
        ErrorText(error)
    }
}

fun describeAsk(ctx: Context, a: OfflineAsk): String = when (a) {
    is OfflineAsk.ExtraPermit -> ctx.getString(R.string.req_extra_today, a.minutes.toInt())
    is OfflineAsk.RestDay -> ctx.getString(R.string.req_rest_today)
    is OfflineAsk.LaterBedtime -> ctx.getString(R.string.req_bedtime_tonight, a.minutes.toInt())
}

private val ASKS = listOf(OfflineAsk.ExtraPermit(15u), OfflineAsk.RestDay, OfflineAsk.LaterBedtime(30u))

/** Without network: ask for a small concession with a code read to the approver, then type their 8 digits. */
@Composable
fun OfflineAskSection(core: Core, channel: () -> Core?, onDone: (String) -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var code by remember { mutableStateOf<String?>(null) }
    var digits by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    ASKS.forEach { ask ->
        SecondaryButton(describeAsk(ctx, ask)) {
            scope.launch {
                var c: String? = null
                error = call(ctx) { c = core.offlineRequest(CoreHolder.clock(), ask) }
                code = c
            }
        }
    }
    code?.let { c ->
        Text(stringResource(R.string.offline_lead))
        Text(stringResource(R.string.offline_code, c), style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
        OutlinedTextField(digits, { digits = it.filter(Char::isDigit).take(8) }, label = { Text(stringResource(R.string.offline_digits)) }, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number), singleLine = true)
        PrimaryButton(stringResource(R.string.offline_redeem), enabled = digits.length == 8) {
            scope.launch {
                error = call(ctx) { core.offlineRedeem(CoreHolder.clock(), digits, channel()) }
                digits = ""
                if (error == null) { code = null; onDone(ctx.getString(R.string.offline_done)); SyncScheduler.syncNow(ctx) }
            }
        }
    }
    ErrorText(error)
}

/** (Minor's device) "My guardian is here": the guardian types THEIR secret. Never the tablet's biometrics. */
@Composable
fun GuardianHereSection(core: Core, onDone: (String) -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val methods = remember { runCatching { core.authState(CoreHolder.clock()).guardianMethods }.getOrDefault(emptyList()) }
    if (methods.isEmpty()) return Text(stringResource(R.string.guardian_here_none), style = MaterialTheme.typography.bodySmall)
    var ask by remember { mutableStateOf<OfflineAsk>(ASKS.first()) }
    var factor by remember { mutableStateOf(methods.first()) }
    var secret by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    Text(stringResource(R.string.guardian_here_lead))
    ASKS.forEach { a -> FilterChip(selected = ask == a, onClick = { ask = a }, label = { Text(describeAsk(ctx, a)) }) }
    if (methods.size > 1) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            methods.forEach { f -> FilterChip(selected = factor == f, onClick = { factor = f; secret = "" }, label = { Text(stringResource(factorName(f))) }) }
        }
    }
    SecretInput(factor, secret) { secret = it }
    PrimaryButton(stringResource(R.string.approve), enabled = secret.isNotEmpty()) {
        scope.launch {
            error = call(ctx) { core.grantInPerson(CoreHolder.clock(), factor, secret, ask) }
            secret = ""
            if (error == null) { onDone(ctx.getString(R.string.offline_done)); SyncScheduler.syncNow(ctx) }
        }
    }
    ErrorText(error)
}

/** (Guardian or trusted contact) Answer a code read to you: see what it asks, confirm, read back 8 digits. */
@Composable
fun AnswerCodeSection(core: Core) {
    val ctx = LocalContext.current
    val activity = ctx as FragmentActivity
    val scope = rememberCoroutineScope()
    var code by remember { mutableStateOf("") }
    var asked by remember { mutableStateOf<String?>(null) }
    var answer by remember { mutableStateOf<String?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    Section(stringResource(R.string.answer_code_title)) {
        OutlinedTextField(code, { code = it.take(16); asked = null; answer = null }, label = { Text(stringResource(R.string.answer_code_field)) }, singleLine = true, modifier = Modifier.fillMaxWidth())
        PrimaryButton(stringResource(R.string.answer_code_check), enabled = code.length >= 10) {
            scope.launch {
                var result: uniffi.procastinacioff_core.OfflineAnswer? = null
                error = call(ctx) { result = core.offlineAnswer(code) }
                val r = result ?: return@launch
                asked = describeAsk(ctx, r.ask)
                // Someone else holding this phone cannot hand out codes.
                if (confirmAsGuardian(activity, ctx.getString(R.string.answer_code_asks, asked), ctx.getString(R.string.confirm_identity))) {
                    answer = r.response
                }
            }
        }
        asked?.let { Text(stringResource(R.string.answer_code_asks, it)) }
        answer?.let { Text(stringResource(R.string.answer_code_give, it.chunked(4).joinToString(" ")), style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold) }
        ErrorText(error)
    }
}

/** (Guardian) The PIN or pattern they can type on the minor's devices ("my guardian is here"). */
@Composable
fun GuardianSecretSection(core: Core, childName: String) {
    val ctx = LocalContext.current
    val activity = ctx as FragmentActivity
    val scope = rememberCoroutineScope()
    var factor by remember { mutableStateOf(LocalFactor.PIN) }
    var secret by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    var saved by remember { mutableStateOf(false) }
    Section(stringResource(R.string.guardian_secret_title, childName)) {
        Text(stringResource(R.string.guardian_secret_lead), style = MaterialTheme.typography.bodySmall)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf(LocalFactor.PIN, LocalFactor.PATTERN).forEach { f -> FilterChip(selected = factor == f, onClick = { factor = f; secret = "" }, label = { Text(stringResource(factorName(f))) }) }
        }
        SecretInput(factor, secret) { secret = it }
        PrimaryButton(stringResource(R.string.save), enabled = secret.isNotEmpty()) {
            scope.launch {
                if (!confirmAsGuardian(activity, ctx.getString(R.string.guardian_secret_title, childName))) return@launch
                error = call(ctx) { core.setSecret(CoreHolder.clock(), factor, secret, SecretHolder.GUARDIAN); CoreHolder.syncNow(core) }
                secret = ""
                saved = error == null
            }
        }
        if (saved) Text(stringResource(R.string.offline_done))
        ErrorText(error)
    }
}

private fun days(ctx: Context, mask: UByte): String {
    val labels = dayLabels()
    return labels.indices.filter { mask.toInt() and (1 shl it) != 0 }.joinToString(" ") { labels[it] }.ifEmpty { ctx.getString(R.string.none) }
}

private fun hhmm(t: TimeOfDay) = "%02d:%02d".format(t.minutes.toInt() / 60, t.minutes.toInt() % 60)

/** One typed line of a requested change, as the trusted contact sees it. */
fun changeText(ctx: Context, c: Change): String {
    fun fromTo(@StringRes label: Int, from: Any, to: Any) = ctx.getString(R.string.change_from_to, ctx.getString(label), from.toString(), to.toString())
    fun list(l: AppList) = ctx.getString(if (l == AppList.ESSENTIAL) R.string.app_essential else R.string.check_focus)
    fun methods(v: List<LocalFactor>) = v.joinToString(", ") { ctx.getString(factorName(it)) }.ifEmpty { ctx.getString(R.string.none) }
    return when (c) {
        is Change.Permits -> fromTo(R.string.passes_per_day_short, c.from, c.to)
        is Change.PermitMinutes -> fromTo(R.string.label_permit_minutes, c.from, c.to)
        is Change.Cooldown -> fromTo(R.string.label_cooldown, c.from, c.to)
        is Change.EmergencyMinutes -> fromTo(R.string.label_emergency_minutes, c.from, c.to)
        is Change.EmergencyPenalty -> fromTo(R.string.label_emergency_penalty, c.from, c.to)
        is Change.HabitPenalty -> ctx.getString(if (c.on) R.string.change_habit_on else R.string.change_habit_off)
        is Change.Bedtime -> fromTo(R.string.bedtime, hhmm(c.from), hhmm(c.to))
        is Change.Wake -> fromTo(R.string.wake, hhmm(c.from), hhmm(c.to))
        is Change.GoalAdded -> ctx.getString(R.string.change_goal_added, c.title)
        is Change.GoalRemoved -> ctx.getString(R.string.change_goal_removed, c.title)
        is Change.GoalChanged -> ctx.getString(R.string.change_goal_changed, c.title)
        is Change.FreeDays -> fromTo(R.string.free_days, days(ctx, c.from), days(ctx, c.to))
        is Change.AppAdded -> ctx.getString(R.string.change_app_added, c.app.substringAfterLast(':'), list(c.list))
        is Change.AppRemoved -> ctx.getString(R.string.change_app_removed, c.app.substringAfterLast(':'), list(c.list))
        is Change.SiteAdded -> ctx.getString(R.string.change_site_added, c.domain)
        is Change.SiteRemoved -> ctx.getString(R.string.change_site_removed, c.domain)
        is Change.WeeklyBonus -> fromTo(R.string.extra_per_week, c.from, c.to)
        is Change.LocalMethods -> fromTo(R.string.label_methods, methods(c.from), methods(c.to))
        is Change.ContactRequired -> ctx.getString(if (c.on) R.string.change_contact_on else R.string.change_contact_off)
    }
}
