package dev.procastinacioff.app.shield

import androidx.compose.ui.res.stringResource
import dev.procastinacioff.app.R
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

/** Shield over what is not allowed. Always explains the block and the next step; never blames. */
class ShieldActivity : ComponentActivity() {
    companion object { const val EXTRA_PACKAGE = "pkg" }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_SECURE) // no screenshots nor preview in recents
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

    BackHandler { /* the back button does not close the shield */ }
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
                Title(stringResource(R.string.bedtime_title))
                Text(stringResource(R.string.shield_sleep_lead, hhmm(st.nextWakeMs)))
            }
            is DayState.Focus -> {
                Title(stringResource(R.string.shield_focus_title))
                Text(stringResource(R.string.shield_focus_only))
                PrimaryButton(stringResource(R.string.back_to_task)) { onOpen(MainActivity.ROUTE_FOCUS, s.goalId) }
            }
            else -> if (st.needsPlan) {
                Title(stringResource(R.string.shield_plan_title))
                Text(stringResource(R.string.shield_plan_lead))
                PrimaryButton(stringResource(R.string.shield_plan_button)) { onOpen(MainActivity.ROUTE_PLAN, null) }
            } else {
                Title(stringResource(R.string.shield_not_done_title))
                next?.let { Text(stringResource(R.string.shield_continue_with, it.title)) }
                next?.let { PrimaryButton(stringResource(R.string.shield_continue, it.title)) { onOpen(MainActivity.ROUTE_FOCUS, it.goalId) } }
                if (st.permitsLeft > 0u) {
                    SecondaryButton(stringResource(R.string.use_pass_count, st.permitsLeft.toInt())) {
                        scope.launch { error = coreAction(ctx) { it.startPass(CoreHolder.clock()) } }
                    }
                }
            }
        }
        ErrorText(error)
        val minor = remember { runCatching { CoreHolder.get(ctx).myRole() == Role.MINOR }.getOrDefault(false) }
        if (minor && st.state !is DayState.Sleep) {
            SecondaryButton(stringResource(R.string.ask_your_guardian)) { onOpen(MainActivity.ROUTE_TUTOR, null) }
        }
        EmergencySection { scope.launch { error = coreAction(ctx) { it.emergency(CoreHolder.clock()) } } }
    }
}

@Composable
private fun Title(text: String) =
    Text(text, style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.Bold)
