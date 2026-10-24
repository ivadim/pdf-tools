// Stamps and signatures as PNG pictures: typed text drawn with the system's
// fonts, drawings and chosen images.

package io.github.ivadim.pdftools

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.ImageDecoder
import android.graphics.Paint
import android.graphics.Typeface
import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Folder
import androidx.compose.material.icons.rounded.Photo
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import java.io.ByteArrayOutputStream
import kotlin.math.ceil

class Picture(
    /** PNG or JPEG bytes, as the engine reads them. */
    val data: ByteArray,
    val bitmap: Bitmap,
) {
    /** In pixels. */
    val size get() = Size(bitmap.width.toFloat(), bitmap.height.toFloat())

    companion object {
        /** A chosen image: PNG and upright JPEG bytes are kept as they are, anything else becomes PNG. */
        fun fromImageData(data: ByteArray): Picture? {
            val png = data.startsWith(0x89, 0x50, 0x4e, 0x47)
            val jpeg = data.startsWith(0xff, 0xd8)
            val upright = decodeUpright(data) ?: return null
            if (png) return Picture(data, upright)
            if (jpeg && exifRotation(data) == 0) return Picture(data, upright)
            return fromBitmap(upright)
        }

        fun fromBitmap(bitmap: Bitmap): Picture {
            val out = ByteArrayOutputStream()
            bitmap.compress(Bitmap.CompressFormat.PNG, 100, out)
            return Picture(out.toByteArray(), bitmap)
        }

        /** Text in `family`, cropped to what's drawn. */
        fun text(text: String, family: Font, bold: Boolean, color: Color, size: Float = 160f): Picture? {
            val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
                typeface = family.typeface(bold)
                textSize = size
                this.color = color.toArgb()
            }
            val lines = text.split('\n')
            val pad = ceil(size * 0.15f)
            val lineHeight = ceil(size * 1.2f)
            val width = ceil(lines.maxOf { paint.measureText(it) }) + pad * 2
            val height = lineHeight * lines.size + pad * 2
            val bitmap = Bitmap.createBitmap(width.toInt().coerceAtLeast(1), height.toInt().coerceAtLeast(1), Bitmap.Config.ARGB_8888)
            val canvas = Canvas(bitmap)
            val metrics = paint.fontMetrics
            lines.forEachIndexed { i, line ->
                val centre = pad + lineHeight * (i + 0.5f)
                canvas.drawText(line, pad, centre - (metrics.ascent + metrics.descent) / 2, paint)
            }
            return trim(bitmap, (pad / 2).toInt())?.let(::fromBitmap)
        }
    }
}

/** Font families to draw stamps and signatures with: the ones every Android device has. */
enum class Font(val label: String, private val family: String, private val italic: Boolean = false) {
    System("System", "sans-serif"),
    Serif("Serif", "serif"),
    SerifItalic("Serif italic", "serif", italic = true),
    Condensed("Condensed", "sans-serif-condensed"),
    Mono("Monospace", "monospace"),
    Cursive("Cursive", "cursive"),
    Casual("Casual", "casual");

    fun typeface(bold: Boolean): Typeface {
        val style = when {
            bold && italic -> Typeface.BOLD_ITALIC
            bold -> Typeface.BOLD
            italic -> Typeface.ITALIC
            else -> Typeface.NORMAL
        }
        return Typeface.create(family, style)
    }

    /** For previews in the font menu. */
    val compose: FontFamily get() = FontFamily(typeface(false))
}

private fun ByteArray.startsWith(vararg bytes: Int) = size >= bytes.size && bytes.indices.all { this[it] == bytes[it].toByte() }

/** The image with its camera rotation applied. */
fun decodeUpright(data: ByteArray, maxSide: Int = 0): Bitmap? = runCatching {
    ImageDecoder.decodeBitmap(ImageDecoder.createSource(java.nio.ByteBuffer.wrap(data))) { decoder, info, _ ->
        decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
        if (maxSide > 0) {
            val longest = maxOf(info.size.width, info.size.height)
            if (longest > maxSide) {
                val scale = maxSide.toFloat() / longest
                decoder.setTargetSize((info.size.width * scale).toInt().coerceAtLeast(1), (info.size.height * scale).toInt().coerceAtLeast(1))
            }
        }
    }
}.getOrNull()

/** A JPEG's EXIF rotation in degrees (0 for anything else). */
fun exifRotation(data: ByteArray): Int = runCatching {
    when (android.media.ExifInterface(data.inputStream()).getAttributeInt(android.media.ExifInterface.TAG_ORIENTATION, 1)) {
        3, 4 -> 180
        5, 6 -> 90
        7, 8 -> 270
        else -> 0
    }
}.getOrDefault(0)

/** The image's size as displayed (camera rotation applied), without decoding it. */
fun displayedSize(data: ByteArray): Size? {
    val options = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeByteArray(data, 0, data.size, options)
    if (options.outWidth <= 0) return decodeUpright(data)?.let { Size(it.width.toFloat(), it.height.toFloat()) }
    val (w, h) = options.outWidth.toFloat() to options.outHeight.toFloat()
    return if (exifRotation(data) % 180 == 90) Size(h, w) else Size(w, h)
}

/** Crops an image to what is drawn on it (alpha above a few percent), keeping `pad` pixels around it. */
fun trim(image: Bitmap, pad: Int): Bitmap? {
    val (width, height) = image.width to image.height
    val pixels = IntArray(width * height)
    image.getPixels(pixels, 0, width, 0, 0, width, height)
    var left = width
    var top = height
    var right = -1
    var bottom = -1
    for (y in 0 until height) {
        for (x in 0 until width) {
            if ((pixels[y * width + x] ushr 24) > 8) {
                if (x < left) left = x
                if (x > right) right = x
                if (y < top) top = y
                if (y > bottom) bottom = y
            }
        }
    }
    if (right < 0) return null
    val x = maxOf(0, left - pad)
    val y = maxOf(0, top - pad)
    return Bitmap.createBitmap(image, x, y, minOf(width - 1, right + pad) - x + 1, minOf(height - 1, bottom + pad) - y + 1)
}

/** Reads a picked image's bytes. */
fun readUri(uri: Uri): ByteArray? = runCatching { App.context.contentResolver.openInputStream(uri)?.use { it.readBytes() } }.getOrNull()

/** Picks an image from Photos or Files. */
@Composable
fun ImageSourceButtons(onPick: (ByteArray) -> Unit) {
    fun pick(uri: Uri?) {
        uri?.let(::readUri)?.let(onPick)
    }
    val photo = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { pick(it) }
    val file = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { pick(it) }
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedButton({
            val test = TestHooks.files?.firstOrNull { it.extension.lowercase() in setOf("png", "jpg", "jpeg") }
            if (test != null) onPick(test.readBytes()) else photo.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly))
        }, Modifier.testTag("image.photos")) {
            Icon(Icons.Rounded.Photo, null, Modifier.size(18.dp))
            Spacer(Modifier.width(8.dp))
            Text("Photos")
        }
        OutlinedButton({ file.launch(arrayOf("image/png", "image/jpeg", "image/*")) }) {
            Icon(Icons.Rounded.Folder, null, Modifier.size(18.dp))
            Spacer(Modifier.width(8.dp))
            Text("Files")
        }
    }
}
