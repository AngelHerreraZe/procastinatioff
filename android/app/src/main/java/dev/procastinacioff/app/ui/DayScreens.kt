package dev.procastinacioff.app.ui

import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import dev.procastinacioff.app.R
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import dev.procastinacioff.app.verify.onVerifierResult
import dev.procastinacioff.app.verify.verifierIntent
import dev.procastinacioff.app.verify.verifierIsGenuine
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.procastinacioff_core.DayState
import uniffi.procastinacioff_core.Goal
import uniffi.procastinacioff_core.GoalKind
import uniffi.procastinacioff_core.GoalProgress
import uniffi.procastinacioff_core.Method
import uniffi.procastinacioff_core.Status

private const val ALL_DAYS: UByte = 0x7Fu

@Composable
private fun actionLabel(g: GoalProgress, def: Goal?): String = stringResource(
    if (def?.methods?.contains(Method.FOCUS_SESSION) == true) (if (g.focusMinutes > 0u) R.string.action_continue else R.string.action_start)
    else R.string.mark_done,
)

// ───────────────────────── Today (01 Calm) ─────────────────────────

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HomeScreen(
    snap: Snapshot,
    onStart: (Goal) -> Unit,
    onFocus: (String) -> Unit,
    onPlan: () -> Unit,
    onPair: () -> Unit,
    onPartner: () -> Unit,
    minor: Boolean = false,
    onTutor: () -> Unit = {},
    onFamily: (() -> Unit)? = null,
) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var error by remember { mutableStateOf<String?>(null) }
    val st = snap.status ?: return
    val state = st.state
    val keys = st.goals.filter { it.kind == GoalKind.KEY }
    val pending = keys.filter { !it.completed }
    val next = pending.firstOrNull() ?: st.goals.firstOrNull { !it.completed }
    val nextDef = next?.let { n -> snap.goals.firstOrNull { it.id == n.goalId } }
    val canWork = state is DayState.Locked || state is DayState.Free

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Column { Text(stringResource(R.string.today)); Text(todayText(), style = MaterialTheme.typography.bodySmall) } },
                actions = {
                    if (!st.freeDay && !st.needsPlan) TextButton(onClick = onPlan) { Text(stringResource(R.string.add_goal)) }
                    if (minor) {
                        // A minor's device does not pair nor have a partner: the guardian decides that
                        TextButton(onClick = onTutor) { Text(stringResource(R.string.your_guardian)) }
                    } else {
                        TextButton(onClick = onPair) { Text(stringResource(R.string.pair_short)) }
                        TextButton(onClick = onPartner) { Text(stringResource(R.string.partner_short)) }
                        onFamily?.let { TextButton(onClick = it) { Text(stringResource(R.string.family_short)) } }
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.background),
            )
        },
        bottomBar = {
            // Thumb zone: one main action, the pass and the emergency
            Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
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
                        scope.launch { error = coreAction(ctx) { it.startPass(dev.procastinacioff.app.core.CoreHolder.clock()) } }
                    }
                    EmergencySection {
                        scope.launch { error = coreAction(ctx) { it.emergency(dev.procastinacioff.app.core.CoreHolder.clock()) } }
                    }
                }
            }
        },
    ) { padding ->
        LazyColumn(
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = padding.calculateTopPadding() + 8.dp, bottom = padding.calculateBottomPadding() + 16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            // Cross heartbeats: a device that stops reporting shows up on the others
            items(silentDevices(snap), key = { it.deviceId }) { d ->
                Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface)) {
                    Text(
                        stringResource(R.string.device_silent, d.label, hoursAgo(ctx, d.lastSeenMs, snap.nowMs)),
                        Modifier.padding(16.dp).semantics { liveRegion = LiveRegionMode.Polite },
                    )
                }
            }
            item { StatusCard(st, snap.nowMs, pending.size, next?.title) }
            if (st.goals.isNotEmpty()) {
                item { Text(stringResource(R.string.today_goals), style = MaterialTheme.typography.titleMedium, modifier = Modifier.semantics { heading() }) }
                items(st.goals, key = { it.goalId }) { g ->
                    val def = snap.goals.firstOrNull { it.id == g.goalId }
                    GoalRow(g, if (!g.completed && canWork && def != null) actionLabel(g, def) else null) { def?.let(onStart) }
                }
            }
        }
    }
}

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
    Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.primaryContainer)) {
        Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(title, style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold, color = MaterialTheme.colorScheme.onBackground)
            if (sentence.isNotEmpty()) Text(sentence, color = MaterialTheme.colorScheme.onSurfaceVariant)
            val sleepChip = st.sleepAtMs?.let { stringResource(R.string.sleep_at, hhmm(it)) }
            val passChip = if (st.freeDay) null else pluralStringResource(R.plurals.passes, st.permitsLeft.toInt(), st.permitsLeft.toInt())
            val chips = listOfNotNull(sleepChip, passChip)
            if (chips.isNotEmpty()) Text(chips.joinToString(" · "), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onPrimaryContainer)
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
    val scope = rememberCoroutineScope()
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

    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text(todayText(), color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(stringResource(if (adding) R.string.plan_title_adding else R.string.plan_title_morning), style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.Bold)
        Text(
            stringResource(if (adding) R.string.plan_lead_adding else R.string.plan_lead_morning),
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        rows.forEachIndexed { i, r ->
            Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface)) {
                Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text(r.goal.title, fontWeight = FontWeight.SemiBold)
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            KindChip(r.goal.kind)
                            Text(
                                if (r.goal.targetMinutes > 0u) duration(r.goal.targetMinutes.toInt())
                                else pluralStringResource(R.plurals.times, r.goal.targetCount.toInt(), r.goal.targetCount.toInt()),
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                    if (r.goal.targetMinutes > 0u) {
                        TextButton(onClick = {
                            rows[i] = r.copy(goal = r.goal.copy(targetMinutes = (r.goal.targetMinutes.toInt() + 5).coerceAtMost(600).toUShort()))
                        }) { Text(stringResource(R.string.plus_5_min)) }
                    }
                    if (!r.locked) TextButton(onClick = { rows.removeAt(i) }) { Text(stringResource(R.string.remove)) }
                }
            }
        }

        Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface)) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(stringResource(R.string.plan_add_title), style = MaterialTheme.typography.titleMedium)
                OutlinedTextField(title, { title = it.take(80) }, label = { Text(stringResource(R.string.name)) }, modifier = Modifier.fillMaxWidth())
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                    FilterChip(selected = key, onClick = { key = true }, label = { Text(stringResource(R.string.kind_key)) })
                    FilterChip(selected = !key, onClick = { key = false }, label = { Text(stringResource(R.string.kind_habit)) })
                    if (check.usesMinutes) {
                        OutlinedTextField(
                            minutes, { minutes = it.filter(Char::isDigit).take(3) },
                            label = { Text(stringResource(R.string.minutes)) },
                            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                            modifier = Modifier.width(120.dp),
                        )
                    }
                }
                Text(stringResource(R.string.how_checked), style = MaterialTheme.typography.labelLarge)
                Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    Check.entries.forEach { c -> FilterChip(selected = check == c, onClick = { check = c }, label = { Text(stringResource(c.label)) }) }
                }
                SecondaryButton(stringResource(R.string.add), enabled = title.isNotBlank()) {
                    val m = if (check.usesMinutes) (minutes.toIntOrNull() ?: 0).coerceIn(0, 600) else 0
                    rows += PlanRow(
                        Goal(
                            id = slug(title, rows.map { it.goal.id }), title = title.trim(),
                            kind = if (key) GoalKind.KEY else GoalKind.HABIT,
                            targetMinutes = m.toUShort(), targetCount = 1u, methods = methodsFor(check, m), weekdays = ALL_DAYS,
                        ),
                        locked = false,
                    )
                    title = ""
                }
                Text(
                    stringResource(R.string.plan_note),
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        }

        ErrorText(error)
        val hasKey = rows.any { it.goal.kind == GoalKind.KEY }
        if (!hasKey) Text(stringResource(R.string.plan_need_key), color = MaterialTheme.colorScheme.onSurfaceVariant)
        PrimaryButton(stringResource(if (adding) R.string.save_changes else R.string.start_the_day), enabled = hasKey) {
            scope.launch {
                error = coreAction(ctx) { it.planDay(dev.procastinacioff.app.core.CoreHolder.clock(), rows.map { r -> r.goal }) }
                if (error == null) onDone()
            }
        }
        if (adding) TextButton(onClick = onCancel) { Text(stringResource(R.string.cancel)) }
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
    val st = snap.status ?: return
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

    if (!focusing) LaunchedEffect(goalId) { while (left > 0) { delay(1_000); left-- } }
    // If the goal is already done or the session ended on another device, go back to the day
    LaunchedEffect(progress?.completed, st.state) {
        if (progress?.completed == true || (def?.methods?.contains(Method.FOCUS_SESSION) == true && !focusing && st.state !is DayState.Locked)) onBack()
    }

    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
            TextButton(onClick = onBack) { Text(stringResource(R.string.see_whole_day)) }
            if (keys.isNotEmpty()) Text(stringResource(R.string.step_of, step, keys.size), color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Card(
            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
            shape = MaterialTheme.shapes.large,
        ) {
            Column(Modifier.padding(28.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                Text(stringResource(R.string.now), color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.SemiBold)
                Text(progress?.title ?: stringResource(R.string.your_goal), fontSize = 34.sp, lineHeight = 40.sp, fontWeight = FontWeight.Bold)
                if (focusing) {
                    Text(stringResource(R.string.focus_instruction), fontSize = 18.sp)
                    val remainingLabel = pluralStringResource(R.plurals.minutes_left, remaining, remaining)
                    Text(
                        duration(remaining),
                        fontSize = 72.sp,
                        fontWeight = FontWeight.SemiBold,
                        modifier = Modifier.semantics {
                            contentDescription = remainingLabel
                            if (remaining % 5 == 0 || remaining <= 1) liveRegion = LiveRegionMode.Polite
                        },
                    )
                    ProgressBar(done, target, stringResource(R.string.progress), thick = true)
                    Text(stringResource(R.string.progress_minutes, done, target), color = MaterialTheme.colorScheme.onSurfaceVariant)
                    PrimaryButton(stringResource(if (remaining == 0) R.string.finish_done else R.string.finish_for_now), big = true) {
                        scope.launch {
                            error = coreAction(ctx) { it.endFocus(dev.procastinacioff.app.core.CoreHolder.clock()) }
                            if (error == null) onBack()
                        }
                    }
                } else {
                    Text(stringResource(R.string.declare_instruction), fontSize = 18.sp)
                    OutlinedTextField(
                        note, { note = it.take(300) },
                        label = { Text(stringResource(R.string.what_you_did)) },
                        supportingText = { Text(stringResource(R.string.declare_note)) },
                        modifier = Modifier.fillMaxWidth(),
                    )
                    PrimaryButton(
                        if (left > 0) stringResource(R.string.wait_seconds, left) else stringResource(R.string.mark_done),
                        enabled = left == 0 && note.trim().length >= 10, big = true,
                    ) {
                        scope.launch {
                            error = coreAction(ctx) {
                                it.completeGoal(
                                    dev.procastinacioff.app.core.CoreHolder.clock(), goalId, Method.HONOR_DECLARATION,
                                    100u, def?.targetMinutes ?: 0u,
                                )
                            }
                            if (error == null) onBack()
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
                        if (!verifierIsGenuine(ctx)) {
                            error = ctx.getString(R.string.install_verifier)
                        } else {
                            verifierIntent(goalId, cameraMethod, def?.targetMinutes?.toInt() ?: 0)?.let { verifier.launch(it) }
                        }
                    }
                    Text(
                        stringResource(R.string.camera_note),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                ErrorText(error)
            }
        }
        after?.let { Text(stringResource(R.string.next_goal, it.title), fontWeight = FontWeight.SemiBold) }
        habit?.let { Text(stringResource(R.string.no_rush_habit, it.title), color = MaterialTheme.colorScheme.onSurfaceVariant) }
        if (focusing) {
            FilledTonalButton(
                onClick = { scope.launch { error = coreAction(ctx) { it.startPass(dev.procastinacioff.app.core.CoreHolder.clock()) } } },
                enabled = st.permitsLeft > 0u,
            ) { Text(stringResource(R.string.pass_count, st.permitsLeft.toInt())) }
            EmergencySection {
                scope.launch { error = coreAction(ctx) { it.emergency(dev.procastinacioff.app.core.CoreHolder.clock()) } }
            }
        }
    }
}
