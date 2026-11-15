package dev.procastinacioff.app.ui

import android.icu.text.MeasureFormat
import android.icu.util.Measure
import android.icu.util.MeasureUnit
import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Shapes
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.procastinacioff.app.R
import dev.procastinacioff.app.core.CoreHolder
import dev.procastinacioff.app.core.userMessage
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import uniffi.procastinacioff_core.DayState
import uniffi.procastinacioff_core.Goal
import uniffi.procastinacioff_core.GoalKind
import uniffi.procastinacioff_core.GoalProgress
import uniffi.procastinacioff_core.Method
import uniffi.procastinacioff_core.Status

// ── Visual system: 01 Calm by default · 03 Night 1 h before bedtime · 08 Focus ──

enum class Look { CALMA, NOCTURNO, FOCO }

private val Calma = lightColorScheme(
    primary = Color(0xFF0F6E64), onPrimary = Color.White,
    primaryContainer = Color(0xFFDCEFEA), onPrimaryContainer = Color(0xFF0F6E64),
    secondaryContainer = Color(0xFFDCEFEA), onSecondaryContainer = Color(0xFF0F6E64),
    background = Color(0xFFF6F7F5), onBackground = Color(0xFF1C2421),
    surface = Color.White, onSurface = Color(0xFF1C2421),
    surfaceVariant = Color(0xFFE3E9E6), onSurfaceVariant = Color(0xFF4D5A55),
    outline = Color(0xFFD5DCD8), error = Color(0xFF9A3412),
)
private val Nocturno = darkColorScheme(
    primary = Color(0xFFF5B544), onPrimary = Color(0xFF1A1300),
    primaryContainer = Color(0xFF2A3348), onPrimaryContainer = Color(0xFFECEFF7),
    secondaryContainer = Color(0xFF2A3348), onSecondaryContainer = Color(0xFFECEFF7),
    background = Color(0xFF0E1320), onBackground = Color(0xFFECEFF7),
    surface = Color(0xFF161D2E), onSurface = Color(0xFFECEFF7),
    surfaceVariant = Color(0xFF2A3348), onSurfaceVariant = Color(0xFFA9B3C7),
    outline = Color(0xFF2A3348), error = Color(0xFFFCA5A5),
)
private val Foco = lightColorScheme(
    primary = Color(0xFF5B3CC4), onPrimary = Color.White,
    primaryContainer = Color(0xFFEDE9FB), onPrimaryContainer = Color(0xFF5B3CC4),
    secondaryContainer = Color(0xFFEDE9FB), onSecondaryContainer = Color(0xFF5B3CC4),
    background = Color(0xFFF3F2F8), onBackground = Color(0xFF16161D),
    surface = Color.White, onSurface = Color(0xFF16161D),
    surfaceVariant = Color(0xFFEDE9FB), onSurfaceVariant = Color(0xFF4A4860),
    outline = Color(0xFFD9D6E8), error = Color(0xFF9A3412),
)

/** Night while sleeping, in the last hour before bedtime or with a dark system theme; Focus on the current goal. */
fun lookFor(status: Status?, focus: Boolean, systemDark: Boolean, nowMs: Long): Look = when {
    status?.state is DayState.Sleep || systemDark -> Look.NOCTURNO
    status?.sleepAtMs?.let { nowMs >= it - 3_600_000 } == true -> Look.NOCTURNO
    focus -> Look.FOCO
    else -> Look.CALMA
}

@Composable
fun PcoTheme(look: Look, content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = when (look) {
            Look.CALMA -> Calma
            Look.NOCTURNO -> Nocturno
            Look.FOCO -> Foco
        },
        shapes = Shapes(medium = RoundedCornerShape(20.dp), large = RoundedCornerShape(32.dp)),
    ) { Surface(color = MaterialTheme.colorScheme.background, content = content) }
}

// ── Core state, polled every second off the main thread ──

data class Snapshot(
    val status: Status?,
    val goals: List<Goal>,
    val nowMs: Long,
    val devices: List<uniffi.procastinacioff_core.DeviceSeen> = emptyList(),
)

@Composable
fun rememberSnapshot(): Snapshot {
    val ctx = LocalContext.current
    val snap by produceState(Snapshot(null, emptyList(), System.currentTimeMillis())) {
        while (true) {
            value = withContext(Dispatchers.IO) {
                runCatching {
                    val core = CoreHolder.get(ctx)
                    val clock = CoreHolder.clock()
                    Snapshot(core.status(clock), core.todayGoals(clock), clock.nowMs, core.devicesSeen())
                }.getOrElse { Snapshot(null, emptyList(), System.currentTimeMillis()) }
            }
            delay(1_000)
        }
    }
    return snap
}

