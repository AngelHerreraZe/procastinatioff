package dev.procastinacioff.app.ui

import android.app.Activity
import android.icu.text.MeasureFormat
import android.icu.util.Measure
import android.icu.util.MeasureUnit
import androidx.annotation.StringRes
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.PlainTooltip
import androidx.compose.material3.Shapes
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TooltipBox
import androidx.compose.material3.TooltipDefaults
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.material3.rememberTooltipState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.view.WindowCompat
import dev.procastinacioff.app.R
import dev.procastinacioff.app.core.CoreHolder
import dev.procastinacioff.app.core.userMessage
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
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

// ── Visual system (DESIGN.md): Calma by day · Nocturno from 1 h before bedtime until waking ──

enum class Look { CALMA, NOCTURNO }

// Calma: the tokens of DESIGN.md §2. Material roles: accent-soft = primaryContainer, track = surfaceVariant,
// border = outlineVariant (decoration), outline = input borders (they need 3:1 to be seen as controls).
private val Calma = lightColorScheme(
    primary = Color(0xFF087C70), onPrimary = Color.White,
    // Text on accent-soft uses accent-pressed: accent itself is only 4.4:1 there (fine for icons, not for text)
    primaryContainer = Color(0xFFDDF3ED), onPrimaryContainer = Color(0xFF0E625B),
    secondary = Color(0xFF087C70), onSecondary = Color.White,
    secondaryContainer = Color(0xFFDDF3ED), onSecondaryContainer = Color(0xFF0E625B),
    tertiary = Color(0xFF087C70), onTertiary = Color.White,
    background = Color(0xFFF5F7F5), onBackground = Color(0xFF172321),
    surface = Color.White, onSurface = Color(0xFF172321),
    surfaceVariant = Color(0xFFE6EBE8), onSurfaceVariant = Color(0xFF64736F),
    surfaceContainerLowest = Color.White, surfaceContainerLow = Color(0xFFF5F7F5), surfaceContainer = Color(0xFFF5F7F5),
    surfaceContainerHigh = Color(0xFFEEF1EF), surfaceContainerHighest = Color(0xFFE6EBE8),
    outline = Color(0xFF64736F), outlineVariant = Color(0xFFE6EBE8),
    // danger-text: the error token #D9534F is 3.9:1 on white, below AA for text; this is the same hue darkened.
    error = Color(0xFFB3403C), onError = Color.White,
)
private val Nocturno = darkColorScheme(
    primary = Color(0xFFF5B544), onPrimary = Color(0xFF1A1300),
    primaryContainer = Color(0xFF2A3348), onPrimaryContainer = Color(0xFFECEFF7),
    secondary = Color(0xFFF5B544), onSecondary = Color(0xFF1A1300),
    secondaryContainer = Color(0xFF2A3348), onSecondaryContainer = Color(0xFFECEFF7),
    tertiary = Color(0xFFF5B544), onTertiary = Color(0xFF1A1300),
    background = Color(0xFF0E1320), onBackground = Color(0xFFECEFF7),
    surface = Color(0xFF161D2E), onSurface = Color(0xFFECEFF7),
    surfaceVariant = Color(0xFF2A3348), onSurfaceVariant = Color(0xFFA9B3C7),
    surfaceContainerLowest = Color(0xFF0B0F1A), surfaceContainerLow = Color(0xFF0E1320), surfaceContainer = Color(0xFF161D2E),
    surfaceContainerHigh = Color(0xFF2A3348), surfaceContainerHighest = Color(0xFF2A3348),
    outline = Color(0xFFA9B3C7), outlineVariant = Color(0xFF2A3348),
    error = Color(0xFFFCA5A5), onError = Color(0xFF1A1300),
)

/** Status tokens, the same in both looks: badge fills with their own text color (never color alone: badges are words). */
object StatusColors {
    val Success = Color(0xFF198754); val OnSuccess = Color.White        // 4.5:1
    val Warning = Color(0xFFE9A23B); val OnWarning = Color(0xFF172321)  // 7.5:1
    val Info = Color(0xFF4386C6)                                        // icons only: 3.8:1 on white
}

