// Page numbers and Watermark: something drawn on chosen pages, previewed on
// each page card.

package io.github.ivadim.pdftools

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.List
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.roundToInt
import kotlin.math.sin

class NumbersOptions : ToolOptions {
    companion object {
        val formats = listOf("{n}", "Page {n}", "Page {n} of {total}", "{n} / {total}", "- {n} -")
    }

    var format by mutableStateOf("{n}")
    var anchor by mutableStateOf(Anchor.BottomCenter)
    var start by mutableIntStateOf(1)
    var size by mutableFloatStateOf(11f)
    var margin by mutableFloatStateOf(28f)
    var color by mutableStateOf(Color.Black)
    val pages = PageChoice()

    @Composable
    override fun Panel(session: ToolSession) {
        var menu by remember { mutableStateOf(false) }
        OutlinedTextField(
            format,
            { format = it },
            label = { Text("Format") },
            singleLine = true,
            trailingIcon = {
                Box {
                    IconButton({ menu = true }) { Icon(Icons.AutoMirrored.Rounded.List, "Formats") }
                    DropdownMenu(menu, { menu = false }) {
                        for (f in formats) DropdownMenuItem({ Text(f) }, { format = f; menu = false })
                    }
                }
            },
            modifier = Modifier.fillMaxWidth().testTag("numbers.format"),
        )
        LabeledField("Position") { AnchorPicker(anchor, middle = false) { anchor = it } }
        StepperRow("Start at $start", start, -9999..99999) { start = it }
        SliderField("Font size", size, 4f..72f, 1f, { "${it.roundToInt()} pt" }) { size = it }
        SliderField("Margin", margin, 0f..200f, 2f, { "${it.roundToInt()} pt" }) { margin = it }
        ColorRow("Colour", color) { color = it }
        PageChoiceView(pages)
        Note("Uses Helvetica, so Latin characters only.")
    }

    override fun card(session: ToolSession, item: PageItem, index: Int): CardDecor {
        if (!pages.includes(session, item)) return CardDecor()
        val stamped = pages.stamped(session)
        val position = stamped.indexOfFirst { it.id == item.id }.coerceAtLeast(0)
        val text = format.replace("{n}", "${start + position}").replace("{total}", "${start + stamped.size - 1}")
        val anchor = anchor
        val size = size
        val margin = margin
        val color = color
        return CardDecor(overlay = { points -> NumberOverlay(text, anchor, size, margin, color, points) })
    }

    override fun problem(session: ToolSession): String? {
        if (format.isBlank()) return "Enter a format."
        val latin = format.all { c -> c.code in 0x20..0x7e || c.code in 0xa0..0xff || c in "–—‘’“”•…€" }
        if (!latin) return "The format can only use Latin characters."
        if (size <= 0) return "Enter a font size."
        return session.keepProblem ?: pages.problem(session)
    }

    override suspend fun run(session: ToolSession): ToolResult {
        val pdf = session.pdf ?: throw EngineException("Choose a PDF.")
        val args = listOf(
            "numbers", pdf.file.path, "OUT", "--format=$format", "--position=${anchor.value}", "--start=$start",
            "--size=$size", "--margin=$margin", "--color=${color.hex}",
        ) + session.keepArgs + pages.args(session)
        val note = session.removedNote
        return runToFile(session, args, "${pdf.baseName}-numbered.pdf") { json -> "Numbered ${plural(json["pages"].int, "page")}$note." }
    }
}

@Composable
private fun NumberOverlay(text: String, anchor: Anchor, size: Float, margin: Float, color: Color, points: Size) {
    BoxWithConstraints(Modifier.fillMaxSize()) {
        val density = LocalDensity.current
        val scale = constraints.maxWidth / points.width.coerceAtLeast(1f)
        val alignment = when (anchor.horizontal) {
            "left" -> if (anchor.vertical == "top") Alignment.TopStart else Alignment.BottomStart
            "right" -> if (anchor.vertical == "top") Alignment.TopEnd else Alignment.BottomEnd
            else -> if (anchor.vertical == "top") Alignment.TopCenter else Alignment.BottomCenter
        }
        val pad = with(density) { (margin * scale).toDp() }
        Text(
            text,
            color = color,
            fontFamily = FontFamily.SansSerif,
            fontSize = with(density) { (size * scale).coerceAtLeast(1f).toSp() },
            softWrap = false,
            modifier = Modifier
                .align(alignment)
                .padding(
                    top = if (anchor.vertical == "top") pad else 0.dp,
                    bottom = if (anchor.vertical == "bottom") pad else 0.dp,
                    start = if (anchor.horizontal == "left") pad else 0.dp,
                    end = if (anchor.horizontal == "right") pad else 0.dp,
                ),
        )
    }
}

class WatermarkOptions : ToolOptions {
    companion object {
        const val MARGIN = 0.05f
    }

    enum class Mode { Text, Image }