/** Other devices that have not reported for over 24 h (they are off at night; hence not less). */
fun silentDevices(snap: Snapshot) =
    snap.devices.filter { !it.isMe && it.lastSeenMs > 0 && snap.nowMs - it.lastSeenMs > 24 * 3_600_000L }

fun hoursAgo(ctx: android.content.Context, ms: Long, now: Long): String {
    val h = ((now - ms) / 3_600_000).toInt()
    return when {
        h < 1 -> ctx.getString(R.string.time_ago_lt1h)
        h < 48 -> ctx.getString(R.string.time_ago_h, h)
        else -> ctx.resources.getQuantityString(R.plurals.time_ago_days, h / 24, h / 24)
    }
}

/** How a goal is checked. There is always a camera-free option (mark as done). */
enum class Check(@StringRes val label: Int, val usesMinutes: Boolean) {
    FOCUS(R.string.check_focus, true),
    READING(R.string.check_reading, true),
    PAGES(R.string.check_pages, false),
    EXERCISE(R.string.check_exercise, false),
    FOOD(R.string.check_food, false),
    MARK(R.string.check_mark, false),
}

fun methodsFor(check: Check, minutes: Int): List<Method> = when (check) {
    Check.FOCUS -> methodsFor(minutes)
    Check.READING -> listOf(Method.READING_PRESENCE, Method.FOCUS_SESSION, Method.HONOR_DECLARATION)
    Check.PAGES -> listOf(Method.PAGE_OCR, Method.SUMMARY, Method.HONOR_DECLARATION)
    Check.EXERCISE -> listOf(Method.POSE_REPS, Method.HONOR_DECLARATION)
    Check.FOOD -> listOf(Method.FOOD_PHOTO, Method.HONOR_DECLARATION)
    Check.MARK -> listOf(Method.HONOR_DECLARATION)
}

/** Methods checked by the verifier APK (camera/microphone, no Internet). */
val CAMERA_METHODS = setOf(Method.READING_PRESENCE, Method.PAGE_OCR, Method.SUMMARY, Method.POSE_REPS, Method.FOOD_PHOTO)

/** Runs a core action off the main thread; returns the translated error message or null. */
suspend fun coreAction(ctx: android.content.Context, block: (uniffi.procastinacioff_core.Core) -> Unit): String? =
    withContext(Dispatchers.IO) { runCatching { block(CoreHolder.get(ctx)) }.exceptionOrNull()?.userMessage(ctx) }

// ── Formats, in the user's language (no translation files needed) ──

fun hhmm(ms: Long): String = java.text.DateFormat.getTimeInstance(java.text.DateFormat.SHORT).format(Date(ms))
fun minutesUntil(ms: Long, now: Long): Int = maxOf(0, ((ms - now + 59_999) / 60_000).toInt())
fun duration(minutes: Int): String {
    val f = MeasureFormat.getInstance(Locale.getDefault(), MeasureFormat.FormatWidth.SHORT)
    val (h, m) = minutes / 60 to minutes % 60
    return when {
        h == 0 -> f.formatMeasures(Measure(m, MeasureUnit.MINUTE))
        m == 0 -> f.formatMeasures(Measure(h, MeasureUnit.HOUR))
        else -> f.formatMeasures(Measure(h, MeasureUnit.HOUR), Measure(m, MeasureUnit.MINUTE))
    }
}
fun todayText(): String {
    val pattern = android.text.format.DateFormat.getBestDateTimePattern(Locale.getDefault(), "EEEEdMMMM")
    return SimpleDateFormat(pattern, Locale.getDefault()).format(Date()).replaceFirstChar { it.uppercase() }
}

/** Narrow weekday names, Monday first ("M T W…" / "L M X…"), from the system. */
fun dayLabels(): List<String> =
    java.time.DayOfWeek.values().map { it.getDisplayName(java.time.format.TextStyle.NARROW, Locale.getDefault()) }

fun methodsFor(minutes: Int): List<Method> =
    if (minutes > 0) listOf(Method.FOCUS_SESSION, Method.HONOR_DECLARATION) else listOf(Method.HONOR_DECLARATION)

/** Stable identifier from the title: lowercase, no accents, hyphens. */
fun slug(title: String, taken: Collection<String>): String {
    val base = java.text.Normalizer.normalize(title.lowercase(), java.text.Normalizer.Form.NFD)
        .replace(Regex("\\p{M}+"), "").replace(Regex("[^a-z0-9]+"), "-").trim('-').take(24).ifEmpty { "goal" }
    var id = base
    var n = 2
    while (id in taken) id = "${base.take(20)}-${n++}"
    return id
}

data class Progress(val done: Int, val total: Int, val text: String)

