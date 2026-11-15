package dev.procastinacioff.verifier

import android.content.Context
import android.graphics.Bitmap
import com.google.android.gms.tasks.Tasks
import com.google.mediapipe.framework.image.BitmapImageBuilder
import com.google.mediapipe.tasks.core.BaseOptions
import com.google.mediapipe.tasks.vision.core.RunningMode
import com.google.mediapipe.tasks.vision.facedetector.FaceDetector
import com.google.mediapipe.tasks.vision.handlandmarker.HandLandmarker
import com.google.mediapipe.tasks.vision.imageclassifier.ImageClassifier
import com.google.mediapipe.tasks.vision.objectdetector.ObjectDetector
import com.google.mediapipe.tasks.vision.poselandmarker.PoseLandmarker
import com.google.mlkit.vision.common.InputImage
import com.google.mlkit.vision.text.TextRecognition
import com.google.mlkit.vision.text.latin.TextRecognizerOptions
import uniffi.procastinacioff_verify.Label
import uniffi.procastinacioff_verify.OcrLine

/**
 * Local detectors. Each model is loaded (and verified) only when a method needs it.
 * Everything runs on the device; this APK has no Internet.
 */
class Detectors(private val ctx: Context) : AutoCloseable {
    private fun base(name: String) = BaseOptions.builder().setModelAssetPath(Models.verified(ctx, name)).build()

    private val face by lazy {
        FaceDetector.createFromOptions(ctx, FaceDetector.FaceDetectorOptions.builder()
            .setBaseOptions(base("blaze_face_short_range.tflite")).setRunningMode(RunningMode.IMAGE).build())
    }
    private val objects by lazy {
        ObjectDetector.createFromOptions(ctx, ObjectDetector.ObjectDetectorOptions.builder()
            .setBaseOptions(base("efficientdet_lite0.tflite")).setRunningMode(RunningMode.IMAGE)
            .setScoreThreshold(0.4f).setCategoryAllowlist(listOf("book")).build())
    }
    private val hands by lazy {
        HandLandmarker.createFromOptions(ctx, HandLandmarker.HandLandmarkerOptions.builder()
            .setBaseOptions(base("hand_landmarker.task")).setRunningMode(RunningMode.IMAGE).setNumHands(1).build())
    }
    private val food by lazy {
        ImageClassifier.createFromOptions(ctx, ImageClassifier.ImageClassifierOptions.builder()
            .setBaseOptions(base("food_classifier.tflite")).setRunningMode(RunningMode.IMAGE).setMaxResults(5).build())
    }
    private val pose by lazy {
        PoseLandmarker.createFromOptions(ctx, PoseLandmarker.PoseLandmarkerOptions.builder()
            .setBaseOptions(base("pose_landmarker_lite.task")).setRunningMode(RunningMode.IMAGE).build())
    }
    // ML Kit OCR with a bundled model (downloads nothing; besides, this APK has no network)
    private val ocr by lazy { TextRecognition.getClient(TextRecognizerOptions.DEFAULT_OPTIONS) }

    private val loaded = mutableListOf<AutoCloseable>()
    private fun <T : AutoCloseable> T.track(): T = also { if (it !in loaded) loaded += it }

    fun hasFace(b: Bitmap) = face.track().detect(BitmapImageBuilder(b).build()).detections().isNotEmpty()
    fun hasBook(b: Bitmap) = objects.track().detect(BitmapImageBuilder(b).build()).detections().isNotEmpty()

    /** 21 points × (x, y, z) or null if there is no hand. */
    fun hand(b: Bitmap): List<Float>? {
        val lm = hands.track().detect(BitmapImageBuilder(b).build()).landmarks().firstOrNull() ?: return null
        return lm.flatMap { listOf(it.x(), it.y(), it.z()) }
    }

    /** 33 points × (x, y, visibility) or null. */
    fun poseLandmarks(b: Bitmap): List<Float>? {
        val lm = pose.track().detect(BitmapImageBuilder(b).build()).landmarks().firstOrNull() ?: return null
        return lm.flatMap { listOf(it.x(), it.y(), it.visibility().orElse(0f)) }
    }

    fun foodLabels(b: Bitmap): List<Label> =
        food.track().classify(BitmapImageBuilder(b).build()).classificationResult().classifications().firstOrNull()
            ?.categories()?.map { Label(name = it.categoryName(), score = (it.score() * 100).toInt().coerceIn(0, 100).toUByte()) }
            ?: emptyList()

    /** Page text (memory only) and its lines with position, for the page number. */
    fun readPage(b: Bitmap): Pair<String, List<OcrLine>> {
        val text = Tasks.await(ocr.process(InputImage.fromBitmap(b, 0))) // analysis thread, not the main one
        val lines = text.textBlocks.flatMap { it.lines }.map {
            OcrLine(text = it.text, top = it.boundingBox?.top ?: 0, bottom = it.boundingBox?.bottom ?: 0)
        }
        return text.text to lines
    }

    override fun close() {
        loaded.forEach { runCatching { it.close() } }
        runCatching { ocr.close() }
    }
}
