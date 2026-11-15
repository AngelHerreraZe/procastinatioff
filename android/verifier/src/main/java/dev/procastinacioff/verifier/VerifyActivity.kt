package dev.procastinacioff.verifier

import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.os.Build
import android.os.Bundle
import android.speech.RecognitionListener
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import android.util.Size
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.Preview
import androidx.camera.core.resolutionselector.ResolutionSelector
import androidx.camera.core.resolutionselector.ResolutionStrategy
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.LocalLifecycleOwner
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean
import uniffi.procastinacioff_verify.Exercise
import uniffi.procastinacioff_verify.ReadingTracker
import uniffi.procastinacioff_verify.RepCounter
import uniffi.procastinacioff_verify.VMethod
import uniffi.procastinacioff_verify.VerificationResult
import uniffi.procastinacioff_verify.countFingers
import uniffi.procastinacioff_verify.dhash
import uniffi.procastinacioff_verify.isDuplicate
import uniffi.procastinacioff_verify.judgeFood
import uniffi.procastinacioff_verify.judgePages
import uniffi.procastinacioff_verify.newFingerChallenge
import uniffi.procastinacioff_verify.pageNumber
import uniffi.procastinacioff_verify.salt
import uniffi.procastinacioff_verify.scoreSummary

/** Parámetros que manda la app principal (nada más entra en el verificador). */
data class Args(val goalId: String, val method: String, val targetMinutes: Int, val targetReps: Int, val minPages: Int)

class VerifyActivity : ComponentActivity() {
    private lateinit var detectors: Detectors
    private val done = AtomicBoolean(false)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // 🔒 Sin capturas ni vista en recientes; pantalla encendida mientras se verifica
        window.addFlags(WindowManager.LayoutParams.FLAG_SECURE or WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        val goalId = intent.getStringExtra("goal_id")
        val method = intent.getStringExtra("method")
        if (goalId == null || method == null) {
            setResult(Activity.RESULT_CANCELED)
            finish()
            return
        }
        detectors = Detectors(this)
        val args = Args(
            goalId, method,
            intent.getIntExtra("target_minutes", 30).coerceIn(1, 600),
            intent.getIntExtra("target_reps", 20).coerceIn(1, 500),
            intent.getIntExtra("min_pages", 10).coerceIn(1, 300),
        )
        setContent { MaterialTheme { VerifyScreen(args, detectors, ::deliver) } }
    }

    /** Devuelve SOLO los 5 campos de VerificationResult a la app principal. Una sola vez. */
    fun deliver(r: VerificationResult?) {
        if (!done.compareAndSet(false, true)) return
        runOnUiThread {
            val data = Intent()
            if (r != null) {
                data.putExtra("goal_id", r.goalId).putExtra("method", r.method.name)
                    .putExtra("approved", r.approved).putExtra("score", r.score.toInt()).putExtra("minutes", r.minutes.toInt())
            }
            setResult(if (r != null) Activity.RESULT_OK else Activity.RESULT_CANCELED, data)
            finish()
        }
    }

    override fun onDestroy() {
        if (::detectors.isInitialized) detectors.close()
        super.onDestroy()
    }
}

/** Reduce a 9×8 en gris, en memoria (para el hash perceptual). */
private fun gray9x8(src: Bitmap): ByteArray {
    val small = Bitmap.createScaledBitmap(src, 9, 8, true)
    val out = ByteArray(72)
    for (y in 0 until 8) for (x in 0 until 9) {
        val p = small.getPixel(x, y)
        out[y * 9 + x] = ((((p shr 16) and 0xff) * 30 + ((p shr 8) and 0xff) * 59 + (p and 0xff) * 11) / 100).toByte()
    }
    small.recycle()
    return out
}

