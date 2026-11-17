package dev.procastinatioff.app.ui

import androidx.compose.ui.res.stringResource
import dev.procastinatioff.app.R
import android.Manifest
import android.content.ClipData
import android.content.ClipboardManager
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.Color
import android.os.Build
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.google.zxing.BarcodeFormat
import com.google.zxing.BinaryBitmap
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.common.HybridBinarizer
import com.google.zxing.qrcode.QRCodeReader
import com.google.zxing.qrcode.QRCodeWriter
import dev.procastinatioff.app.core.CoreHolder
import dev.procastinatioff.app.core.userMessage
import uniffi.procastinatioff_core.Core
import uniffi.procastinatioff_core.Role
import java.util.concurrent.Executors
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

fun qrBitmap(text: String, size: Int = 640): Bitmap {
    val m = QRCodeWriter().encode(text, BarcodeFormat.QR_CODE, size, size)
    return Bitmap.createBitmap(size, size, Bitmap.Config.RGB_565).apply {
        for (x in 0 until size) for (y in 0 until size) setPixel(x, y, if (m[x, y]) Color.BLACK else Color.WHITE)
    }
}

/** The code both devices must show, large and spaced (tabular figures: digits do not shift). */
@Composable
private fun CodeToCompare(code: String) {
    Surface(Modifier.fillMaxWidth(), shape = MaterialTheme.shapes.medium, color = MaterialTheme.colorScheme.primaryContainer) {
        Text(
            code, Modifier.padding(vertical = 24.dp),
            style = MaterialTheme.typography.displayMedium.copy(fontFeatureSettings = "tnum", letterSpacing = 6.sp),
            color = MaterialTheme.colorScheme.onPrimaryContainer, textAlign = TextAlign.Center,
        )
    }
}

/** (A) Shows the QR and the 6-digit code to compare. */
@Composable
fun InviteScreen(box: Core? = null, role: Role? = null, onDone: () -> Unit) {
    val ctx = LocalContext.current
    val core = remember { box ?: CoreHolder.get(ctx) }
    var offer by remember { mutableStateOf<String?>(null) }
    var qr by remember { mutableStateOf<Bitmap?>(null) }
    var code by remember { mutableStateOf<String?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    val cancel = { core.pairingCancel(); onDone() }
    BackHandler(onBack = cancel) // leaving by system back also cancels the pairing

    LaunchedEffect(Unit) {
        try {
            val o = withContext(Dispatchers.IO) { core.pairingOffer() }
            offer = o
            qr = withContext(Dispatchers.Default) { qrBitmap(o) }
            while (code == null) { // waits for the other device to scan
                delay(2_000)
                code = withContext(Dispatchers.IO) { core.pairingPoll() }
            }
        } catch (e: Exception) {
            error = e.userMessage(ctx)
        }
    }

    val shown = code
    if (shown == null) {
        Screen(title = stringResource(R.string.pair_title), lead = stringResource(R.string.pair_instructions), onBack = cancel) {
            AppCard {
                val bitmap = qr
                if (bitmap == null) LoadingState()
                else Image(
                    bitmap.asImageBitmap(), contentDescription = stringResource(R.string.pair_qr_label),
                    modifier = Modifier.align(Alignment.CenterHorizontally).size(240.dp),
                )
                offer?.let { o ->
                    TextButton(
                        onClick = { ctx.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText(ctx.getString(R.string.pair_code), o)) },
                        modifier = Modifier.align(Alignment.CenterHorizontally),
                    ) { Text(stringResource(R.string.pair_copy_for_pc)) }
                }
            }
            Footnote(stringResource(R.string.pair_expires), PcoIcons.ShieldLock)
            ErrorText(error)
        }
    } else {
        Screen(
            title = stringResource(R.string.pair_match_question),
            onBack = cancel,
            bottom = {
                ErrorText(error)
                PrimaryButton(stringResource(R.string.pair_match_yes)) {
                    // On THIS core (profile, contact channel or a minor's profile)
                    error = withContext(Dispatchers.IO) {
                        runCatching { core.pairingConfirm(CoreHolder.clock(), role) }.exceptionOrNull()?.userMessage(ctx)
                    }
                    if (error == null) onDone()
                }
                SecondaryButton(stringResource(R.string.pair_match_no)) { cancel() }
            },
        ) { CodeToCompare(shown) }
    }
}