fun progressOf(ctx: android.content.Context, g: GoalProgress): Progress {
    val minutes = g.targetMinutes.toInt()
    return if (minutes > 0) {
        val done = if (g.completed) minutes else minOf(g.focusMinutes.toInt(), minutes)
        Progress(done, minutes, ctx.getString(R.string.progress_minutes, done, minutes))
    } else {
        val (done, total) = g.doneCount.toInt() to g.targetCount.toInt()
        Progress(done, total, ctx.getString(R.string.progress_count, done, total))
    }
}

// ── Components ──

@Composable
fun KindChip(kind: GoalKind) {
    val key = kind == GoalKind.KEY
    Surface(
        shape = RoundedCornerShape(16.dp),
        color = if (key) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surfaceVariant,
    ) {
        Text(
            stringResource(if (key) R.string.kind_key else R.string.kind_habit),
            Modifier.padding(horizontal = 10.dp, vertical = 2.dp),
            style = MaterialTheme.typography.labelMedium,
            fontWeight = FontWeight.SemiBold,
            color = if (key) MaterialTheme.colorScheme.onPrimaryContainer else MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
fun ProgressBar(done: Int, total: Int, label: String, thick: Boolean = false) {
    LinearProgressIndicator(
        progress = { if (total > 0) done.toFloat() / total else 0f },
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = if (thick) 14.dp else 8.dp)
            .semantics {
                contentDescription = label
                progressBarRangeInfo = ProgressBarRangeInfo(done.toFloat(), 0f..maxOf(total, 1).toFloat())
            },
        trackColor = MaterialTheme.colorScheme.surfaceVariant,
    )
}

@Composable
fun GoalRow(g: GoalProgress, actionLabel: String?, onAction: () -> Unit) {
    val p = progressOf(LocalContext.current, g)
    Card(
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
        shape = MaterialTheme.shapes.medium,
    ) {
        Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(g.title, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f, fill = false))
                    KindChip(g.kind)
                }
                ProgressBar(p.done, p.total, g.title)
                Text(
                    if (g.completed) stringResource(R.string.goal_done) else p.text,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            if (actionLabel != null) {
                Spacer(Modifier.width(8.dp))
                TextButton(onClick = onAction, modifier = Modifier.heightIn(min = 48.dp)) { Text(actionLabel) }
            }
        }
    }
}

@Composable
fun PrimaryButton(text: String, enabled: Boolean = true, big: Boolean = false, onClick: () -> Unit) {
    Button(
        onClick = onClick,
        enabled = enabled,
        modifier = Modifier.fillMaxWidth().heightIn(min = if (big) 64.dp else 56.dp),
    ) { Text(text, fontWeight = FontWeight.SemiBold) }
}

@Composable
fun SecondaryButton(text: String, enabled: Boolean = true, onClick: () -> Unit) {
    OutlinedButton(onClick = onClick, enabled = enabled, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) {
        Text(text, fontWeight = FontWeight.SemiBold)
    }
}

/** "Break the glass": wait 60 s + write the reason. The reason is NOT saved or sent. */
@Composable
fun EmergencySection(onUse: () -> Unit) {
    var open by remember { mutableStateOf(false) }
    var reason by remember { mutableStateOf("") }
    if (!open) {
        TextButton(onClick = { open = true }, modifier = Modifier.heightIn(min = 48.dp)) {
            Text(stringResource(R.string.emergency_title), color = MaterialTheme.colorScheme.error, fontWeight = FontWeight.SemiBold)
        }
        return
    }
    var left by remember { mutableIntStateOf(60) }
    LaunchedEffect(Unit) { while (left > 0) { delay(1_000); left-- } }
    Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface)) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(stringResource(R.string.emergency_title), color = MaterialTheme.colorScheme.error, fontWeight = FontWeight.Bold)
            Text(stringResource(R.string.emergency_body))
            OutlinedTextField(
                value = reason,
                onValueChange = { reason = it.take(200) },
                label = { Text(stringResource(R.string.emergency_why)) },
                supportingText = { Text(stringResource(R.string.emergency_not_saved)) },
                modifier = Modifier.fillMaxWidth(),
            )
            SecondaryButton(
                if (left > 0) stringResource(R.string.wait_seconds, left) else stringResource(R.string.emergency_use),
                enabled = left == 0 && reason.trim().length >= 10,
                onClick = onUse,
            )
            TextButton(onClick = { open = false }) { Text(stringResource(R.string.cancel)) }
        }
    }
}

@Composable
fun ErrorText(message: String?) {
    if (message != null) Text(message, color = MaterialTheme.colorScheme.error, fontWeight = FontWeight.SemiBold)
}

@Composable
fun Badge(content: @Composable () -> Unit) {
    Surface(shape = RoundedCornerShape(50), color = MaterialTheme.colorScheme.primaryContainer, modifier = Modifier.size(44.dp)) {
        androidx.compose.foundation.layout.Box(contentAlignment = Alignment.Center) { content() }
    }
}