/** Nocturno while sleeping and in the last hour before bedtime; Calma the rest of the day. Same rule on every platform. */
fun lookFor(status: Status?, nowMs: Long): Look = when {
    status?.state is DayState.Sleep -> Look.NOCTURNO
    status?.sleepAtMs?.let { nowMs >= it - 3_600_000 } == true -> Look.NOCTURNO
    else -> Look.CALMA
}

// Inter (OFL 1.1, assets/licenses/Inter-OFL.txt), one variable file in res/font: never loaded from the network.
@OptIn(ExperimentalTextApi::class)
private val Inter = FontFamily(
    listOf(FontWeight.Normal, FontWeight.Medium, FontWeight.SemiBold, FontWeight.Bold, FontWeight.ExtraBold).map {
        Font(R.font.inter, it, variationSettings = FontVariation.Settings(FontVariation.weight(it.weight)))
    },
)

private fun style(size: Int, line: Int, weight: FontWeight, tracking: Double = 0.0) =
    TextStyle(fontFamily = Inter, fontSize = size.sp, lineHeight = line.sp, fontWeight = weight, letterSpacing = tracking.sp)

// Display 32 · H1 28 · H2 24 · H3 20 · Body 16 · Small 14 · Caption 12 · Buttons 16. Nothing smaller than Caption.
private val Calm = Typography(
    displayLarge = style(64, 72, FontWeight.SemiBold, -1.0),  // focus timer
    displayMedium = style(40, 48, FontWeight.Bold, -0.5),     // codes to compare
    displaySmall = style(32, 40, FontWeight.Bold, -0.6),      // Display
    headlineLarge = style(32, 40, FontWeight.Bold, -0.6),
    headlineMedium = style(28, 36, FontWeight.Bold, -0.4),    // H1: screen titles
    headlineSmall = style(24, 32, FontWeight.SemiBold, -0.2), // H2
    titleLarge = style(20, 28, FontWeight.SemiBold),          // H3: day status, section headers
    titleMedium = style(16, 24, FontWeight.SemiBold),         // card titles, goal names
    titleSmall = style(14, 20, FontWeight.Medium, 0.6),       // eyebrows and steps (shown in capitals)
    bodyLarge = style(16, 24, FontWeight.Normal),             // Body
    bodyMedium = style(14, 20, FontWeight.Normal),            // Small
    bodySmall = style(14, 20, FontWeight.Normal),
    labelLarge = style(16, 24, FontWeight.SemiBold),          // Buttons
    labelMedium = style(12, 16, FontWeight.Medium, 0.2),      // Caption
    labelSmall = style(12, 16, FontWeight.Medium, 0.2),
)

/** Light text on a dark ground reads thinner: Nocturno goes one weight up (body 500, titles 800). */
private val Night = Calm.copy(
    bodyLarge = Calm.bodyLarge.copy(fontWeight = FontWeight.Medium),
    bodyMedium = Calm.bodyMedium.copy(fontWeight = FontWeight.Medium),
    bodySmall = Calm.bodySmall.copy(fontWeight = FontWeight.Medium),
    titleLarge = Calm.titleLarge.copy(fontWeight = FontWeight.ExtraBold),
    headlineSmall = Calm.headlineSmall.copy(fontWeight = FontWeight.ExtraBold),
    headlineMedium = Calm.headlineMedium.copy(fontWeight = FontWeight.ExtraBold),
    displaySmall = Calm.displaySmall.copy(fontWeight = FontWeight.ExtraBold),
)

