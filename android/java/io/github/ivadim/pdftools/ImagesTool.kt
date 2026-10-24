// Images to PDF: photos and image files, one per page.

package io.github.ivadim.pdftools

import android.graphics.Bitmap
import android.net.Uri
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.KeyboardArrowLeft
import androidx.compose.material.icons.automirrored.rounded.KeyboardArrowRight
import androidx.compose.material.icons.automirrored.rounded.RotateRight
import androidx.compose.material.icons.rounded.Delete
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

class ImagesOptions : ToolOptions {
    enum class PageSize { A4, Letter, Fit }

    enum class Orientation { Auto, Portrait, Landscape }

    var size by mutableStateOf(PageSize.A4)
    var orientation by mutableStateOf(Orientation.Auto)
    var margin by mutableIntStateOf(0)

    /** The page an image lands on, as the engine lays it out (in points), and the image's size on it. */
    fun layout(image: ImageItem): Pair<Size, Size> {
        val quarter = image.rotation % 180 == 90
        val (iw, ih) = if (quarter) image.size.height to image.size.width else image.size.width to image.size.height
        val m = margin.toFloat()
        val page = if (size == PageSize.Fit) {
            Size(iw * 0.75f + 2 * m, ih * 0.75f + 2 * m)
        } else {
            val (w, h) = if (size == PageSize.Letter) 612f to 792f else 595.28f to 841.89f
            val landscape = if (orientation == Orientation.Auto) iw > ih else orientation == Orientation.Landscape
            if (landscape) Size(h, w) else Size(w, h)
        }
        val scale = minOf(maxOf(1f, page.width - 2 * m) / maxOf(1f, iw), maxOf(1f, page.height - 2 * m) / maxOf(1f, ih))
        return page to Size(iw * scale, ih * scale)
    }

    @Composable
    override fun Panel(session: ToolSession) {
        Segmented(listOf(PageSize.A4 to "A4", PageSize.Letter to "Letter", PageSize.Fit to "Image size"), size) { size = it }
        if (size != PageSize.Fit) {
            Segmented(listOf(Orientation.Auto to "Automatic", Orientation.Portrait to "Portrait", Orientation.Landscape to "Landscape"), orientation) { orientation = it }
        }
        Segmented(listOf(0 to "No margin", 18 to "Small", 36 to "Large"), margin) { margin = it }
        Note("JPEGs are embedded without re-compression; PNG transparency is kept.")
    }

    override fun problem(session: ToolSession) = if (session.images.isEmpty()) "Add at least one image." else null

    override suspend fun run(session: ToolSession): ToolResult {
        val args = listOf("images", "OUT") + session.images.map { "${it.file.path}:${it.rotation}" } +
            listOf("--size=${size.name.lowercase()}", "--orientation=${orientation.name.lowercase()}", "--margin=$margin")
        return runToFile(session, args, "images.pdf") { json -> "Created a PDF with ${plural(json["pages"].int, "page")}." }
    }

    companion object {
        /** Adds picked images (or the UI tests' files) to the session. */
        suspend fun add(session: ToolSession, sources: List<Any>) {
            session.loading = true
            try {
                for (source in sources) {
                    val name = (source as? File)?.name ?: Workspace.displayName(source as Uri)
                    val item = withContext(Dispatchers.IO) {
                        val data = if (source is File) source.readBytes() else readUri(source as Uri)
                        data?.let { item(it, name) }
                    }
                    if (item != null) session.images += item else session.error = "$name isn’t an image that can be read."
                }
            } finally {
                session.loading = false
            }
        }

        /** Saves picked image data into the workspace (PNG and JPEG as they are). */
        fun item(data: ByteArray, name: String): ImageItem? {
            val png = data.size > 4 && data[0] == 0x89.toByte() && data[1] == 0x50.toByte()
            val jpeg = data.size > 2 && data[0] == 0xff.toByte() && data[1] == 0xd8.toByte()
            val thumbnail = decodeUpright(data, maxSide = 600) ?: return null
            var bytes = data
            var size = displayedSize(data) ?: return null
            if (!png && !jpeg) {
                // HEIC, WebP and others: re-encoded as JPEG, upright.
                val full = decodeUpright(data) ?: return null
                bytes = java.io.ByteArrayOutputStream().also { full.compress(Bitmap.CompressFormat.JPEG, 90, it) }.toByteArray()
                size = Size(full.width.toFloat(), full.height.toFloat())
            }
            val base = name.substringBeforeLast('.').ifBlank { "image" }
            val file = Workspace.write(bytes, "$base.${if (png) "png" else "jpg"}")
            return ImageItem(file, name, size, thumbnail)
        }
    }
}

/** One image on its page, as it will come out. */
@Composable
fun ImageCard(session: ToolSession, options: ImagesOptions, item: ImageItem, index: Int, modifier: Modifier) {
    val (page, image) = options.layout(item)
    val thumbnail = remember(item.thumbnail) { item.thumbnail.asImageBitmap() }
    val controls = listOf(
        CardControl(Icons.AutoMirrored.Rounded.KeyboardArrowLeft, "Move earlier", disabled = index == 0) {
            session.images[index] = session.images[index - 1].also { session.images[index - 1] = session.images[index] }
        },
        CardControl(Icons.AutoMirrored.Rounded.KeyboardArrowRight, "Move later", disabled = index == session.images.size - 1) {
            session.images[index] = session.images[index + 1].also { session.images[index + 1] = session.images[index] }
        },
        CardControl(Icons.AutoMirrored.Rounded.RotateRight, "Rotate") { session.images[index] = item.copy(rotation = (item.rotation + 90) % 360) },
        CardControl(Icons.Rounded.Delete, "Remove") { session.images.removeAt(index) },
    )
    Column(modifier, verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Box(Modifier.fillMaxWidth().aspectRatio(5f / 6f), contentAlignment = Alignment.Center) {
            val ratio = page.width / page.height
            BoxWithConstraints(Modifier.aspectRatio(ratio, matchHeightConstraintsFirst = ratio < 5f / 6f).shadow(2.dp).background(Color.White)) {
                val scale = constraints.maxWidth / page.width
                val density = LocalDensity.current
                val quarter = item.rotation % 180 == 90
                // The image is drawn unrotated, then turned into place.
                val (w, h) = if (quarter) image.height to image.width else image.width to image.height
                Image(
                    thumbnail,
                    item.name,
                    Modifier
                        .align(Alignment.Center)
                        .size(with(density) { (w * scale).toDp() }, with(density) { (h * scale).toDp() })
                        .rotate(item.rotation.toFloat()),
                    contentScale = ContentScale.FillBounds,
                )
            }
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(item.name, Modifier.weight(1f), style = MaterialTheme.typography.labelMedium, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
            for (control in controls) {
                IconButton(control.action, Modifier.size(30.dp), enabled = !control.disabled) { Icon(control.icon, control.label, Modifier.size(20.dp)) }
            }
        }
    }
}
