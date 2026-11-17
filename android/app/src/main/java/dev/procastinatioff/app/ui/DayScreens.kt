package dev.procastinatioff.app.ui

import android.Manifest
import android.content.pm.PackageManager
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.layout.height
import androidx.core.content.ContextCompat
import androidx.health.connect.client.PermissionController
import dev.procastinatioff.app.goals.HealthMinutes
import uniffi.procastinatioff_core.Check
import uniffi.procastinatioff_core.checkUsesMinutes
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import dev.procastinatioff.app.R
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.KeyboardOptions
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
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.lifecycle.compose.LifecycleResumeEffect
import dev.procastinatioff.app.core.CoreHolder
import dev.procastinatioff.app.sync.isAccessibilityEnabled
import dev.procastinatioff.app.verify.ensureVerifier
import dev.procastinatioff.app.verify.onVerifierResult
import dev.procastinatioff.app.verify.verifierIntent
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.procastinatioff_core.DayState
import uniffi.procastinatioff_core.Goal
import uniffi.procastinatioff_core.GoalKind
import uniffi.procastinatioff_core.GoalProgress
import uniffi.procastinatioff_core.Method
import uniffi.procastinatioff_core.Status

private const val ALL_DAYS: UByte = 0x7Fu
private val PLAN_CHECKS = offeredChecks(guardian = false, plan = true)

@Composable
private fun actionLabel(g: GoalProgress, def: Goal?): String = stringResource(
    if (def?.methods?.contains(Method.FOCUS_SESSION) == true) (if (g.focusMinutes > 0u) R.string.action_continue else R.string.action_start)
    else R.string.mark_done,
)

// ───────────────────────── Today (01 Calm) ─────────────────────────

@Composable
fun HomeScreen(
    snap: Snapshot,
    onStart: (Goal) -> Unit,
    onFocus: (String) -> Unit,
    onPlan: () -> Unit,
    onPair: () -> Unit,
    onSettings: () -> Unit,
    onPermissions: () -> Unit,
    minor: Boolean = false,
    onTutor: () -> Unit = {},
    onFamily: (() -> Unit)? = null,
) {
    val ctx = LocalContext.current
    var error by remember { mutableStateOf<String?>(null) }
    var blocking by remember { mutableStateOf(true) }
    LifecycleResumeEffect(Unit) {
        blocking = isAccessibilityEnabled(ctx)
        onPauseOrDispose { }
    }
    val st = snap.status ?: return Screen { LoadingState() }
    val state = st.state
    val keys = st.goals.filter { it.kind == GoalKind.KEY }
    val pending = keys.filter { !it.completed }
    val next = pending.firstOrNull() ?: st.goals.firstOrNull { !it.completed }
    val nextDef = next?.let { n -> snap.goals.firstOrNull { it.id == n.goalId } }
    val canWork = state is DayState.Locked || state is DayState.Free

    Screen(
        eyebrow = todayText(),
        title = stringResource(R.string.today),
        actions = {
            if (!st.freeDay && !st.needsPlan) IconAction(PcoIcons.Plus, stringResource(R.string.add_goal), onPlan)
            if (minor) {
                // A minor's device does not pair nor have a trusted contact: the guardian decides that
                IconAction(PcoIcons.User, stringResource(R.string.your_guardian), onTutor)
            } else {
                IconAction(PcoIcons.Smartphone, stringResource(R.string.pair_short), onPair)
                onFamily?.let { IconAction(PcoIcons.Users, stringResource(R.string.family_short), it) }
                IconAction(PcoIcons.Settings, stringResource(R.string.settings), onSettings)
            }
        },
        // Thumb zone: one main action, the pass and the emergency
        bottom = {
            ErrorText(error)
            if (state is DayState.Focus) {
                PrimaryButton(stringResource(R.string.back_to_task)) { onFocus(state.goalId) }
            } else if (canWork && next != null && nextDef != null) {
                val left = next.targetMinutes.toInt() - next.focusMinutes.toInt()
                val action = actionLabel(next, nextDef)
                val label = if (next.targetMinutes > 0u) stringResource(R.string.primary_action_left, action, next.title, duration(maxOf(0, left)))
                else stringResource(R.string.primary_action, action, next.title)
                PrimaryButton(label) { onStart(nextDef) }
            }
            if (state is DayState.Locked || state is DayState.Focus) {
                SecondaryButton(stringResource(R.string.use_pass_count, st.permitsLeft.toInt()), enabled = st.permitsLeft > 0u) {
                    error = coreAction(ctx) { it.startPass(CoreHolder.clock()) }
                }
                EmergencySection { error = coreAction(ctx) { it.emergency(CoreHolder.clock()) } }
            }
        },
    ) {
        // System state always visible: without Accessibility nothing is blocked on this phone
        if (!blocking) {
            AppCard(padding = 16.dp) {
                IconText(PcoIcons.Alert, stringResource(R.string.protection_off))
                LinkRow(stringResource(R.string.review_permissions), onPermissions)
            }
        }
        // Cross heartbeats: a device that stops reporting shows up on the others
        silentDevices(snap).forEach { d ->
            AppCard(Modifier.semantics { liveRegion = LiveRegionMode.Polite }, padding = 16.dp) {
                IconText(PcoIcons.Smartphone, stringResource(R.string.device_silent, d.label, hoursAgo(ctx, d.lastSeenMs, snap.nowMs)))
            }
        }
        StatusCard(st, snap.nowMs, pending.size, next?.title)
        if (st.goals.isNotEmpty()) {
            SectionHeader(stringResource(R.string.today_goals))
            st.goals.forEach { g ->
                val def = snap.goals.firstOrNull { it.id == g.goalId }
                GoalRow(g, if (!g.completed && canWork && def != null) actionLabel(g, def) else null) { def?.let(onStart) }
            }
        }
    }
}