@Composable
fun PcoTheme(look: Look, content: @Composable () -> Unit) {
    val night = look == Look.NOCTURNO
    val view = LocalView.current
    if (!view.isInEditMode) SideEffect { // system bar icons follow the look, not the system dark mode
        (view.context as? Activity)?.window?.let { w ->
            WindowCompat.getInsetsController(w, view).run {
                isAppearanceLightStatusBars = !night
                isAppearanceLightNavigationBars = !night
            }
        }
    }
    MaterialTheme(
        colorScheme = if (night) Nocturno else Calma,
        typography = if (night) Night else Calm,
        // Radii: 12 inputs · pill buttons, chips and badges · cards 20 dp in Calma, 24 dp in Nocturno (wider softens the dark)
        shapes = Shapes(
            extraSmall = RoundedCornerShape(12.dp),
            small = RoundedCornerShape(50),
            medium = RoundedCornerShape(if (night) 24.dp else 20.dp),
            large = RoundedCornerShape(28.dp),
            extraLarge = RoundedCornerShape(28.dp),
        ),
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
                    Snapshot(core.status(clock), core.todayGoals(clock), clock.nowMs, core.devicesSeen(clock.nowMs))
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

// ── Layout ──

/** Where button actions run: the screen's scope, so an action finishes even if its button goes away mid-way. */
private val LocalActionScope = staticCompositionLocalOf<CoroutineScope?> { null }

/**
 * Every screen: safe areas, an optional top row (back, step, actions), a scrolling body at most 640 dp wide
 * (tablets, landscape) and the main actions pinned at the bottom, in thumb reach. The pinned part scrolls by
 * itself if it ever grows past 60 % of the height (large text, open emergency), so nothing is cut off.
 */
@Composable
fun Screen(
    title: String? = null,
    lead: String? = null,
    eyebrow: String? = null,
    onBack: (() -> Unit)? = null,
    backLabel: String? = null,
    step: String? = null,
    actions: (@Composable RowScope.() -> Unit)? = null,
    bottom: (@Composable ColumnScope.() -> Unit)? = null,
    content: @Composable ColumnScope.() -> Unit,
) = CompositionLocalProvider(LocalActionScope provides rememberCoroutineScope()) {
    BoxWithConstraints(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing)) {
        val maxBottom = maxHeight * 0.6f
        Column(Modifier.fillMaxSize(), horizontalAlignment = Alignment.CenterHorizontally) {
            if (onBack != null || step != null || actions != null) {
                Row(
                    Modifier.widthIn(max = 656.dp).fillMaxWidth().heightIn(min = 56.dp).padding(horizontal = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    onBack?.let {
                        IconButton(onClick = it) { Icon(PcoIcons.Back, backLabel ?: stringResource(R.string.back)) }
                    }
                    Spacer(Modifier.weight(1f))
                    step?.let {
                        Text(
                            it.uppercase(), Modifier.padding(horizontal = 16.dp),
                            style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    actions?.invoke(this)
                }
            }
            Column(Modifier.weight(1f).fillMaxWidth().verticalScroll(rememberScrollState()), horizontalAlignment = Alignment.CenterHorizontally) {
                Column(
                    Modifier.widthIn(max = 640.dp).fillMaxWidth().padding(start = 24.dp, end = 24.dp, top = 8.dp, bottom = 24.dp),
                    verticalArrangement = Arrangement.spacedBy(16.dp),
                ) {
                    if (eyebrow != null || title != null || lead != null) {
                        Column(Modifier.padding(bottom = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            eyebrow?.let { Text(it.uppercase(), style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                            title?.let { Text(it, Modifier.semantics { heading() }, style = MaterialTheme.typography.headlineMedium) }
                            lead?.let { Text(it, style = MaterialTheme.typography.bodyLarge) }
                        }
                    }
                    content()
                }
            }
            bottom?.let {
                Column(
                    Modifier.widthIn(max = 640.dp).fillMaxWidth().heightIn(max = maxBottom).verticalScroll(rememberScrollState())
                        .padding(start = 24.dp, end = 24.dp, top = 12.dp, bottom = 16.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    content = it,
                )
            }
        }
    }
}

/** Top-row action: a 48 dp icon button whose label is read by screen readers and shown on long press. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun IconAction(icon: ImageVector, label: String, onClick: () -> Unit) {
    TooltipBox(TooltipDefaults.rememberPlainTooltipPositionProvider(), { PlainTooltip { Text(label) } }, rememberTooltipState()) {
        IconButton(onClick = onClick) { Icon(icon, label) }
    }
}

/** A surface card: 1 dp border, no shadow. Clickable when `onClick` is given. */
@Composable
fun AppCard(
    modifier: Modifier = Modifier,
    onClick: (() -> Unit)? = null,
    padding: Dp = 20.dp,
    content: @Composable ColumnScope.() -> Unit,
) {
    val shape = MaterialTheme.shapes.medium
    val color = MaterialTheme.colorScheme.surface
    val border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant)
    val inner: @Composable () -> Unit = { Column(Modifier.padding(padding), verticalArrangement = Arrangement.spacedBy(12.dp), content = content) }
    if (onClick == null) Surface(modifier.fillMaxWidth(), shape = shape, color = color, border = border, content = inner)
    else Surface(onClick = onClick, modifier = modifier.fillMaxWidth(), shape = shape, color = color, border = border, content = inner)
}

/** A card with a title: the settings-style group used across the app. */
@Composable
fun Section(title: String, content: @Composable ColumnScope.() -> Unit) = AppCard {
    Text(title, Modifier.semantics { heading() }, style = MaterialTheme.typography.titleMedium)
    content()
}

@Composable
fun SectionHeader(text: String) =
    Text(text, Modifier.padding(top = 8.dp).semantics { heading() }, style = MaterialTheme.typography.titleLarge)

/** Decorative icon on an accent-soft circle (the leading element of features, permissions and states). */
@Composable
fun IconCircle(icon: ImageVector, size: Dp = 48.dp) {
    Box(
        Modifier.size(size).clip(CircleShape).background(MaterialTheme.colorScheme.primaryContainer),
        contentAlignment = Alignment.Center,
    ) { Icon(icon, null, Modifier.size(size / 2), tint = MaterialTheme.colorScheme.primary) }
}

/** Icon + text on one line. `muted` = secondary text (footnotes, privacy lines). */
@Composable
fun IconText(icon: ImageVector, text: String, modifier: Modifier = Modifier, muted: Boolean = false) {
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Icon(
            icon, null, Modifier.padding(top = if (muted) 0.dp else 2.dp).size(20.dp),
            tint = if (muted) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.primary,
        )
        Text(
            text,
            style = if (muted) MaterialTheme.typography.bodyMedium else MaterialTheme.typography.bodyLarge,
            color = if (muted) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.onSurface,
        )
    }
}

/** "Privacy at the moment of use" and similar notes. */
@Composable
fun Footnote(text: String, icon: ImageVector = PcoIcons.Info) = IconText(icon, text, muted = true)

/** A full-width text action with a chevron ("Grant permission  >"), 48 dp tall. */
@Composable
fun LinkRow(text: String, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 48.dp).clip(MaterialTheme.shapes.extraSmall).clickable(role = Role.Button, onClick = onClick),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(text, Modifier.weight(1f), style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.primary)
        Icon(PcoIcons.ChevronRight, null, tint = MaterialTheme.colorScheme.primary)
    }
}

enum class Tone { SUCCESS, WARNING, ACCENT, NEUTRAL }

/** A word in a pill: "Granted", "Pending", "Unlocks", "Done". Status is always written, never color alone. */
@Composable
fun StatusBadge(text: String, tone: Tone) {
    val cs = MaterialTheme.colorScheme
    val (bg, fg) = when (tone) {
        Tone.SUCCESS -> StatusColors.Success to StatusColors.OnSuccess
        Tone.WARNING -> StatusColors.Warning to StatusColors.OnWarning
        Tone.ACCENT -> cs.primaryContainer to cs.onPrimaryContainer
        Tone.NEUTRAL -> cs.surfaceContainerHigh to cs.onSurface
    }
    Surface(shape = MaterialTheme.shapes.small, color = bg, contentColor = fg) {
        Text(text, Modifier.padding(horizontal = 10.dp, vertical = 4.dp), style = MaterialTheme.typography.labelMedium, fontWeight = FontWeight.SemiBold, maxLines = 1)
    }
}

/** A fact with its icon ("Sleep at 22:30", "2 passes"): not interactive. */
@Composable
fun InfoChip(icon: ImageVector, text: String) {
    Surface(
        shape = MaterialTheme.shapes.small, color = MaterialTheme.colorScheme.background,
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
    ) {
        Row(Modifier.heightIn(min = 32.dp).padding(horizontal = 12.dp), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(icon, null, Modifier.size(16.dp), tint = MaterialTheme.colorScheme.primary)
            Text(text, style = MaterialTheme.typography.bodyMedium)
        }
    }
}

@Composable
fun KindChip(kind: GoalKind) {
    val key = kind == GoalKind.KEY
    StatusBadge(stringResource(if (key) R.string.kind_key else R.string.kind_habit), if (key) Tone.ACCENT else Tone.NEUTRAL)
}

@Composable
fun ProgressBar(done: Int, total: Int, label: String, thick: Boolean = false) {
    val target = if (total > 0) done.toFloat() / total else 0f
    val shown by animateFloatAsState(target, tween(200), label = "progress")
    LinearProgressIndicator(
        progress = { shown },
        modifier = Modifier
            .fillMaxWidth()
            .height(if (thick) 12.dp else 8.dp)
            .clip(CircleShape)
            .semantics {
                contentDescription = label
                progressBarRangeInfo = ProgressBarRangeInfo(done.toFloat(), 0f..maxOf(total, 1).toFloat())
            },
        trackColor = MaterialTheme.colorScheme.surfaceVariant,
        strokeCap = StrokeCap.Round,
        gapSize = 0.dp,
        drawStopIndicator = {},
    )
}

@Composable
fun GoalRow(g: GoalProgress, actionLabel: String?, onAction: () -> Unit) {
    val p = progressOf(LocalContext.current, g)
    AppCard(padding = 16.dp) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(g.title, Modifier.weight(1f), style = MaterialTheme.typography.titleMedium)
            if (g.completed) StatusBadge(stringResource(R.string.goal_done), Tone.SUCCESS) else KindChip(g.kind)
        }
        ProgressBar(p.done, p.total, g.title)
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(p.text, Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            if (actionLabel != null) TextButton(onClick = onAction) { Text(actionLabel) }
        }
    }
}

@Composable
fun EmptyState(icon: ImageVector, text: String) {
    Column(Modifier.fillMaxWidth().padding(vertical = 32.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(16.dp)) {
        IconCircle(icon, 56.dp)
        Text(text, style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.onSurfaceVariant, textAlign = TextAlign.Center)
    }
}

@Composable
fun LoadingState() {
    val label = stringResource(R.string.loading)
    Box(Modifier.fillMaxWidth().padding(vertical = 48.dp), contentAlignment = Alignment.Center) {
        CircularProgressIndicator(Modifier.semantics { contentDescription = label })
    }
}

/** The brand: the logo (branding/logo.png, never redrawn) + "ProcastinaciOFF" with OFF in the accent. */
@Composable
fun Wordmark() {
    val accent = MaterialTheme.colorScheme.primary
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
        Image(painterResource(R.drawable.logo), null, Modifier.size(48.dp))
        Text(
            buildAnnotatedString { append("Procastinaci"); withStyle(SpanStyle(color = accent)) { append("OFF") } },
            style = MaterialTheme.typography.titleLarge,
        )
    }
}

// ── Buttons ──

/**
 * One action at a time per button (no duplicate submissions). The spinner appears only if the action
 * takes over 300 ms, so instant actions (navigation) do not flicker.
 */
private class ButtonAction(private val scope: CoroutineScope) {
    var busy by mutableStateOf(false)
    var spinning by mutableStateOf(false)
    fun run(action: suspend () -> Unit) {
        if (busy) return
        busy = true
        scope.launch(start = CoroutineStart.UNDISPATCHED) {
            val spin = launch { delay(300); spinning = true }
            try { action() } finally { spin.cancel(); spinning = false; busy = false }
        }
    }
}

@Composable
private fun rememberButtonAction(): ButtonAction {
    val own = rememberCoroutineScope()
    val scope = LocalActionScope.current ?: own
    return remember(scope) { ButtonAction(scope) }
}

@Composable
private fun ButtonLabel(text: String, spinning: Boolean) {
    if (spinning) {
        CircularProgressIndicator(Modifier.size(20.dp), color = LocalContentColor.current, strokeWidth = 2.dp)
        Spacer(Modifier.width(12.dp))
    }
    Text(text, textAlign = TextAlign.Center)
}

/** The one main action of a screen: accent pill, 56 dp (64 when `big`). */
@Composable
fun PrimaryButton(text: String, modifier: Modifier = Modifier, enabled: Boolean = true, big: Boolean = false, onClick: suspend () -> Unit) {
    val action = rememberButtonAction()
    Button(
        onClick = { action.run(onClick) },
        enabled = enabled,
        modifier = modifier.fillMaxWidth().heightIn(min = if (big) 64.dp else 56.dp),
        contentPadding = PaddingValues(horizontal = 24.dp, vertical = 12.dp),
    ) { ButtonLabel(text, action.spinning) }
}

/** Second-level action: accent-soft pill, 52 dp. `danger`: outlined in danger, for destructive steps. */
@Composable
fun SecondaryButton(text: String, modifier: Modifier = Modifier, enabled: Boolean = true, danger: Boolean = false, onClick: suspend () -> Unit) {
    val action = rememberButtonAction()
    val m = modifier.fillMaxWidth().heightIn(min = 52.dp)
    val padding = PaddingValues(horizontal = 24.dp, vertical = 12.dp)
    if (danger) {
        val error = MaterialTheme.colorScheme.error
        OutlinedButton(
            onClick = { action.run(onClick) }, modifier = m, enabled = enabled, contentPadding = padding,
            border = BorderStroke(1.dp, if (enabled) error else MaterialTheme.colorScheme.outlineVariant),
            colors = ButtonDefaults.outlinedButtonColors(contentColor = error),
        ) { ButtonLabel(text, action.spinning) }
    } else {
        FilledTonalButton(onClick = { action.run(onClick) }, modifier = m, enabled = enabled, contentPadding = padding) {
            ButtonLabel(text, action.spinning)
        }
    }
}

/** "Break the glass": wait 60 s + write the reason. The reason is NOT saved or sent. */
@Composable
fun EmergencySection(onUse: suspend () -> Unit) {
    var open by rememberSaveable { mutableStateOf(false) }
    var reason by remember { mutableStateOf("") }
    val danger = MaterialTheme.colorScheme.error
    if (!open) {
        TextButton(onClick = { open = true }, colors = ButtonDefaults.textButtonColors(contentColor = danger)) {
            Icon(PcoIcons.Alert, null, Modifier.size(20.dp))
            Spacer(Modifier.width(8.dp))
            Text(stringResource(R.string.emergency_title))
        }
        return
    }
    var left by remember { mutableIntStateOf(60) }
    LaunchedEffect(Unit) { while (left > 0) { delay(1_000); left-- } }
    AppCard {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(PcoIcons.Alert, null, Modifier.size(20.dp), tint = danger)
            Text(stringResource(R.string.emergency_title), Modifier.semantics { heading() }, style = MaterialTheme.typography.titleMedium, color = danger)
        }
        Text(stringResource(R.string.emergency_body), style = MaterialTheme.typography.bodyMedium)
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
        TextButton(onClick = { open = false }, modifier = Modifier.align(Alignment.CenterHorizontally)) { Text(stringResource(R.string.cancel)) }
    }
}

/** Inline error, next to what failed; announced by screen readers. */
@Composable
fun ErrorText(message: String?) {
    if (message == null) return
    Row(
        Modifier.fillMaxWidth().semantics(mergeDescendants = true) { liveRegion = LiveRegionMode.Assertive },
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(PcoIcons.Alert, null, Modifier.size(20.dp), tint = MaterialTheme.colorScheme.error)
        Text(message, style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.Medium, color = MaterialTheme.colorScheme.error)
    }
}
