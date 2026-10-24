// Signature: draw, type or upload a signature, then tap pages to place it and
// drag it into position.

package io.github.ivadim.pdftools

import android.graphics.Bitmap
import android.graphics.Paint
import android.graphics.Path
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Cancel
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import java.util.Locale
import java.util.UUID
import kotlin.math.roundToInt

class SignatureOptions : ToolOptions {
    enum class Source { Draw, Type, Upload }

    data class Placement(
        val page: Int,
        /** The centre, as fractions of the page. */
        val cx: Float,
        val cy: Float,
        val id: String = UUID.randomUUID().toString(),
    )

    companion object {
        val fonts = listOf(Font.Cursive, Font.Casual, Font.SerifItalic, Font.System)
    }

    var source by mutableStateOf(Source.Draw)
    /** Strokes on the pad, in pad pixels. */
    val strokes = mutableStateListOf<List<Offset>>()
    var padSize by mutableStateOf(Size(1f, 1f))
    var ink by mutableStateOf(Color(0xff1a237e))
    var name by mutableStateOf("")
    var font by mutableStateOf(Font.Cursive)
    var upload by mutableStateOf<Picture?>(null)
    var width by mutableFloatStateOf(0.3f)
    val placed = mutableStateListOf<Placement>()

    private var cachedKey = ""
    private var cachedPicture: Picture? = null

    val picture: Picture?
        get() = when (source) {
            Source.Upload -> upload
            Source.Type -> if (name.isBlank()) null else cached("type|$name|$font|${ink.hex}") { Picture.text(name, font, bold = false, color = ink, size = 140f) }
            Source.Draw -> if (strokes.isEmpty()) null else cached("draw|${strokes.size}|${strokes.lastOrNull()?.size}|${ink.hex}") { drawn() }
        }

    private fun cached(key: String, make: () -> Picture?): Picture? {
        if (key != cachedKey) {
            cachedKey = key
            cachedPicture = make()
        }
        return cachedPicture
    }

    /** The strokes at twice the pad's size, cropped to what's drawn. */
    private fun drawn(): Picture? {
        val scale = 2f
        val bitmap = Bitmap.createBitmap((padSize.width * scale).roundToInt().coerceAtLeast(1), (padSize.height * scale).roundToInt().coerceAtLeast(1), Bitmap.Config.ARGB_8888)
        val canvas = android.graphics.Canvas(bitmap)
        val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            color = ink.toArgb()
            style = Paint.Style.STROKE
            strokeWidth = STROKE_DP * App.context.resources.displayMetrics.density * scale
            strokeCap = Paint.Cap.ROUND
            strokeJoin = Paint.Join.ROUND
        }
        for (stroke in strokes) canvas.drawPath(androidPath(stroke, scale), paint)
        return trim(bitmap, 12)?.let(Picture::fromBitmap)
    }

    /** A placement's box, as fractions of the page: x, y, width, height. */
    fun box(placement: Placement, points: Size): FloatArray {
        val w = width
        val h = picture?.let { w * it.size.height * points.width / (it.size.width * points.height) } ?: (w / 3)
        val x = (placement.cx - w / 2).coerceIn(0f, 1 - w)
        val y = (placement.cy - h / 2).coerceIn(0f, maxOf(0f, 1 - h))
        return floatArrayOf(x, y, w, h)
    }

    override fun reset(session: ToolSession) {
        // Placements belong to the file they were made on.
        placed.clear()
    }

    @Composable
    override fun Panel(session: ToolSession) {
        Segmented(Source.entries.map { it to it.name }, source) { source = it }
        when (source) {
            Source.Draw -> {
                SignaturePad(this)
                OutlinedButton({ strokes.clear() }, enabled = strokes.isNotEmpty()) { Text("Clear") }
            }
            Source.Type -> {
                TextInput("Your name", name, { name = it }, tag = "signature.name")
                DropdownField("Style", fonts.map { it to it.label }, font, { font = it }, font = { it.compose })
            }
            Source.Upload -> {
                upload?.let { Image(it.bitmap.asImageBitmap(), null, Modifier.heightIn(max = 80.dp)) }
                ImageSourceButtons { data -> Picture.fromImageData(data)?.let { upload = it } }
            }
        }
        if (source != Source.Upload) ColorRow("Ink", ink) { ink = it }
        SliderField("Size", width, 0.08f..0.8f, 0.02f, ::percent) { width = it }
        Note("Tap a page to place the signature; drag it to move it.")
    }

    override fun card(session: ToolSession, item: PageItem, index: Int): CardDecor {
        val picture = picture ?: return CardDecor()
        return CardDecor(
            overlay = { points -> PlacedSignatures(this, item.page, points, picture) },
            onTap = { point -> placed += Placement(item.page, point.x, point.y) },
        )
    }

    override fun problem(session: ToolSession): String? {
        if (picture == null) {
            return when (source) {
                Source.Draw -> "Draw your signature."
                Source.Type -> "Type your name."
                Source.Upload -> "Choose an image."
            }
        }
        return if (placed.isEmpty()) "Tap a page to place the signature." else null
    }

    override suspend fun run(session: ToolSession): ToolResult {
        val pdf = session.pdf ?: throw EngineException("Choose a PDF.")
        val picture = picture ?: throw EngineException("Draw your signature.")
        val image = Workspace.write(picture.data, "signature.png")
        val places = placed.map { placement ->
            val item = session.items.firstOrNull { it.page == placement.page }
            val box = box(placement, item?.let(session::points) ?: Size(612f, 792f))
            "${placement.page}:" + box.joinToString(":") { String.format(Locale.ROOT, "%.5f", it) }
        }
        val args = listOf("signature", pdf.file.path, "OUT", "--image=${image.path}", "--place=${places.joinToString(",")}")
        return runToFile(session, args, "${pdf.baseName}-signed.pdf") { json -> "Signed ${plural(json["pages"].int, "page")}." }
    }
}

