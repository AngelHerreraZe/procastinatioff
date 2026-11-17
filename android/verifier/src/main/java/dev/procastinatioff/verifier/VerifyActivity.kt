package dev.procastinatioff.verifier

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
import androidx.compose.material3.Surface
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
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
import uniffi.procastinatioff_verify.Exercise
import uniffi.procastinatioff_verify.ReadingTracker
import uniffi.procastinatioff_verify.RepCounter
import uniffi.procastinatioff_verify.VMethod
import uniffi.procastinatioff_verify.VerificationResult
import uniffi.procastinatioff_verify.countFingers
import uniffi.procastinatioff_verify.dhash
import uniffi.procastinatioff_verify.isDuplicate
import uniffi.procastinatioff_verify.judgeFood
import uniffi.procastinatioff_verify.judgePages
import uniffi.procastinatioff_verify.newFingerChallenge
import uniffi.procastinatioff_verify.pageNumber
import uniffi.procastinatioff_verify.salt
import uniffi.procastinatioff_verify.scoreSummary

/** Parameters sent by the main app (nothing else enters the verifier). */
data class Args(val goalId: String, val method: String, val targetMinutes: Int, val targetReps: Int, val minPages: Int)

class VerifyActivity : ComponentActivity() {
    private lateinit var detectors: Detectors
    private val done = AtomicBoolean(false)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // SECURITY: No screenshots nor preview in recents; screen kept on while verifying
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
        val night = intent.getBooleanExtra("night", false)
        setContent {
            MaterialTheme(colorScheme = if (night) Nocturno else Calma) {
                Surface(color = MaterialTheme.colorScheme.background) { VerifyScreen(args, detectors, ::deliver) }
            }
        }
    }

    /** Returns ONLY VerificationResult's 5 fields to the main app. Only once. */
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

// The app's two looks (DESIGN.md), chosen by the app and passed as the "night" extra.
// ponytail: copied from the app's ui/Common.kt (a separate APK without a shared module); keep both equal.
private val Calma = lightColorScheme(
    primary = Color(0xFF087C70), onPrimary = Color.White,
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

/** Reduces to 9×8 gray, in memory (for the perceptual hash). */
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
    // Exception messages are for developers: the user sees a translated, fixed text.
    val report: (Throwable) -> Unit = { e ->
        main.execute { error = ctx.getString(if (e is Models.MissingModel) R.string.models_missing else R.string.analyze_failed) }
    }

    // Flow state (in memory only)
    val tracker = remember { ReadingTracker() }
    val counter = remember { RepCounter(Exercise.SQUAT) }
    val challenge = remember { newFingerChallenge() }
    var step by remember { mutableStateOf(if (args.method == "SUMMARY") 2 else 0) } // pages: 0 start · 1 reading · 2 end · 3 summary
    var firstPage by remember { mutableStateOf<UShort?>(null) }
    var lastPage by remember { mutableStateOf<UShort?>(null) }
    var source by remember { mutableStateOf("") } // OCR text: RAM only, discarded when done
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
                    main.execute { progress = ctx.getString(R.string.progress_minutes, m.toInt(), args.targetMinutes) }
                    if (m.toInt() >= args.targetMinutes) deliver(tracker.finish(args.goalId, args.targetMinutes.toUShort()))
                }.onFailure(report)
            }
            "POSE_REPS" -> FrameSource(100) { bmp, at ->
                runCatching {
                    val lm = detectors.poseLandmarks(bmp) ?: return@FrameSource
                    val reps = counter.feed(lm, at)
                    main.execute { progress = ctx.resources.getQuantityString(R.plurals.progress_reps, args.targetReps, reps.toInt(), args.targetReps) }
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
                    HashStore.add(ctx, salted) // only the salted hash, 7 days
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
                                error = ctx.getString(R.string.page_number_missing)
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
        // Fixed banner: the user always knows the camera is on and can stop it
        Row(
            Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.primaryContainer).padding(12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text(
                stringResource(if (cameraNeeded) R.string.camera_on_banner else R.string.camera_off_banner),
                fontWeight = FontWeight.SemiBold,
                modifier = Modifier.weight(1f),
            )
            OutlinedButton(onClick = {
                // Reading: what was accumulated counts (approved at 85 %); everything else, no result
                deliver(if (args.method == "READING_PRESENCE") tracker.finish(args.goalId, args.targetMinutes.toUShort()) else null)
            }) { Text(stringResource(R.string.stop)) }
        }

        Text(
            when (args.method) {
                "READING_PRESENCE" -> stringResource(R.string.instr_reading)
                "POSE_REPS" -> pluralStringResource(R.plurals.instr_squats, args.targetReps, args.targetReps)
                "FOOD_PHOTO" -> pluralStringResource(R.plurals.instr_fingers, challenge.toInt(), challenge.toInt())
                else -> when (step) {
                    0 -> stringResource(R.string.instr_first_page)
                    1 -> stringResource(R.string.instr_read)
                    2 -> stringResource(R.string.instr_last_page)
                    else -> stringResource(R.string.instr_summary)
                }
            },
            style = MaterialTheme.typography.titleMedium,
        )
        if (progress.isNotEmpty()) Text(progress, Modifier.semantics { liveRegion = LiveRegionMode.Polite })
        error?.let { Text(it, color = MaterialTheme.colorScheme.error) }

        if (cameraNeeded) {
            if (!cameraOk) {
                Text(stringResource(R.string.camera_explanation))
                Button(onClick = { askCamera.launch(Manifest.permission.CAMERA) }) { Text(stringResource(R.string.allow_camera)) }
            } else if (analyzer != null) {
                CameraView(analyzer, front, Modifier.fillMaxWidth().height(360.dp))
                if (pages) Button(onClick = { capture.set(true) }) { Text(stringResource(R.string.capture)) }
            }
        }
        if (pages && step == 1) Button(onClick = { step = 2 }) { Text(stringResource(R.string.done_reading)) }
        if (pages && step == 3) {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(summary, { summary = it.take(4000) }, label = { Text(stringResource(R.string.your_summary)) }, modifier = Modifier.fillMaxWidth())
                Dictation { summary = (summary + " " + it).trim() }
                Button(onClick = {
                    val s = scoreSummary(args.goalId, source, summary)
                    source = ""
                    summary = ""
                    val result = if (args.method == "PAGE_OCR" && s != null) {
                        judgePages(args.goalId, firstPage ?: 0u, lastPage ?: 0u, args.minPages.toUShort(), s)
                    } else s
                    deliver(result)
                }) { Text(stringResource(R.string.send_summary)) }
            }
        }
    }
}

private fun granted(ctx: Context, permission: String) =
    ContextCompat.checkSelfPermission(ctx, permission) == PackageManager.PERMISSION_GRANTED

/** Preview + in-memory analysis at ~640×480. Unbinds when leaving the composition. */
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
 * Dictation ONLY with the on-device recognizer (Android 12+), in the user's language. If it does not exist,
 * it is not offered: the cloud recognizer is never used. Audio is not saved; the text only lives in the field.
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
            Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).putExtra(RecognizerIntent.EXTRA_LANGUAGE, java.util.Locale.getDefault().toLanguageTag()),
        )
    }) { Text(stringResource(if (listening) R.string.listening else R.string.dictate)) }
}
