package dev.procastinacioff.app.ui

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

private fun actionLabel(g: GoalProgress, def: Goal?): String =
    if (def?.methods?.contains(Method.FOCUS_SESSION) == true) (if (g.focusMinutes > 0u) "Continuar" else "Empezar")
    else "Marcar como hecha"

// ───────────────────────── Hoy (01 Calma) ─────────────────────────

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
                title = { Column { Text("Hoy"); Text(todayText(), style = MaterialTheme.typography.bodySmall) } },
                actions = {
                    if (!st.freeDay && !st.needsPlan) TextButton(onClick = onPlan) { Text("Añadir meta") }
                    if (minor) {
                        // El dispositivo de un menor no vincula ni tiene compañero: eso lo decide el tutor
                        TextButton(onClick = onTutor) { Text("Tu tutor") }
                    } else {
                        TextButton(onClick = onPair) { Text("Vincular") }
                        TextButton(onClick = onPartner) { Text("Compañero") }
                        onFamily?.let { TextButton(onClick = it) { Text("Familia") } }
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.background),
            )
        },
        bottomBar = {
            // Zona del pulgar: una acción principal, el permiso y la emergencia
            Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                ErrorText(error)
                if (state is DayState.Focus) {
                    PrimaryButton("Volver a la tarea") { onFocus(state.goalId) }
                } else if (canWork && next != null && nextDef != null) {
                    val left = next.targetMinutes.toInt() - next.focusMinutes.toInt()
                    val extra = if (next.targetMinutes > 0u) " · faltan ${duration(maxOf(0, left))}" else ""
                    PrimaryButton("${actionLabel(next, nextDef)}: ${next.title}$extra") { onStart(nextDef) }
                }
                if (state is DayState.Locked || state is DayState.Focus) {
                    SecondaryButton("Usar un permiso (${st.permitsLeft})", enabled = st.permitsLeft > 0u) {
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
            // Latidos cruzados: un dispositivo que deja de reportar se nota en los demás
            items(silentDevices(snap), key = { it.deviceId }) { d ->
                Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface)) {
                    Text(
                        "«${d.label}» no reporta ${hoursAgo(d.lastSeenMs, snap.nowMs)}. Si lo apagaste, no pasa nada; " +
                            "si no, revisa que la app siga activa allí.",
                        Modifier.padding(16.dp).semantics { liveRegion = LiveRegionMode.Polite },
                    )
                }
            }
            item { StatusCard(st, snap.nowMs, pending.size, next?.title) }
            if (st.goals.isNotEmpty()) {
                item { Text("Metas de hoy", style = MaterialTheme.typography.titleMedium, modifier = Modifier.semantics { heading() }) }
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
        is DayState.Locked -> if (pending == 1) "Bloqueado · falta 1 meta" else "Bloqueado · faltan $pending metas"
        is DayState.Pass -> "Permiso activo · quedan ${minutesUntil(state.untilMs, now)} min"
        is DayState.Emergency -> "Emergencia · quedan ${minutesUntil(state.untilMs, now)} min"
        is DayState.Focus -> "Enfoque en marcha"
        is DayState.Free -> if (st.freeDay) "Día libre" else "Libre"
        is DayState.Sleep -> "Hora de dormir"
    }
    val sentence = when (state) {
        is DayState.Locked -> nextTitle?.let { "Completa ${if (pending == 1) "la meta que falta" else "$pending metas"} para desbloquear. Sigue con «$it»." } ?: ""
        is DayState.Pass -> "Cuando termine, vuelve el bloqueo."
        is DayState.Emergency -> "Todo está disponible. Queda registrado."
        is DayState.Focus -> "Solo están disponibles las apps de tu tarea."
        is DayState.Free -> if (st.freeDay) "Hoy no hay metas ni bloqueo en ningún dispositivo."
        else st.sleepAtMs?.let { "Libre hasta las ${hhmm(it)}." } ?: "Libre hasta mañana."
        is DayState.Sleep -> "Las llamadas siguen funcionando."
    }
    Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.primaryContainer)) {
        Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(title, style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold, color = MaterialTheme.colorScheme.onBackground)
            if (sentence.isNotEmpty()) Text(sentence, color = MaterialTheme.colorScheme.onSurfaceVariant)
            val chips = buildList {
                st.sleepAtMs?.let { add("Dormir a las ${hhmm(it)}") }
                if (!st.freeDay) add("${st.permitsLeft} ${if (st.permitsLeft.toInt() == 1) "permiso" else "permisos"}")
            }
            if (chips.isNotEmpty()) Text(chips.joinToString(" · "), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onPrimaryContainer)
        }
    }
}

