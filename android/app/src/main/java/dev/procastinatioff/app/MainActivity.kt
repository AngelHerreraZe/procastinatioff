package dev.procastinatioff.app

import android.content.Context
import android.content.Intent
import android.os.Bundle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.fragment.app.FragmentActivity
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import dev.procastinatioff.app.core.CoreHolder
import dev.procastinatioff.app.ui.AuthSettingsScreen
import dev.procastinatioff.app.ui.ChildScreen
import dev.procastinatioff.app.ui.ChildSettingsScreen
import dev.procastinatioff.app.ui.ChooseScreen
import dev.procastinatioff.app.ui.ConfigScreen
import dev.procastinatioff.app.ui.ContactScreen
import dev.procastinatioff.app.ui.FamilyConsentScreen
import dev.procastinatioff.app.ui.FamilyListScreen
import dev.procastinatioff.app.ui.FocusScreen
import dev.procastinatioff.app.ui.HomeScreen
import dev.procastinatioff.app.ui.InviteScreen
import dev.procastinatioff.app.ui.JoinScreen
import dev.procastinatioff.app.ui.PcoTheme
import dev.procastinatioff.app.ui.PermissionsScreen
import dev.procastinatioff.app.ui.PlanScreen
import dev.procastinatioff.app.ui.TutorScreen
import dev.procastinatioff.app.ui.VerifierScreen
import dev.procastinatioff.app.ui.WatchScreen
import dev.procastinatioff.app.ui.WelcomeScreen
import dev.procastinatioff.app.ui.coreAction
import dev.procastinatioff.app.ui.lookFor
import dev.procastinatioff.app.ui.rememberSnapshot
import kotlinx.coroutines.launch
import uniffi.procastinatioff_core.DayState
import uniffi.procastinatioff_core.Goal
import uniffi.procastinatioff_core.Method
import uniffi.procastinatioff_core.Role

/** FragmentActivity (not ComponentActivity): the guardian's biometric confirmation needs it. */
class MainActivity : FragmentActivity() {
    companion object {
        const val EXTRA_ROUTE = "route"
        const val EXTRA_GOAL = "goal"
        const val ROUTE_PLAN = "plan"
        const val ROUTE_FOCUS = "focus"
        const val ROUTE_TUTOR = "tutor"

        fun intent(ctx: Context, route: String, goalId: String? = null): Intent =
            Intent(ctx, MainActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP)
                .putExtra(EXTRA_ROUTE, route)
                .putExtra(EXTRA_GOAL, goalId)
    }

    /** External request (shield or first unlock): route + goal. */
    private var request by mutableStateOf<Pair<String, String?>?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge() // same on every version (targetSdk 35 forces it on Android 15); screens pad the safe areas
        super.onCreate(savedInstanceState)
        request = intent.getStringExtra(EXTRA_ROUTE)?.let { it to intent.getStringExtra(EXTRA_GOAL) }
        val hasProfile = runCatching { CoreHolder.get(this).members().isNotEmpty() }.getOrDefault(false)
        val watching = !hasProfile && runCatching { CoreHolder.watchingBoxOrNull(this) != null }.getOrDefault(false)
        val guardian = CoreHolder.familyProfiles(this).isNotEmpty()
        val start = when {
            hasProfile -> "home"
            guardian -> "family"
            watching -> "watch"
            else -> "welcome"
        }
        setContent { Root(start, request) { request = null } }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        request = intent.getStringExtra(EXTRA_ROUTE)?.let { it to intent.getStringExtra(EXTRA_GOAL) }
    }
}