/** (B) Scans the other device's QR (or pastes its code) and waits for its confirmation. */
@Composable
fun JoinScreen(box: Core? = null, onBack: () -> Unit, onDone: () -> Unit) {
    val ctx = LocalContext.current
    val core = remember { box ?: CoreHolder.get(ctx) }
    // Own profile: ready when the configuration arrives. Contact channel: when the members arrive.
    val ready: () -> Boolean = { if (box == null) core.status(CoreHolder.clock()) != null else core.members().isNotEmpty() }
    val scope = rememberCoroutineScope()
    var code by remember { mutableStateOf<String?>(null) }
    var pasted by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    var cameraOk by remember {
        mutableStateOf(ContextCompat.checkSelfPermission(ctx, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED)
    }
    val askCamera = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { cameraOk = it }

    fun join(qr: String) = scope.launch {
        try {
            code = withContext(Dispatchers.IO) { core.pairingJoin(qr, Build.MODEL.take(64)) }
            // Wait for the other device to confirm and send us the history
            while (!withContext(Dispatchers.IO) { ready() }) {
                delay(3_000)
                runCatching { withContext(Dispatchers.IO) { CoreHolder.syncNow(core) } }
            }
            onDone()
        } catch (e: Exception) {
            error = e.userMessage(ctx)
        }
    }

    val shown = code
    if (shown == null) {
        Screen(title = stringResource(R.string.join_scan), onBack = onBack) {
            if (cameraOk) {
                QrScanner(Modifier.fillMaxWidth().height(320.dp).clip(MaterialTheme.shapes.medium)) { join(it) }
            } else {
                SecondaryButton(stringResource(R.string.join_use_camera)) { askCamera.launch(Manifest.permission.CAMERA) }
            }
            Footnote(stringResource(R.string.join_camera_note), PcoIcons.Camera)
            OutlinedTextField(pasted, { pasted = it }, label = { Text(stringResource(R.string.join_paste)) }, modifier = Modifier.fillMaxWidth())
            SecondaryButton(stringResource(R.string.join_with_code), enabled = pasted.isNotBlank()) { join(pasted.trim()) }
            ErrorText(error)
        }
    } else {
        Screen(title = stringResource(R.string.join_check_code), onBack = onBack) {
            CodeToCompare(shown)
            Footnote(stringResource(R.string.join_confirm_there), PcoIcons.Smartphone)
            LoadingState()
            ErrorText(error)
        }
    }
}

/** SECURITY: ImageAnalysis only: each frame is analyzed in memory and discarded (the buffer is overwritten with zeros). */
private class QrAnalyzer(private val onResult: (String) -> Unit) : ImageAnalysis.Analyzer {
    private val reader = QRCodeReader()
    @Volatile private var done = false

    override fun analyze(image: ImageProxy) {
        if (done) { image.close(); return }
        val plane = image.planes[0]
        val data = ByteArray(plane.buffer.remaining()).also { plane.buffer.get(it) }
        try {
            val source = PlanarYUVLuminanceSource(data, plane.rowStride, image.height, 0, 0, image.width, image.height, false)
            val text = reader.decode(BinaryBitmap(HybridBinarizer(source))).text
            done = true
            onResult(text)
        } catch (_: Exception) {
            // No QR in this frame: keep going
        } finally {
            data.fill(0)
            image.close()
        }
    }
}

@Composable
fun QrScanner(modifier: Modifier = Modifier, onResult: (String) -> Unit) {
    val ctx = LocalContext.current
    val owner = LocalLifecycleOwner.current
    val executor = remember { Executors.newSingleThreadExecutor() }
    DisposableEffect(Unit) { onDispose { executor.shutdown() } }
    AndroidView(modifier = modifier, factory = { c ->
        val view = PreviewView(c)
        val providerFuture = ProcessCameraProvider.getInstance(c)
        providerFuture.addListener({
            val provider = providerFuture.get()
            val preview = Preview.Builder().build().also { it.surfaceProvider = view.surfaceProvider }
            val analysis = ImageAnalysis.Builder()
                .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                .build()
                .also { it.setAnalyzer(executor, QrAnalyzer { text -> ContextCompat.getMainExecutor(ctx).execute { onResult(text) } }) }
            provider.unbindAll()
            provider.bindToLifecycle(owner, CameraSelector.DEFAULT_BACK_CAMERA, preview, analysis)
        }, ContextCompat.getMainExecutor(c))
        view
    })
}