// ───────────────────────── Plan del día ─────────────────────────

private data class PlanRow(val goal: Goal, val locked: Boolean)

/**
 * Se abre solo al primer desbloqueo tras la hora de dormir. Las metas fijas ya vienen incluidas
 * (se pueden subir, no quitar). Tras confirmar, el plan solo se puede endurecer hasta mañana.
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
        Text(if (adding) "Añadir metas a hoy" else "Buenos días", style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.Bold)
        Text(
            if (adding) "Puedes añadir metas o subir objetivos. Lo que ya está no se puede quitar hasta mañana."
            else "Define tus metas de hoy. Al confirmarlas quedan fijas hasta mañana: después solo podrás añadir o subirlas.",
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
                                if (r.goal.targetMinutes > 0u) duration(r.goal.targetMinutes.toInt()) else "${r.goal.targetCount} veces",
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                    if (r.goal.targetMinutes > 0u) {
                        TextButton(onClick = {
                            rows[i] = r.copy(goal = r.goal.copy(targetMinutes = (r.goal.targetMinutes.toInt() + 5).coerceAtMost(600).toUShort()))
                        }) { Text("+5 min") }
                    }
                    if (!r.locked) TextButton(onClick = { rows.removeAt(i) }) { Text("Quitar") }
                }
            }
        }

        Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface)) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text("Añadir una meta", style = MaterialTheme.typography.titleMedium)
                OutlinedTextField(title, { title = it.take(80) }, label = { Text("Nombre") }, modifier = Modifier.fillMaxWidth())
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                    FilterChip(selected = key, onClick = { key = true }, label = { Text("Desbloquea") })
                    FilterChip(selected = !key, onClick = { key = false }, label = { Text("Hábito") })
                    if (check.usesMinutes) {
                        OutlinedTextField(
                            minutes, { minutes = it.filter(Char::isDigit).take(3) },
                            label = { Text("Minutos") },
                            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                            modifier = Modifier.width(120.dp),
                        )
                    }
                }
                Text("Cómo se comprueba", style = MaterialTheme.typography.labelLarge)
                Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    Check.entries.forEach { c -> FilterChip(selected = check == c, onClick = { check = c }, label = { Text(c.label) }) }
                }
                SecondaryButton("Añadir", enabled = title.isNotBlank()) {
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
                    "Toda meta se puede marcar como hecha con una espera de 60 s. La cámara nunca graba: solo cuenta si se cumplió.",
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        }

        ErrorText(error)
        val hasKey = rows.any { it.goal.kind == GoalKind.KEY }
        if (!hasKey) Text("Necesitas al menos una meta que desbloquee.", color = MaterialTheme.colorScheme.onSurfaceVariant)
        PrimaryButton(if (adding) "Guardar cambios" else "Empezar el día", enabled = hasKey) {
            scope.launch {
                error = coreAction(ctx) { it.planDay(dev.procastinacioff.app.core.CoreHolder.clock(), rows.map { r -> r.goal }) }
                if (error == null) onDone()
            }
        }
        if (adding) TextButton(onClick = onCancel) { Text("Cancelar") }
    }
}

// ───────────────────────── Foco (08) ─────────────────────────

/**
 * La siguiente acción, enorme. Meta con tiempo → sesión de ENFOQUE (el núcleo mide los minutos).
 * Meta sin tiempo → declaración con fricción: escribir qué hiciste + 60 s (el texto no se guarda).
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

    // Cámara: solo en el APK verificador (sin Internet). Aquí solo llegan los 5 campos del resultado.
    val cameraMethod = def?.methods?.firstOrNull { it in CAMERA_METHODS }
    val verifier = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { r ->
        scope.launch {
            val message = withContext(Dispatchers.IO) { onVerifierResult(ctx, goalId, r.resultCode, r.data) }
            if (message == null) onBack() else error = message
        }
    }

    if (!focusing) LaunchedEffect(goalId) { while (left > 0) { delay(1_000); left-- } }
    // Si la meta ya está hecha o la sesión terminó en otro dispositivo, volver al día
    LaunchedEffect(progress?.completed, st.state) {
        if (progress?.completed == true || (def?.methods?.contains(Method.FOCUS_SESSION) == true && !focusing && st.state !is DayState.Locked)) onBack()
    }

    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
            TextButton(onClick = onBack) { Text("Ver todo el día") }
            if (keys.isNotEmpty()) Text("Paso $step de ${keys.size}", color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Card(
            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
            shape = MaterialTheme.shapes.large,
        ) {
            Column(Modifier.padding(28.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                Text("Ahora", color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.SemiBold)
                Text(progress?.title ?: "Tu meta", fontSize = 34.sp, lineHeight = 40.sp, fontWeight = FontWeight.Bold)
                if (focusing) {
                    Text("Siéntate, abre lo que necesitas y sigue. Solo están disponibles las apps de esta tarea.", fontSize = 18.sp)
                    Text(
                        "$remaining min",
                        fontSize = 72.sp,
                        fontWeight = FontWeight.SemiBold,
                        modifier = Modifier.semantics {
                            contentDescription = "Quedan $remaining minutos"
                            if (remaining % 5 == 0 || remaining <= 1) liveRegion = LiveRegionMode.Polite
                        },
                    )
                    ProgressBar(done, target, "Progreso", thick = true)
                    Text("$done de $target min", color = MaterialTheme.colorScheme.onSurfaceVariant)
                    PrimaryButton(if (remaining == 0) "Terminar: meta cumplida" else "Terminar sesión por ahora", big = true) {
                        scope.launch {
                            error = coreAction(ctx) { it.endFocus(dev.procastinacioff.app.core.CoreHolder.clock()) }
                            if (error == null) onBack()
                        }
                    }
                } else {
                    Text("Escribe en una frase lo que hiciste. Después de 60 s podrás marcarla como hecha.", fontSize = 18.sp)
                    OutlinedTextField(
                        note, { note = it.take(300) },
                        label = { Text("Lo que hiciste") },
                        supportingText = { Text("No se guarda ni se envía: solo cuenta como «autodeclarada».") },
                        modifier = Modifier.fillMaxWidth(),
                    )
                    PrimaryButton(if (left > 0) "Espera $left s" else "Marcar como hecha", enabled = left == 0 && note.trim().length >= 10, big = true) {
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
                        when (cameraMethod) {
                            Method.POSE_REPS -> "Contar repeticiones con la cámara"
                            Method.FOOD_PHOTO -> "Foto del plato en vivo"
                            Method.PAGE_OCR, Method.SUMMARY -> "Fotografiar las páginas y resumir"
                            else -> "Comprobar la lectura con la cámara"
                        },
                    ) {
                        if (!verifierIsGenuine(ctx)) {
                            error = "Instala el verificador de ProcastinaciOFF (no tiene acceso a Internet) para usar la cámara."
                        } else {
                            verifierIntent(goalId, cameraMethod, def?.targetMinutes?.toInt() ?: 0)?.let { verifier.launch(it) }
                        }
                    }
                    Text(
                        "La cámara solo se usa en el verificador, que no tiene Internet. Nada se graba.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                ErrorText(error)
            }
        }
        after?.let { Text("Después: ${it.title}", fontWeight = FontWeight.SemiBold) }
        habit?.let { Text("Sin prisa: ${it.title} (hábito)", color = MaterialTheme.colorScheme.onSurfaceVariant) }
        if (focusing) {
            FilledTonalButton(
                onClick = { scope.launch { error = coreAction(ctx) { it.startPass(dev.procastinacioff.app.core.CoreHolder.clock()) } } },
                enabled = st.permitsLeft > 0u,
            ) { Text("Permiso (${st.permitsLeft})") }
            EmergencySection {
                scope.launch { error = coreAction(ctx) { it.emergency(dev.procastinacioff.app.core.CoreHolder.clock()) } }
            }
        }
    }
}