private fun stateIcon(st: Status): ImageVector = when (st.state) {
    is DayState.Locked -> PcoIcons.Lock
    is DayState.Pass -> PcoIcons.Ticket
    is DayState.Emergency -> PcoIcons.Alert
    is DayState.Focus -> PcoIcons.Clock
    is DayState.Free -> PcoIcons.Unlock
    is DayState.Sleep -> PcoIcons.Moon
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun StatusCard(st: Status, now: Long, pending: Int, nextTitle: String?) {
    val state = st.state
    val title = when (state) {
        is DayState.Locked -> pluralStringResource(R.plurals.title_locked, pending, pending)
        is DayState.Pass -> stringResource(R.string.title_pass, minutesUntil(state.untilMs, now))
        is DayState.Emergency -> stringResource(R.string.title_emergency, minutesUntil(state.untilMs, now))
        is DayState.Focus -> stringResource(R.string.title_focus)
        is DayState.Free -> stringResource(if (st.freeDay) R.string.title_free_day else R.string.title_free)
        is DayState.Sleep -> stringResource(R.string.bedtime_title)
    }
    val sentence = when (state) {
        is DayState.Locked -> nextTitle?.let { pluralStringResource(R.plurals.sentence_locked, pending, pending, it) } ?: ""
        is DayState.Pass -> stringResource(R.string.sentence_pass)
        is DayState.Emergency -> stringResource(R.string.sentence_emergency)
        is DayState.Focus -> stringResource(R.string.focus_only_task_apps)
        is DayState.Free -> if (st.freeDay) stringResource(R.string.sentence_free_day)
        else st.sleepAtMs?.let { stringResource(R.string.sentence_free_until, hhmm(it)) } ?: stringResource(R.string.sentence_free_tomorrow)
        is DayState.Sleep -> stringResource(R.string.calls_keep_working)
    }
    AppCard {
        Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
            IconCircle(stateIcon(st))
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(title, style = MaterialTheme.typography.titleLarge)
                if (sentence.isNotEmpty()) Text(sentence, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        val sleep = st.sleepAtMs?.let { stringResource(R.string.sleep_at, hhmm(it)) }
        val passes = if (st.freeDay) null else pluralStringResource(R.plurals.passes, st.permitsLeft.toInt(), st.permitsLeft.toInt())
        if (sleep != null || passes != null) {
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                sleep?.let { InfoChip(PcoIcons.Moon, it) }
                passes?.let { InfoChip(PcoIcons.Ticket, it) }
            }
        }
    }
}

// ───────────────────────── Day plan ─────────────────────────

private data class PlanRow(val goal: Goal, val locked: Boolean)

/**
 * Opens by itself at the first unlock after bedtime. The fixed goals are already included
 * (they can be raised, not removed). Once confirmed, the plan can only be tightened until tomorrow.
 */
@Composable
fun PlanScreen(snap: Snapshot, adding: Boolean, onDone: () -> Unit, onCancel: () -> Unit) {
    val ctx = LocalContext.current
    val rows = remember { mutableStateListOf<PlanRow>() }
    var loaded by remember { mutableStateOf(false) }
    var title by remember { mutableStateOf("") }
    var key by remember { mutableStateOf(true) }
    var check by remember { mutableStateOf(Check.FOCUS) }
    var minutes by remember { mutableStateOf("30") }
    var error by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(snap.goals) {
        if (!loaded && snap.status != null) {
            rows.addAll(snap.goals.map { PlanRow(it, locked = true) })
            loaded = true
        }
    }
    val hasKey = rows.any { it.goal.kind == GoalKind.KEY }

    Screen(
        eyebrow = todayText(),
        title = stringResource(if (adding) R.string.plan_title_adding else R.string.plan_title_morning),
        lead = stringResource(if (adding) R.string.plan_lead_adding else R.string.plan_lead_morning),
        onBack = if (adding) onCancel else null,
        bottom = {
            ErrorText(error)
            if (!hasKey) Footnote(stringResource(R.string.plan_need_key))
            PrimaryButton(stringResource(if (adding) R.string.save_changes else R.string.start_the_day), enabled = hasKey) {
                error = coreAction(ctx) { it.planDay(CoreHolder.clock(), rows.map { r -> r.goal }) }
                if (error == null) onDone()
            }
        },
    ) {
        if (!loaded) LoadingState()
        rows.forEachIndexed { i, r ->
            AppCard(padding = 16.dp) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(r.goal.title, Modifier.weight(1f), style = MaterialTheme.typography.titleMedium)
                    KindChip(r.goal.kind)
                }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        if (r.goal.targetMinutes > 0u) duration(r.goal.targetMinutes.toInt())
                        else pluralStringResource(R.plurals.times, r.goal.targetCount.toInt(), r.goal.targetCount.toInt()),
                        Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    if (r.goal.targetMinutes > 0u) {
                        TextButton(onClick = {
                            rows[i] = r.copy(goal = r.goal.copy(targetMinutes = (r.goal.targetMinutes.toInt() + 5).coerceAtMost(600).toUShort()))
                        }) { Text(stringResource(R.string.plus_5_min)) }
                    }
                    if (!r.locked) TextButton(onClick = { rows.removeAt(i) }) { Text(stringResource(R.string.remove)) }
                }
            }
        }

        Section(stringResource(R.string.plan_add_title)) {
            OutlinedTextField(title, { title = it.take(80) }, label = { Text(stringResource(R.string.name)) }, singleLine = true, modifier = Modifier.fillMaxWidth())
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                FilterChip(selected = key, onClick = { key = true }, label = { Text(stringResource(R.string.kind_key)) })
                FilterChip(selected = !key, onClick = { key = false }, label = { Text(stringResource(R.string.kind_habit)) })
                if (checkUsesMinutes(check)) {
                    OutlinedTextField(
                        minutes, { minutes = it.filter(Char::isDigit).take(3) },
                        label = { Text(stringResource(R.string.minutes)) }, singleLine = true,
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                        modifier = Modifier.weight(1f),
                    )
                }
            }
            CheckPicker(title, check, PLAN_CHECKS, suggest = true) { check = it }
            if (check == Check.HEALTH) Footnote(stringResource(R.string.health_minutes_note))
            SecondaryButton(stringResource(R.string.add), enabled = title.isNotBlank()) {
                val m = if (checkUsesMinutes(check)) (minutes.toIntOrNull() ?: 0).coerceIn(0, 600) else 0
                rows += PlanRow(
                    Goal(
                        id = slug(title, rows.map { it.goal.id }), title = title.trim(),
                        kind = if (key) GoalKind.KEY else GoalKind.HABIT,
                        targetMinutes = m.toUShort(), targetCount = 1u, methods = methodsFor(check, m), weekdays = ALL_DAYS,
                        apps = emptyList(), sites = emptyList(),
                    ),
                    locked = false,
                )
                title = ""
            }
            Footnote(stringResource(R.string.plan_note), PcoIcons.Camera)
        }
    }
}

