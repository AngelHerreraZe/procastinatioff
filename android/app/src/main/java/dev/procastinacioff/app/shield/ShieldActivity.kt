package dev.procastinacioff.app.shield

import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
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
import dev.procastinacioff.app.MainActivity
import dev.procastinacioff.app.core.CoreHolder
import dev.procastinacioff.app.ui.EmergencySection
import dev.procastinacioff.app.ui.ErrorText
import dev.procastinacioff.app.ui.PcoTheme
import dev.procastinacioff.app.ui.PrimaryButton
import dev.procastinacioff.app.ui.SecondaryButton
import dev.procastinacioff.app.ui.Snapshot
import dev.procastinacioff.app.ui.coreAction
import dev.procastinacioff.app.ui.hhmm
import dev.procastinacioff.app.ui.lookFor
import dev.procastinacioff.app.ui.rememberSnapshot
import kotlinx.coroutines.launch
import uniffi.procastinacioff_core.DayState
import uniffi.procastinacioff_core.GoalKind
import uniffi.procastinacioff_core.Role

/** Escudo sobre lo no permitido. Siempre explica el bloqueo y el siguiente paso; nunca culpa. */
class ShieldActivity : ComponentActivity() {
    companion object { const val EXTRA_PACKAGE = "pkg" }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_SECURE) // sin capturas ni vista en recientes
        setContent {
            val snap = rememberSnapshot()
            PcoTheme(lookFor(snap.status, focus = false, systemDark = isSystemInDarkTheme(), nowMs = snap.nowMs)) {
                ShieldScreen(
                    snap,
                    onClose = { finish() },
                    onOpen = { route, goal ->
                        startActivity(MainActivity.intent(this, route, goal))
                        finish()
                    },
                )
            }
        }
    }
}

@Composable
private fun ShieldScreen(snap: Snapshot, onClose: () -> Unit, onOpen: (String, String?) -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var error by remember { mutableStateOf<String?>(null) }
    val st = snap.status

    BackHandler { /* el botón atrás no cierra el escudo */ }
    LaunchedEffect(st?.state) {
        val s = st?.state
        if (s is DayState.Free || s is DayState.Pass || s is DayState.Emergency) onClose()
    }
    if (st == null) return
    val next = st.goals.firstOrNull { it.kind == GoalKind.KEY && !it.completed }

    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp, androidx.compose.ui.Alignment.CenterVertically),
    ) {
        when (val s = st.state) {
            is DayState.Sleep -> {
                Title("Hora de dormir")
                Text("Las llamadas siguen funcionando. Despiertas a las ${hhmm(st.nextWakeMs)}.")
            }
            is DayState.Focus -> {
                Title("Ahora toca tu tarea")
                Text("Durante el enfoque solo están disponibles las apps de esa tarea.")
                PrimaryButton("Volver a la tarea") { onOpen(MainActivity.ROUTE_FOCUS, s.goalId) }
            }
            else -> if (st.needsPlan) {
                Title("Define tus metas de hoy")
                Text("Hasta entonces solo funcionan las apps esenciales.")
                PrimaryButton("Definir mis metas") { onOpen(MainActivity.ROUTE_PLAN, null) }
            } else {
                Title("Aún no cumples tus metas")
                next?.let { Text("Sigue con «${it.title}».") }
                next?.let { PrimaryButton("Continuar: ${it.title}") { onOpen(MainActivity.ROUTE_FOCUS, it.goalId) } }
                if (st.permitsLeft > 0u) {
                    SecondaryButton("Usar un permiso (${st.permitsLeft})") {
                        scope.launch { error = coreAction(ctx) { it.startPass(CoreHolder.clock()) } }
                    }
                }
            }
        }
        ErrorText(error)
        val minor = remember { runCatching { CoreHolder.get(ctx).myRole() == Role.MINOR }.getOrDefault(false) }
        if (minor && st.state !is DayState.Sleep) {
            SecondaryButton("Pedir a tu tutor") { onOpen(MainActivity.ROUTE_TUTOR, null) }
        }
        EmergencySection { scope.launch { error = coreAction(ctx) { it.emergency(CoreHolder.clock()) } } }
    }
}

@Composable
private fun Title(text: String) =
    Text(text, style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.Bold)