@Composable
private fun VerifyScreen(args: Args, detectors: Detectors, deliver: (VerificationResult?) -> Unit) {
    val ctx = LocalContext.current
    val main = remember { ContextCompat.getMainExecutor(ctx) }
    var cameraOk by remember { mutableStateOf(granted(ctx, Manifest.permission.CAMERA)) }
    val askCamera = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { cameraOk = it }
    var progress by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    val report: (Throwable) -> Unit = { e -> main.execute { error = e.message ?: "No se pudo analizar la imagen" } }

    // Estado de los flujos (solo en memoria)
    val tracker = remember { ReadingTracker() }
    val counter = remember { RepCounter(Exercise.SQUAT) }
    val challenge = remember { newFingerChallenge() }
    var step by remember { mutableStateOf(if (args.method == "SUMMARY") 2 else 0) } // páginas: 0 inicio · 1 leyendo · 2 final · 3 resumen
    var firstPage by remember { mutableStateOf<UShort?>(null) }
    var lastPage by remember { mutableStateOf<UShort?>(null) }
    var source by remember { mutableStateOf("") } // texto OCR: solo RAM, se descarta al terminar
    var summary by remember { mutableStateOf("") }
    val capture = remember { AtomicBoolean(false) }

    BackHandler { deliver(null) }

    val pages = args.method == "PAGE_OCR" || args.method == "SUMMARY"
    val cameraNeeded = !pages || step == 0 || step == 2
    val front = args.method == "READING_PRESENCE" || args.method == "POSE_REPS"

    val analyzer: ImageAnalysis.Analyzer? = remember(args.method, step) {
        when (args.method) {
            "READING_PRESENCE" -> FrameSource(20_000) { bmp, at ->
                runCatching {
                    val m = tracker.addSample(at, detectors.hasFace(bmp), detectors.hasBook(bmp))
                    main.execute { progress = "$m de ${args.targetMinutes} min" }
                    if (m.toInt() >= args.targetMinutes) deliver(tracker.finish(args.goalId, args.targetMinutes.toUShort()))
                }.onFailure(report)
            }
            "POSE_REPS" -> FrameSource(100) { bmp, at ->
                runCatching {
                    val lm = detectors.poseLandmarks(bmp) ?: return@FrameSource
                    val reps = counter.feed(lm, at)
                    main.execute { progress = "$reps de ${args.targetReps} repeticiones" }
                    if (reps.toInt() >= args.targetReps) {
                        deliver(VerificationResult(args.goalId, VMethod.POSE_REPS, true, 100u, 0u))
                    }
                }.onFailure(report)
            }
            "FOOD_PHOTO" -> FrameSource(300) { bmp, _ ->
                runCatching {
                    val fingers = detectors.hand(bmp)?.let { countFingers(it) } ?: return@FrameSource
                    if (fingers != challenge || !capture.compareAndSet(false, true)) return@FrameSource
                    val salted = salt(dhash(gray9x8(bmp)) ?: return@FrameSource, HashStore.mask(ctx))
                    val duplicate = isDuplicate(salted, HashStore.recent(ctx))
                    HashStore.add(ctx, salted) // solo el hash salado, 7 días
                    deliver(judgeFood(args.goalId, detectors.foodLabels(bmp), emptyList(), true, duplicate))
                }.onFailure(report)
            }
            "PAGE_OCR", "SUMMARY" -> if (step == 0 || step == 2) FrameSource(300) { bmp, _ ->
                if (!capture.compareAndSet(true, false)) return@FrameSource
                runCatching {
                    val (text, lines) = detectors.readPage(bmp)
                    val page = pageNumber(lines, bmp.height)
                    main.execute {
                        when {
                            args.method == "PAGE_OCR" && page == null ->
                                error = "No veo el número de página. Acerca la cámara al borde de la página."
                            step == 0 -> { firstPage = page; error = null; step = 1 }
                            else -> { lastPage = page; source = text; error = null; step = 3 }
                        }
                    }
                }.onFailure(report)
            } else null
            else -> null
        }
    }

    Column(Modifier.fillMaxSize().padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        // Banner fijo: el usuario siempre sabe que la cámara está activa y puede detenerla
        Row(
            Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.primaryContainer).padding(12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text(
                if (cameraNeeded) "Cámara activa · no se graba nada · sin Internet" else "Cámara apagada · sin Internet",
                fontWeight = FontWeight.SemiBold,
                modifier = Modifier.weight(1f),
            )
            OutlinedButton(onClick = {
                // Lectura: lo acumulado cuenta (se aprueba si llega al 85 %); el resto, sin resultado
                deliver(if (args.method == "READING_PRESENCE") tracker.finish(args.goalId, args.targetMinutes.toUShort()) else null)
            }) { Text("Detener") }
        }

        Text(
            when (args.method) {
                "READING_PRESENCE" -> "Coloca el teléfono para que se vea tu cara y el libro. Se toma una muestra cada 20 s."
                "POSE_REPS" -> "Colócate de cuerpo entero frente a la cámara y haz ${args.targetReps} sentadillas."
                "FOOD_PHOTO" -> "Muestra $challenge ${if (challenge.toInt() == 1) "dedo" else "dedos"} junto al plato."
                else -> when (step) {
                    0 -> "Enfoca la página donde empiezas a leer y pulsa «Capturar»."
                    1 -> "Lee. Cuando termines, pulsa «Ya terminé»."
                    2 -> "Enfoca la última página que leíste y pulsa «Capturar»."
                    else -> "Escribe en tus palabras qué leíste (al menos 30 palabras)."
                }
            },
            style = MaterialTheme.typography.titleMedium,
        )
        if (progress.isNotEmpty()) Text(progress, Modifier.semantics { liveRegion = LiveRegionMode.Polite })
        error?.let { Text(it, color = MaterialTheme.colorScheme.error) }

        if (cameraNeeded) {
            if (!cameraOk) {
                Text("La cámara solo se usa aquí, en este verificador sin Internet. Nunca graba ni guarda imágenes.")
                Button(onClick = { askCamera.launch(Manifest.permission.CAMERA) }) { Text("Permitir la cámara") }
            } else if (analyzer != null) {
                CameraView(analyzer, front, Modifier.fillMaxWidth().height(360.dp))
                if (pages) Button(onClick = { capture.set(true) }) { Text("Capturar") }
            }
        }
        if (pages && step == 1) Button(onClick = { step = 2 }) { Text("Ya terminé") }
        if (pages && step == 3) {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(summary, { summary = it.take(4000) }, label = { Text("Tu resumen") }, modifier = Modifier.fillMaxWidth())
                Dictation { summary = (summary + " " + it).trim() }
                Button(onClick = {
                    val s = scoreSummary(args.goalId, source, summary)
                    source = ""
                    summary = ""
                    val result = if (args.method == "PAGE_OCR" && s != null) {
                        judgePages(args.goalId, firstPage ?: 0u, lastPage ?: 0u, args.minPages.toUShort(), s)
                    } else s
                    deliver(result)
                }) { Text("Enviar resumen") }
            }
        }
    }
}

