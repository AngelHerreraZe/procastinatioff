package dev.procastinacioff.verifier

import android.graphics.Bitmap
import android.graphics.Matrix
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy

/**
 * 🔒 Entrega cuadros como Bitmap en memoria (ya girados), como máximo uno cada `everyMs`.
 * Los cuadros intermedios se cierran sin procesar. Nunca se escribe nada a disco y cada
 * Bitmap se libera en cuanto se procesa.
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
        val raw = image.toBitmap() // copia en RAM
        val rotation = image.imageInfo.rotationDegrees
        image.close()
        val upright = if (rotation == 0) raw else
            Bitmap.createBitmap(raw, 0, 0, raw.width, raw.height, Matrix().apply { postRotate(rotation.toFloat()) }, true)
        try {
            onFrame(upright, now)
        } finally {
            upright.recycle() // 🔒 se libera inmediatamente
            if (upright !== raw) raw.recycle()
        }
    }
}
