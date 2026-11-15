package dev.procastinacioff.app.ui

import android.os.Build
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
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
import dev.procastinacioff.app.core.CoreHolder
import dev.procastinacioff.app.core.userMessage
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.procastinacioff_core.PartnerAlert
import uniffi.procastinacioff_core.PartnerAlertView
import uniffi.procastinacioff_core.TamperKind

@Composable
private fun Bullets(title: String, items: List<String>) {
    Text(title, fontWeight = FontWeight.SemiBold)
    items.forEach { Text("• $it", color = MaterialTheme.colorScheme.onSurfaceVariant) }
}

/** Invitar a mi compañero de rendición de cuentas. Explica, antes de nada, qué verá y qué no. */
@Composable
fun PartnerScreen(onInvite: () -> Unit, onBack: () -> Unit) {
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var linked by remember { mutableStateOf<Int?>(null) } // compañeros vinculados (null = sin buzón)
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        linked = withContext(Dispatchers.IO) { CoreHolder.partnerBoxOrNull(ctx)?.members()?.size?.minus(1) }
    }

    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Text("Compañero de rendición de cuentas", style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
        Text("Alguien de confianza que recibe un aviso si te saltas las reglas. Elige a alguien que te apoye, no que te vigile.")
        Bullets("Verá:", listOf(
            "Si desactivas la protección, cambias la hora o intentas desinstalar.",
            "Si usas la emergencia.",
            "Cuántas metas cumpliste cada día (solo el número).",
        ))
        Bullets("Nunca verá:", listOf("Tus metas ni sus nombres.", "Qué apps usas, tus mensajes, tu ubicación ni fotos."))
        Text("Solo desde hoy: no recibe nada de los días anteriores.", style = MaterialTheme.typography.bodySmall)
        ErrorText(error)
        linked?.let { n ->
            Text(if (n > 0) "Tienes $n ${if (n == 1) "compañero vinculado" else "compañeros vinculados"}." else "Aún no se ha unido nadie.")
        }
        PrimaryButton(if (linked == null) "Invitar a mi compañero" else "Invitar a otra persona", enabled = !busy) {
            busy = true
            scope.launch {
                // Crea el buzón (una vez) y lo conecta al mismo relay que el perfil.
                error = withContext(Dispatchers.IO) {
                    val url = CoreHolder.get(ctx).relayUrl()
                    if (url == null) {
                        "Primero conecta un servidor de sincronización en tu perfil."
                    } else {
                        runCatching {
                            val box = CoreHolder.partnerBox(ctx)
                            if (box.members().isEmpty()) box.createPartnerMailbox(CoreHolder.clock(), Build.MODEL.take(64))
                            if (box.relayUrl() == null) box.connectRelay(url)
                        }.exceptionOrNull()?.userMessage()
                    }
                }
                busy = false
                if (error == null) onInvite()
            }
        }
        TextButton(onClick = onBack) { Text("Volver") }
    }
}

private val DATE = SimpleDateFormat("EEE d 'de' MMM, HH:mm", Locale("es"))
private val DAY = SimpleDateFormat("EEEE d 'de' MMMM", Locale("es"))

private fun describe(a: PartnerAlertView): Pair<String, String> = when (val alert = a.alert) {
    is PartnerAlert.Emergency -> "Usó la emergencia" to DATE.format(Date(a.happenedAtMs))
    is PartnerAlert.Tamper -> when (alert.what) {
        TamperKind.PERMISSION_REVOKED -> "Desactivó un permiso de protección"
        TamperKind.SERVICE_STOPPED -> "La protección estuvo detenida"
        TamperKind.CLOCK_CHANGED -> "Cambió la hora del dispositivo"
        TamperKind.UNINSTALL_ATTEMPT -> "Intentó desinstalar la app"
        TamperKind.SAFE_MODE -> "Arrancó en modo seguro"
        TamperKind.DEVICE_SILENT -> "Un dispositivo dejó de reportar"
    } to DATE.format(Date(a.happenedAtMs))
    is PartnerAlert.DailySummary -> "Cumplió ${alert.completed} de ${alert.total} metas" to DAY.format(Date(a.happenedAtMs))
}

/** Lo que ve el compañero: solo avisos y números, nunca metas ni textos. */
@Composable
fun WatchScreen() {
    val ctx = LocalContext.current
    var alerts by remember { mutableStateOf<List<PartnerAlertView>>(emptyList()) }
    LaunchedEffect(Unit) {
        while (true) {
            alerts = withContext(Dispatchers.IO) {
                runCatching {
                    val box = CoreHolder.watchingBox(ctx)
                    runCatching { box.sync(CoreHolder.clock()) }
                    box.partnerAlerts()
                }.getOrDefault(emptyList())
            }
            delay(60_000)
        }
    }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(24.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text("Persona a la que acompañas", style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
        Text(
            "Solo ves esto: avisos de manipulación, emergencias y cuántas metas cumplió cada día. Nunca sus metas, apps, mensajes ni ubicación.",
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        if (alerts.isEmpty()) Text("Todavía no hay avisos.")
        alerts.forEach { a ->
            val (what, `when`) = describe(a)
            Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface)) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(what, fontWeight = FontWeight.SemiBold)
                    Text("${`when`.replaceFirstChar { it.uppercase() }} · ${a.deviceLabel}", style = MaterialTheme.typography.bodySmall)
                }
            }
        }
    }
}