@Composable
private fun Root(start: String, request: Pair<String, String?>?, consumed: () -> Unit) {
    val ctx = LocalContext.current
    val nav = rememberNavController()
    val snap = rememberSnapshot()
    val scope = rememberCoroutineScope()
    val entry by nav.currentBackStackEntryAsState()
    val route = entry?.destination?.route
    val look = lookFor(snap.status, snap.nowMs)
    val minor = remember(snap.status != null) { runCatching { CoreHolder.get(ctx).myRole() == Role.MINOR }.getOrDefault(false) }

    // Open a goal: timed → FOCUS session in the core; untimed → declaration (Focus screen).
    fun start(goal: Goal) {
        scope.launch {
            if (Method.FOCUS_SESSION in goal.methods && snap.status?.state !is DayState.Focus) {
                coreAction(ctx) { it.startFocus(CoreHolder.clock(), goal.id) }
            }
            nav.navigate("focus/${goal.id}")
        }
    }

    // First unlock after sleep: the day plan
    val st = snap.status
    LaunchedEffect(st?.needsPlan, route) {
        if (st?.needsPlan == true && st.state is DayState.Locked && route == "home") nav.navigate("plan")
    }
    LaunchedEffect(request, st != null) {
        val (r, goalId) = request ?: return@LaunchedEffect
        if (st == null) return@LaunchedEffect
        when (r) {
            MainActivity.ROUTE_PLAN -> if (route != "plan") nav.navigate("plan")
            MainActivity.ROUTE_TUTOR -> if (route != "tutor") nav.navigate("tutor")
            MainActivity.ROUTE_FOCUS -> snap.goals.firstOrNull { it.id == goalId }?.let { start(it) }
        }
        consumed()
    }

    PcoTheme(look) {
        NavHost(nav, startDestination = start) {
            // The verifier goes first: once Accessibility is on, the blocker gets in the installer's way.
            composable("welcome") { WelcomeScreen { nav.navigate("verifier") } }
            composable("verifier") { VerifierScreen(onNext = { nav.navigate("permissions") }, onBack = { nav.popBackStack() }) }
            composable("permissions") {
                // Onboarding goes on to "choose"; from Home (blocking paused) it is a checklist that returns.
                val onboarding = nav.previousBackStackEntry?.destination?.route != "home"
                PermissionsScreen(
                    onboarding,
                    onNext = { if (onboarding) nav.navigate("choose") else nav.popBackStack() },
                    onBack = { nav.popBackStack() },
                )
            }
            composable("choose") {
                ChooseScreen(
                    onNew = { nav.navigate("config") },
                    onJoin = { nav.navigate("join") },
                    onWatch = { nav.navigate("watch-join") },
                    onFamily = { nav.navigate("family-consent") },
                    onBack = { nav.popBackStack() },
                )
            }
            // Family mode: guardian
            composable("family-consent") { FamilyConsentScreen(onAccept = { nav.navigate("family-new") }, onBack = { nav.popBackStack() }) }
            composable("family-new") { ConfigScreen(family = true, onBack = { nav.popBackStack() }) { nav.navigate("family") { popUpTo(0) } } }
            composable("family") { FamilyListScreen(onOpen = { nav.navigate("child/$it") }, onNew = { nav.navigate("family-consent") }) }
            composable("child/{id}") { e ->
                val id = e.arguments?.getString("id").orEmpty()
                ChildScreen(
                    id,
                    onInvite = { role -> nav.navigate("child/$id/invite/${role.name}") },
                    onSettings = { nav.navigate("child/$id/settings") },
                    onBack = { nav.popBackStack() },
                )
            }
            composable("child/{id}/settings") { e -> ChildSettingsScreen(e.arguments?.getString("id").orEmpty()) { nav.popBackStack() } }
            composable("child/{id}/invite/{role}") { e ->
                val id = e.arguments?.getString("id").orEmpty()
                val role = if (e.arguments?.getString("role") == Role.GUARDIAN.name) Role.GUARDIAN else Role.MINOR
                InviteScreen(CoreHolder.family(ctx, id), role) { nav.popBackStack() }
            }
            // Family mode: minor
            composable("tutor") { TutorScreen { nav.popBackStack() } }
            // Change authorization and trusted contact
            composable("auth") {
                AuthSettingsScreen(
                    onContact = { nav.navigate("contact") },
                    onErased = { nav.navigate("welcome") { popUpTo(0) } },
                    onBack = { nav.popBackStack() },
                )
            }
            composable("contact") { ContactScreen(onInvite = { nav.navigate("contact-invite") }, onBack = { nav.popBackStack() }) }
            composable("contact-invite") { InviteScreen(CoreHolder.contactChannel(ctx)) { nav.popBackStack() } }
            composable("watch-join") { JoinScreen(CoreHolder.watchingBox(ctx), onBack = { nav.popBackStack() }) { nav.navigate("watch") { popUpTo(0) } } }
            composable("watch") { WatchScreen() }
            composable("config") { ConfigScreen(onBack = { nav.popBackStack() }) { nav.navigate("home") { popUpTo(0) } } }
            composable("join") { JoinScreen(onBack = { nav.popBackStack() }) { nav.navigate("home") { popUpTo(0) } } }
            composable("home") {
                HomeScreen(
                    snap,
                    onStart = { start(it) },
                    onFocus = { nav.navigate("focus/$it") },
                    onPlan = { nav.navigate("plan") },
                    onPair = { nav.navigate("invite") },
                    onSettings = { nav.navigate("auth") },
                    onPermissions = { nav.navigate("permissions") },
                    minor = minor,
                    onTutor = { nav.navigate("tutor") },
                    onFamily = if (CoreHolder.familyProfiles(ctx).isNotEmpty()) ({ nav.navigate("family") }) else null,
                )
            }
            composable("plan") {
                PlanScreen(
                    snap,
                    adding = snap.status?.needsPlan == false,
                    onDone = { nav.navigate("home") { popUpTo(0) } },
                    onCancel = { nav.popBackStack() },
                )
            }
            composable("invite") { InviteScreen { nav.popBackStack() } }
            composable("focus/{goalId}") { e ->
                FocusScreen(e.arguments?.getString("goalId").orEmpty(), snap) { nav.navigate("home") { popUpTo(0) } }
            }
        }
    }
}
