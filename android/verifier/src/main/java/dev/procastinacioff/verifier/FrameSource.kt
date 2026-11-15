package dev.procastinacioff.verifier

import android.graphics.Bitmap
import android.graphics.Matrix
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy

/**
 * SECURITY: Delivers frames as in-memory Bitmaps (already rotated), at most one every `everyMs`.
 * Intermediate frames are closed unprocessed. Nothing is ever written to disk and each
 * Bitmap is released as soon as it is processed.
 */
class FrameSource(private val everyMs: Long, private val onFrame: (Bitmap, Long) -> Unit) : ImageAnalysis.Analyzer {
    private var last = 0L

    override fun analyze(image: ImageProxy) {
        val now = System.currentTimeMillis()
        if (now - last < everyMs) {
            image.close()
            return
        }
        last = now
        val raw = image.toBitmap() // copy in RAM
        val rotation = image.imageInfo.rotationDegrees
        image.close()
        val upright = if (rotation == 0) raw else
            Bitmap.createBitmap(raw, 0, 0, raw.width, raw.height, Matrix().apply { postRotate(rotation.toFloat()) }, true)
        try {
            onFrame(upright, now)
        } finally {
            upright.recycle() // SECURITY: released immediately
            if (upright !== raw) raw.recycle()
        }
    }
}