    var mode by mutableStateOf(Mode.Text)
    var text by mutableStateOf("CONFIDENTIAL")
    var font by mutableStateOf(Font.System)
    var bold by mutableStateOf(true)
    var color by mutableStateOf(Color(0xffc62828))
    var image by mutableStateOf<Picture?>(null)
    var width by mutableFloatStateOf(0.7f)
    var angle by mutableFloatStateOf(45f)
    var opacity by mutableFloatStateOf(0.3f)
    var anchor by mutableStateOf(Anchor.MiddleCenter)
    var under by mutableStateOf(false)
    val pages = PageChoice()

    private var cachedKey = ""
    private var cachedText: Picture? = null

    /** The stamp as drawn: the image, or the text in its font. */
    val picture: Picture?
        get() {
            if (mode == Mode.Image) return image
            if (text.isBlank()) return null
            val key = "$text|$font|$bold|${color.hex}"
            if (key != cachedKey) {
                cachedKey = key
                cachedText = Picture.text(text, font, bold, color)
            }
            return cachedText
        }

    @Composable
    override fun Panel(session: ToolSession) {
        Segmented(Mode.entries.map { it to it.name }, mode) { mode = it }
        when (mode) {
            Mode.Text -> {
                TextInput("Text", text, { text = it }, placeholder = "CONFIDENTIAL", singleLine = false, tag = "watermark.text")
                DropdownField("Font", Font.entries.map { it to it.label }, font, { font = it }, font = { it.compose })
                SwitchRow("Bold", bold) { bold = it }
                ColorRow("Colour", color) { color = it }
            }
            Mode.Image -> {
                image?.let { Image(it.bitmap.asImageBitmap(), null, Modifier.heightIn(max = 80.dp)) }
                ImageSourceButtons { data -> Picture.fromImageData(data)?.let { image = it } }
            }
        }
        SliderField("Size", width, 0.1f..1f, 0.05f, ::percent) { width = it }
        SliderField("Angle", angle, -90f..90f, 5f, { "${it.roundToInt()}°" }) { angle = it }
        SliderField("Opacity", opacity, 0.05f..1f, 0.05f, ::percent) { opacity = it }
        LabeledField("Position") { AnchorPicker(anchor) { anchor = it } }
        SwitchRow("Behind the page content", under) { under = it }
        PageChoiceView(pages)
    }

    override fun card(session: ToolSession, item: PageItem, index: Int): CardDecor {
        val picture = picture ?: return CardDecor()
        if (!pages.includes(session, item)) return CardDecor()
        val stamp = Stamp(picture, width, angle, opacity, anchor, under)
        return CardDecor(overlay = { points -> WatermarkOverlay(stamp, points) })
    }

    override fun problem(session: ToolSession): String? = when {
        mode == Mode.Text && text.isBlank() -> "Enter the watermark text."
        mode == Mode.Image && image == null -> "Choose an image."
        picture == null -> "Couldn’t draw the watermark."
        else -> session.keepProblem ?: pages.problem(session)
    }

    override suspend fun run(session: ToolSession): ToolResult {
        val pdf = session.pdf ?: throw EngineException("Choose a PDF.")
        val picture = picture ?: throw EngineException("Couldn’t draw the watermark.")
        val stamp = Workspace.write(picture.data, "watermark.png")
        val args = listOf(
            "stamp", pdf.file.path, "OUT", "--image=${stamp.path}", "--position=${anchor.value}", "--margin=$MARGIN",
            "--width=$width", "--angle=$angle", "--opacity=$opacity",
        ) + session.keepArgs + pages.args(session) + if (under) listOf("--under") else emptyList()
        val note = session.removedNote
        return runToFile(session, args, "${pdf.baseName}-watermarked.pdf") { json -> "Watermarked ${plural(json["pages"].int, "page")}$note." }
    }
}

private class Stamp(val picture: Picture, val width: Float, val angle: Float, val opacity: Float, val anchor: Anchor, val under: Boolean)

@Composable
private fun WatermarkOverlay(stamp: Stamp, points: Size) {
    val image = remember(stamp.picture) { stamp.picture.bitmap.asImageBitmap() }
    Canvas(Modifier.fillMaxSize()) {
        val scale = size.width / points.width.coerceAtLeast(1f)
        val w = stamp.width * points.width
        val h = w * stamp.picture.size.height / stamp.picture.size.width.coerceAtLeast(1f)
        val rad = stamp.angle * PI.toFloat() / 180
        val box = Size(abs(w * cos(rad)) + abs(h * sin(rad)), abs(w * sin(rad)) + abs(h * cos(rad)))
        val centre = stamp.anchor.place(points, box, WatermarkOptions.MARGIN * minOf(points.width, points.height))
        rotate(-stamp.angle, Offset(centre.x * scale, centre.y * scale)) {
            drawImage(
                image,
                dstOffset = IntOffset(((centre.x - w / 2) * scale).roundToInt(), ((centre.y - h / 2) * scale).roundToInt()),
                dstSize = IntSize((w * scale).roundToInt().coerceAtLeast(1), (h * scale).roundToInt().coerceAtLeast(1)),
                alpha = stamp.opacity,
                // Behind the content: the page's ink stays on top, as in the result.
                blendMode = if (stamp.under) BlendMode.Multiply else BlendMode.SrcOver,
            )
        }
    }
}