/** The pen's width on the pad. */
private const val STROKE_DP = 3f

private fun androidPath(points: List<Offset>, scale: Float) = Path().apply {
    points.firstOrNull()?.let { moveTo(it.x * scale, it.y * scale) }
    if (points.size == 1) lineTo(points[0].x * scale + 0.1f, points[0].y * scale)
    for (point in points.drop(1)) lineTo(point.x * scale, point.y * scale)
}

/** A white pad to sign on with a finger or stylus. */
@Composable
private fun SignaturePad(options: SignatureOptions) {
    var current by remember { mutableStateOf<List<Offset>>(emptyList()) }
    val density = LocalDensity.current
    Canvas(
        Modifier
            .fillMaxWidth()
            .height(160.dp)
            .clip(RoundedCornerShape(10.dp))
            .background(Color.White)
            .border(1.dp, MaterialTheme.colorScheme.outlineVariant, RoundedCornerShape(10.dp))
            .testTag("signature.pad")
            // Ink starts where the finger touches down (no drag threshold), so dots and short strokes count.
            .pointerInput(Unit) {
                awaitEachGesture {
                    val down = awaitFirstDown()
                    down.consume()
                    current = listOf(down.position)
                    while (true) {
                        val change = awaitPointerEvent().changes.firstOrNull { it.id == down.id } ?: break
                        if (!change.pressed) break
                        change.consume()
                        current = current + change.position
                    }
                    options.padSize = Size(size.width.toFloat(), size.height.toFloat())
                    options.strokes += current
                    current = emptyList()
                }
            },
    ) {
        val stroke = Stroke(width = with(density) { STROKE_DP.dp.toPx() }, cap = StrokeCap.Round, join = StrokeJoin.Round)
        for (points in options.strokes + listOf(current)) {
            if (points.isEmpty()) continue
            if (points.size == 1) {
                drawCircle(options.ink, stroke.width / 2, points[0])
                continue
            }
            val path = androidx.compose.ui.graphics.Path().apply {
                moveTo(points[0].x, points[0].y)
                for (p in points.drop(1)) lineTo(p.x, p.y)
            }
            drawPath(path, options.ink, style = stroke)
        }
    }
}

/** The signatures on one page; drag to move, × to remove. */
@Composable
private fun PlacedSignatures(options: SignatureOptions, page: Int, points: Size, picture: Picture) {
    val image = remember(picture) { picture.bitmap.asImageBitmap() }
    val primary = MaterialTheme.colorScheme.primary
    BoxWithConstraints(Modifier.fillMaxSize()) {
        val (width, height) = constraints.maxWidth.toFloat() to constraints.maxHeight.toFloat()
        val density = LocalDensity.current
        for (placement in options.placed.filter { it.page == page }) {
            val (x, y, w, h) = options.box(placement, points).toList()
            Box(
                Modifier
                    .offset { IntOffset((x * width).roundToInt(), (y * height).roundToInt()) }
                    .size(with(density) { (w * width).toDp() }, with(density) { (h * height).toDp() })
                    .pointerInput(placement.id) {
                        detectDragGestures(
                            onDrag = { change, delta ->
                                change.consume()
                                val index = options.placed.indexOfFirst { it.id == placement.id }
                                if (index >= 0) {
                                    val p = options.placed[index]
                                    options.placed[index] = p.copy(cx = p.cx + delta.x / width, cy = p.cy + delta.y / height)
                                }
                            },
                            onDragEnd = {
                                // Keep the stored centre where the box was drawn.
                                val index = options.placed.indexOfFirst { it.id == placement.id }
                                if (index >= 0) {
                                    val (bx, by, bw, bh) = options.box(options.placed[index], points).toList()
                                    options.placed[index] = options.placed[index].copy(cx = bx + bw / 2, cy = by + bh / 2)
                                }
                            },
                        )
                    },
            ) {
                Image(image, "Signature", Modifier.fillMaxSize())
                Canvas(Modifier.fillMaxSize()) {
                    drawRect(primary, style = Stroke(width = 1.dp.toPx(), pathEffect = PathEffect.dashPathEffect(floatArrayOf(8f, 6f))))
                }
                Icon(
                    Icons.Rounded.Cancel,
                    "Remove signature",
                    tint = primary,
                    modifier = Modifier
                        .align(Alignment.TopEnd)
                        .offset(10.dp, (-10).dp)
                        .size(22.dp)
                        .background(Color.White, CircleShape)
                        .pointerInput(placement.id) {
                            detectTapGestures(onTap = { options.placed.removeAll { it.id == placement.id } })
                        },
                )
            }
        }
    }
}
