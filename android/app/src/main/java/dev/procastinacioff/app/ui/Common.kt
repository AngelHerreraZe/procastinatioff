package dev.procastinacioff.app.ui

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
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
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

// ── Sistema visual (ADR 0004): 01 Calma de base · 03 Nocturno 1 h antes de dormir · 08 Foco ──

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

/** Nocturno al dormir, en la última hora antes de dormir o con el sistema en oscuro; Foco en la meta en curso. */
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

// ── Estado del núcleo, consultado cada segundo fuera del hilo principal ──

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

/** Otros dispositivos que no reportan en más de 24 h (de noche se apagan; por eso no menos). */
fun silentDevices(snap: Snapshot) =
    snap.devices.filter { !it.isMe && it.lastSeenMs > 0 && snap.nowMs - it.lastSeenMs > 24 * 3_600_000L }

fun hoursAgo(ms: Long, now: Long): String {
    val h = ((now - ms) / 3_600_000).toInt()
    return when {
        h < 1 -> "hace menos de 1 h"
        h < 48 -> "hace $h h"
        else -> "hace ${h / 24} días"
    }
}

/** Cómo se comprueba una meta. Siempre queda la opción sin cámara (marcar como hecha). */
enum class Check(val label: String, val usesMinutes: Boolean) {
    FOCUS("Enfoque", true),
    READING("Lectura (cámara)", true),
    PAGES("Páginas + resumen", false),
    EXERCISE("Ejercicio (cámara)", false),
    FOOD("Comida sana (foto)", false),
    MARK("Marcar como hecha", false),
}

fun methodsFor(check: Check, minutes: Int): List<Method> = when (check) {
    Check.FOCUS -> methodsFor(minutes)
    Check.READING -> listOf(Method.READING_PRESENCE, Method.FOCUS_SESSION, Method.HONOR_DECLARATION)
    Check.PAGES -> listOf(Method.PAGE_OCR, Method.SUMMARY, Method.HONOR_DECLARATION)
    Check.EXERCISE -> listOf(Method.POSE_REPS, Method.HONOR_DECLARATION)
    Check.FOOD -> listOf(Method.FOOD_PHOTO, Method.HONOR_DECLARATION)
    Check.MARK -> listOf(Method.HONOR_DECLARATION)
}

/** Métodos que comprueba el APK verificador (cámara/micrófono, sin Internet). */
val CAMERA_METHODS = setOf(Method.READING_PRESENCE, Method.PAGE_OCR, Method.SUMMARY, Method.POSE_REPS, Method.FOOD_PHOTO)

/** Ejecuta una acción del núcleo fuera del hilo principal; devuelve el mensaje de error o null. */
suspend fun coreAction(ctx: android.content.Context, block: (uniffi.procastinacioff_core.Core) -> Unit): String? =
    withContext(Dispatchers.IO) { runCatching { block(CoreHolder.get(ctx)) }.exceptionOrNull()?.userMessage() }

// ── Formatos ──

fun hhmm(ms: Long): String = SimpleDateFormat("HH:mm", Locale("es")).format(Date(ms))
fun minutesUntil(ms: Long, now: Long): Int = maxOf(0, ((ms - now + 59_999) / 60_000).toInt())
fun duration(minutes: Int): String = when {
    minutes < 60 -> "$minutes min"
    minutes % 60 == 0 -> "${minutes / 60} h"
    else -> "${minutes / 60} h ${"%02d".format(minutes % 60)} min"
}
fun todayText(): String =
    SimpleDateFormat("EEEE, d 'de' MMMM", Locale("es")).format(Date()).replaceFirstChar { it.uppercase() }

fun methodsFor(minutes: Int): List<Method> =
    if (minutes > 0) listOf(Method.FOCUS_SESSION, Method.HONOR_DECLARATION) else listOf(Method.HONOR_DECLARATION)

/** Identificador estable a partir del título: minúsculas, sin tildes, con guiones. */
fun slug(title: String, taken: Collection<String>): String {
    val base = java.text.Normalizer.normalize(title.lowercase(), java.text.Normalizer.Form.NFD)
        .replace(Regex("\\p{M}+"), "").replace(Regex("[^a-z0-9]+"), "-").trim('-').take(24).ifEmpty { "meta" }
    var id = base
    var n = 2
    while (id in taken) id = "${base.take(20)}-${n++}"
    return id
}

data class Progress(val done: Int, val total: Int, val text: String)

fun progressOf(g: GoalProgress): Progress {
    val minutes = g.targetMinutes.toInt()
    return if (minutes > 0) {
        val done = if (g.completed) minutes else minOf(g.focusMinutes.toInt(), minutes)
        Progress(done, minutes, "$done de $minutes min")
    } else {
        Progress(g.doneCount.toInt(), g.targetCount.toInt(), "${g.doneCount} de ${g.targetCount}")
    }
}

// ── Componentes ──

@Composable
fun KindChip(kind: GoalKind) {
    val key = kind == GoalKind.KEY
    Surface(
        shape = RoundedCornerShape(16.dp),
        color = if (key) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surfaceVariant,
    ) {
        Text(
            if (key) "Desbloquea" else "Hábito",
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
    val p = progressOf(g)
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
                    if (g.completed) "Hecha" else p.text,
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

/** "Romper el cristal": esperar 60 s + escribir el motivo. El motivo NO se guarda ni se envía. */
@Composable
fun EmergencySection(onUse: () -> Unit) {
    var open by remember { mutableStateOf(false) }
    var reason by remember { mutableStateOf("") }
    if (!open) {
        TextButton(onClick = { open = true }, modifier = Modifier.heightIn(min = 48.dp)) {
            Text("Emergencia", color = MaterialTheme.colorScheme.error, fontWeight = FontWeight.SemiBold)
        }
        return
    }
    var left by remember { mutableIntStateOf(60) }
    LaunchedEffect(Unit) { while (left > 0) { delay(1_000); left-- } }
    Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface)) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text("Emergencia", color = MaterialTheme.colorScheme.error, fontWeight = FontWeight.Bold)
            Text("Te da 15 minutos libres. Cuesta un permiso mañana y queda registrado.")
            OutlinedTextField(
                value = reason,
                onValueChange = { reason = it.take(200) },
                label = { Text("¿Por qué lo necesitas?") },
                supportingText = { Text("El motivo no se guarda ni se envía a nadie.") },
                modifier = Modifier.fillMaxWidth(),
            )
            SecondaryButton(
                if (left > 0) "Espera $left s" else "Usar emergencia (15 min)",
                enabled = left == 0 && reason.trim().length >= 10,
                onClick = onUse,
            )
            TextButton(onClick = { open = false }) { Text("Cancelar") }
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
