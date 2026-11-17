package dev.procastinatioff.app.shield

import androidx.compose.ui.res.stringResource
import dev.procastinatioff.app.R
import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import dev.procastinatioff.app.MainActivity
import dev.procastinatioff.app.core.CoreHolder
import dev.procastinatioff.app.ui.EmergencySection
import dev.procastinatioff.app.ui.ErrorText
import dev.procastinatioff.app.ui.Footnote
import dev.procastinatioff.app.ui.IconCircle
import dev.procastinatioff.app.ui.PcoIcons
import dev.procastinatioff.app.ui.PcoTheme
import dev.procastinatioff.app.ui.PrimaryButton
import dev.procastinatioff.app.ui.Screen
import dev.procastinatioff.app.ui.SecondaryButton
import dev.procastinatioff.app.ui.Snapshot
import dev.procastinatioff.app.ui.coreAction
import dev.procastinatioff.app.ui.hhmm
import dev.procastinatioff.app.ui.lookFor
import dev.procastinatioff.app.ui.rememberSnapshot
import uniffi.procastinatioff_core.DayState
import uniffi.procastinatioff_core.GoalKind
import uniffi.procastinatioff_core.Role

/** Shield over what is not allowed. Always explains the block and the next step; never blames. */
class ShieldActivity : ComponentActivity() {
    companion object { const val EXTRA_PACKAGE = "pkg" }

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_SECURE) // no screenshots nor preview in recents
        setContent {
            val snap = rememberSnapshot()
            PcoTheme(lookFor(snap.status, snap.nowMs)) {
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
    var error by remember { mutableStateOf<String?>(null) }
    val st = snap.status

    BackHandler { /* the back button does not close the shield */ }
    LaunchedEffect(st?.state) {
        val s = st?.state
        if (s is DayState.Free || s is DayState.Pass || s is DayState.Emergency) onClose()
    }
    if (st == null) return
    val next = st.goals.firstOrNull { it.kind == GoalKind.KEY && !it.completed }
    val minor = remember { runCatching { CoreHolder.get(ctx).myRole() == Role.MINOR }.getOrDefault(false) }
    val state = st.state

    Screen(
        bottom = {
            ErrorText(error)
            when {
                state is DayState.Sleep -> Unit // at bedtime only calls: no other action
                state is DayState.Focus -> PrimaryButton(stringResource(R.string.back_to_task)) { onOpen(MainActivity.ROUTE_FOCUS, state.goalId) }
                st.needsPlan -> PrimaryButton(stringResource(R.string.shield_plan_button)) { onOpen(MainActivity.ROUTE_PLAN, null) }
                else -> {
                    next?.let { PrimaryButton(stringResource(R.string.shield_continue, it.title)) { onOpen(MainActivity.ROUTE_FOCUS, it.goalId) } }
                    if (st.permitsLeft > 0u) {
                        SecondaryButton(stringResource(R.string.use_pass_count, st.permitsLeft.toInt())) {
                            error = coreAction(ctx) { it.startPass(CoreHolder.clock()) }
                        }
                    }
                }
            }
            if (minor && state !is DayState.Sleep) {
                SecondaryButton(stringResource(R.string.ask_your_guardian)) { onOpen(MainActivity.ROUTE_TUTOR, null) }
            }
            EmergencySection { error = coreAction(ctx) { it.emergency(CoreHolder.clock()) } }
        },
    ) {
        Spacer(Modifier.height(32.dp))
        when {
            state is DayState.Sleep -> {
                IconCircle(PcoIcons.Moon, 72.dp)
                Title(stringResource(R.string.bedtime_title))
                Lead(stringResource(R.string.shield_sleep_lead, hhmm(st.nextWakeMs)))
            }
            state is DayState.Focus -> {
                IconCircle(PcoIcons.Clock, 72.dp)
                Title(stringResource(R.string.shield_focus_title))
                Lead(stringResource(R.string.shield_focus_only))
            }
            st.needsPlan -> {
                IconCircle(PcoIcons.Plus, 72.dp)
                Title(stringResource(R.string.shield_plan_title))
                Lead(stringResource(R.string.shield_plan_lead))
            }
            else -> {
                IconCircle(PcoIcons.Lock, 72.dp)
                Title(stringResource(R.string.shield_not_done_title))
                next?.let { Lead(stringResource(R.string.shield_continue_with, it.title)) }
            }
        }
        if (state !is DayState.Sleep) Footnote(stringResource(R.string.calls_stay_available), PcoIcons.Phone) // Sleep already says it
    }
}

@Composable
private fun Title(text: String) = Text(text, Modifier.semantics { heading() }, style = MaterialTheme.typography.headlineMedium)

@Composable
private fun Lead(text: String) = Text(text, style = MaterialTheme.typography.bodyLarge)