// ───────────────────────── Focus (08) ─────────────────────────

/**
 * The next action, huge. Timed goal → FOCUS session (the core measures the minutes).
 * Untimed goal → declaration with friction: write what you did + 60 s (the text is not saved).
 */
@Composable
fun FocusScreen(goalId: String, snap: Snapshot, onBack: () -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var error by remember { mutableStateOf<String?>(null) }
    var note by remember { mutableStateOf("") }
    var left by remember { mutableIntStateOf(60) }
    val st = snap.status ?: return Screen { LoadingState() }
    val focusing = (st.state as? DayState.Focus)?.goalId == goalId
    val progress = st.goals.firstOrNull { it.goalId == goalId }
    val def = snap.goals.firstOrNull { it.id == goalId }
    val keys = st.goals.filter { it.kind == GoalKind.KEY }
    val step = keys.indexOfFirst { it.goalId == goalId }.coerceAtLeast(0) + 1
    val after = keys.firstOrNull { !it.completed && it.goalId != goalId }
    val habit = st.goals.firstOrNull { it.kind == GoalKind.HABIT && !it.completed && it.goalId != goalId }
    val target = progress?.targetMinutes?.toInt() ?: 0
    val done = minOf(progress?.focusMinutes?.toInt() ?: 0, target)
    val remaining = maxOf(0, target - done)

    // Camera: only in the verifier APK (no Internet). Only the result's 5 fields arrive here.
    val cameraMethod = def?.methods?.firstOrNull { it in CAMERA_METHODS }
    val verifier = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { r ->
        scope.launch {
            val message = withContext(Dispatchers.IO) { onVerifierResult(ctx, goalId, r.resultCode, r.data) }
            if (message == null) onBack() else error = message
        }
    }
    // Witness: only the guardian completes it, so there is no declaration here
    val declares = def == null || Method.HONOR_DECLARATION in def.methods

    // Place: scan the goal's QR here (the same in-memory scanner as pairing); the core checks the code
    var scanning by remember { mutableStateOf(false) }
    var cameraOk by remember {
        mutableStateOf(ContextCompat.checkSelfPermission(ctx, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED)
    }
    val askCamera = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { cameraOk = it; scanning = it }

    // Health: today's exercise minutes from Health Connect
    suspend fun checkHealth() {
        val target = def?.targetMinutes?.toInt() ?: 0
        val done: Int = withContext(Dispatchers.IO) { runCatching { HealthMinutes.since(ctx, st.dayStartMs) }.getOrNull() }
            ?: run { error = ctx.getString(R.string.health_unavailable); return }
        error = if (done < target) ctx.getString(R.string.health_progress, done, target)
        else coreAction(ctx) { it.completeGoal(CoreHolder.clock(), goalId, Method.HEALTH, 100u, done.coerceAtMost(1440).toUShort()) }
        if (error == null) onBack()
    }
    val askHealth = rememberLauncherForActivityResult(PermissionController.createRequestPermissionResultContract()) { granted ->
        if (granted.containsAll(HealthMinutes.PERMISSIONS)) scope.launch { checkHealth() } else error = ctx.getString(R.string.health_denied)
    }

    if (!focusing) LaunchedEffect(goalId) { while (left > 0) { delay(1_000); left-- } }
    // If the goal is already done or the session ended on another device, go back to the day
    LaunchedEffect(progress?.completed, st.state) {
        if (progress?.completed == true || (def?.methods?.contains(Method.FOCUS_SESSION) == true && !focusing && st.state !is DayState.Locked)) onBack()
    }

    Screen(
        onBack = onBack,
        backLabel = stringResource(R.string.see_whole_day),
        step = if (keys.isNotEmpty()) stringResource(R.string.step_of, step, keys.size) else null,
        bottom = {
            ErrorText(error)
            if (focusing) {
                PrimaryButton(stringResource(if (remaining == 0) R.string.finish_done else R.string.finish_for_now), big = true) {
                    error = coreAction(ctx) { it.endFocus(CoreHolder.clock()) }
                    if (error == null) onBack()
                }
                SecondaryButton(stringResource(R.string.pass_count, st.permitsLeft.toInt()), enabled = st.permitsLeft > 0u) {
                    error = coreAction(ctx) { it.startPass(CoreHolder.clock()) }
                }
                EmergencySection { error = coreAction(ctx) { it.emergency(CoreHolder.clock()) } }
            } else if (declares) {
                PrimaryButton(
                    if (left > 0) stringResource(R.string.wait_seconds, left) else stringResource(R.string.mark_done),
                    enabled = left == 0 && note.trim().length >= 10, big = true,
                ) {
                    error = coreAction(ctx) {
                        it.completeGoal(CoreHolder.clock(), goalId, Method.HONOR_DECLARATION, 100u, def?.targetMinutes ?: 0u)
                    }
                    if (error == null) onBack()
                }
            }
        },
    ) {
        Text(stringResource(R.string.now).uppercase(), style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.primary)
        Text(progress?.title ?: stringResource(R.string.your_goal), style = MaterialTheme.typography.displaySmall)
        if (focusing) {
            Text(stringResource(R.string.focus_instruction), style = MaterialTheme.typography.bodyLarge)
            AppCard {
                val remainingLabel = pluralStringResource(R.plurals.minutes_left, remaining, remaining)
                Text(
                    duration(remaining),
                    Modifier.semantics {
                        contentDescription = remainingLabel
                        if (remaining % 5 == 0 || remaining <= 1) liveRegion = LiveRegionMode.Polite
                    },
                    style = MaterialTheme.typography.displayLarge.copy(fontFeatureSettings = "tnum"),
                )
                ProgressBar(done, target, stringResource(R.string.progress), thick = true)
                Text(stringResource(R.string.progress_minutes, done, target), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        } else if (declares) {
            Text(stringResource(R.string.declare_instruction), style = MaterialTheme.typography.bodyLarge)
            OutlinedTextField(
                note, { note = it.take(300) },
                label = { Text(stringResource(R.string.what_you_did)) },
                supportingText = { Text(stringResource(R.string.declare_note)) },
                modifier = Modifier.fillMaxWidth(),
            )
        } else {
            IconText(PcoIcons.User, stringResource(R.string.witness_waiting))
        }
        if (!focusing && def != null && Method.QR_ANCHOR in def.methods) {
            if (scanning && cameraOk) {
                QrScanner(Modifier.fillMaxWidth().height(280.dp).clip(MaterialTheme.shapes.medium)) { code ->
                    scanning = false
                    scope.launch {
                        error = coreAction(ctx) { it.completeAtAnchor(CoreHolder.clock(), goalId, code) }
                        if (error == null) onBack()
                    }
                }
            } else {
                SecondaryButton(stringResource(R.string.scan_place)) {
                    if (cameraOk) scanning = true else askCamera.launch(Manifest.permission.CAMERA)
                }
            }
            Footnote(stringResource(R.string.join_camera_note), PcoIcons.Camera)
        }
        if (!focusing && def != null && Method.HEALTH in def.methods) {
            SecondaryButton(stringResource(R.string.health_check)) {
                when {
                    !HealthMinutes.available(ctx) -> error = ctx.getString(R.string.health_unavailable)
                    !HealthMinutes.granted(ctx) -> askHealth.launch(HealthMinutes.PERMISSIONS)
                    else -> checkHealth()
                }
            }
        }
        if (cameraMethod != null) {
            SecondaryButton(
                stringResource(
                    when (cameraMethod) {
                        Method.POSE_REPS -> R.string.camera_count_reps
                        Method.FOOD_PHOTO -> R.string.camera_food
                        Method.PAGE_OCR, Method.SUMMARY -> R.string.camera_pages
                        else -> R.string.camera_reading
                    },
                ),
            ) {
                scope.launch {
                    if (!ensureVerifier(ctx)) {
                        error = ctx.getString(R.string.verifier_install_failed)
                    } else {
                        verifierIntent(goalId, cameraMethod, def?.targetMinutes?.toInt() ?: 0, night = lookFor(snap.status, snap.nowMs) == Look.NOCTURNO)
                            ?.let { verifier.launch(it) }
                    }
                }
            }
            Footnote(stringResource(R.string.camera_note), PcoIcons.Camera)
        }
        after?.let { IconText(PcoIcons.ChevronRight, stringResource(R.string.next_goal, it.title)) }
        habit?.let { Footnote(stringResource(R.string.no_rush_habit, it.title), PcoIcons.Clock) }
    }
}