private fun granted(ctx: Context, permission: String) =
    ContextCompat.checkSelfPermission(ctx, permission) == PackageManager.PERMISSION_GRANTED

/** Vista previa + análisis en memoria a ~640×480. Se desconecta al salir de la composición. */
@Composable
private fun CameraView(analyzer: ImageAnalysis.Analyzer, front: Boolean, modifier: Modifier) {
    val ctx = LocalContext.current
    val owner = LocalLifecycleOwner.current
    val executor = remember { Executors.newSingleThreadExecutor() }
    val providerFuture = remember { ProcessCameraProvider.getInstance(ctx) }
    DisposableEffect(analyzer) {
        onDispose { runCatching { providerFuture.get().unbindAll() } }
    }
    DisposableEffect(Unit) { onDispose { executor.shutdown() } }
    AndroidView(modifier = modifier, factory = { c ->
        PreviewView(c).also { view ->
            providerFuture.addListener({
                val provider = providerFuture.get()
                val preview = Preview.Builder().build().also { it.surfaceProvider = view.surfaceProvider }
                val small = ResolutionSelector.Builder().setResolutionStrategy(
                    ResolutionStrategy(Size(640, 480), ResolutionStrategy.FALLBACK_RULE_CLOSEST_LOWER_THEN_HIGHER),
                ).build()
                val analysis = ImageAnalysis.Builder()
                    .setResolutionSelector(small)
                    .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                    .setOutputImageFormat(ImageAnalysis.OUTPUT_IMAGE_FORMAT_RGBA_8888)
                    .build()
                    .also { it.setAnalyzer(executor, analyzer) }
                val selector = if (front) CameraSelector.DEFAULT_FRONT_CAMERA else CameraSelector.DEFAULT_BACK_CAMERA
                provider.unbindAll()
                provider.bindToLifecycle(owner, selector, preview, analysis)
            }, ContextCompat.getMainExecutor(c))
        }
    })
}

/**
 * Dictado SOLO con el reconocedor en el dispositivo (Android 12+). Si no existe, no se ofrece:
 * nunca se usa el reconocedor en la nube. El audio no se guarda; el texto solo vive en el campo.
 */
@Composable
private fun Dictation(onText: (String) -> Unit) {
    val ctx = LocalContext.current
    val available = remember {
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && SpeechRecognizer.isOnDeviceRecognitionAvailable(ctx)
    }
    if (!available) return
    var listening by remember { mutableStateOf(false) }
    val recognizer = remember {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) SpeechRecognizer.createOnDeviceSpeechRecognizer(ctx) else null
    }
    DisposableEffect(Unit) { onDispose { recognizer?.destroy() } }
    val askMic = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {}
    OutlinedButton(onClick = {
        if (!granted(ctx, Manifest.permission.RECORD_AUDIO)) {
            askMic.launch(Manifest.permission.RECORD_AUDIO)
            return@OutlinedButton
        }
        recognizer?.setRecognitionListener(object : RecognitionListener {
            override fun onResults(results: Bundle) {
                results.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull()?.let(onText)
                listening = false
            }
            override fun onError(error: Int) { listening = false }
            override fun onReadyForSpeech(params: Bundle?) = Unit
            override fun onBeginningOfSpeech() = Unit
            override fun onRmsChanged(rmsdB: Float) = Unit
            override fun onBufferReceived(buffer: ByteArray?) = Unit
            override fun onEndOfSpeech() = Unit
            override fun onPartialResults(partialResults: Bundle?) = Unit
            override fun onEvent(eventType: Int, params: Bundle?) = Unit
        })
        listening = true
        recognizer?.startListening(
            Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).putExtra(RecognizerIntent.EXTRA_LANGUAGE, "es-ES"),
        )
    }) { Text(if (listening) "Escuchando… (micrófono activo)" else "Dictar (en el dispositivo)") }
}
