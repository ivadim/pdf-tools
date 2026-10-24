// What each tool adds to the shared screen: its options panel, what it shows
// on the page cards, what's missing before it can run, and the command. Also
// the form pieces the panels share.

package io.github.ivadim.pdftools

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.TextAutoSize
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.KeyboardArrowLeft
import androidx.compose.material.icons.automirrored.rounded.KeyboardArrowRight
import androidx.compose.material.icons.automirrored.rounded.RotateLeft
import androidx.compose.material.icons.automirrored.rounded.RotateRight
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.Check
import androidx.compose.material.icons.rounded.Remove
import androidx.compose.material.icons.rounded.Visibility
import androidx.compose.material.icons.rounded.VisibilityOff
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExposedDropdownMenuAnchorType
import androidx.compose.material3.ExposedDropdownMenuBox
import androidx.compose.material3.ExposedDropdownMenuDefaults
import androidx.compose.material3.FilledTonalIconButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.roundToInt

interface ToolOptions {
    /** The options panel. */
    @Composable fun Panel(session: ToolSession)

    /** Buttons above the page grid. */
    fun gridActions(session: ToolSession): List<CardControl> = emptyList()

    /** How a page card looks and what it can do. */
    fun card(session: ToolSession, item: PageItem, index: Int): CardDecor = CardDecor()

    /** What's missing before the tool can run. */
    fun problem(session: ToolSession): String?

    suspend fun run(session: ToolSession): ToolResult

    /** Called when a new file is opened. */
    fun reset(session: ToolSession) {}
}

class CardControl(val icon: ImageVector, val label: String, val disabled: Boolean = false, val action: () -> Unit)

class CardDecor(
    val controls: List<CardControl> = emptyList(),
    /** Split: the colour of the file the page goes to. */
    val partColor: Color? = null,
    /** Shown dimmed, as not in the result. */
    val dimmed: Boolean = false,
    val gray: Boolean = false,
    /** Drawn over the page; gets the page's size in points. */
    val overlay: (@Composable (Size) -> Unit)? = null,
    /** A tap on the page, at a fraction of its width and height (instead of opening it). */
    val onTap: ((Offset) -> Unit)? = null,
)

// Shared pieces

val partColors = listOf(0xff2f6fed, 0xffe0752d, 0xff1f9d6a, 0xffb8459e, 0xffc9a227, 0xff3aa7c9, 0xffd24b4b, 0xff7a5cd6).map { Color(it) }

fun fileColor(index: Int) = partColors[index % partColors.size]

fun moveControls(session: ToolSession, index: Int) = listOf(
    CardControl(Icons.AutoMirrored.Rounded.KeyboardArrowLeft, "Move earlier", disabled = index == 0) { session.move(index, -1) },
    CardControl(Icons.AutoMirrored.Rounded.KeyboardArrowRight, "Move later", disabled = index == session.items.size - 1) { session.move(index, 1) },
)

fun rotateControls(session: ToolSession, index: Int) = listOf(
    CardControl(Icons.AutoMirrored.Rounded.RotateLeft, "Rotate left") { session.update(index) { it.copy(rotation = (it.rotation + 270) % 360) } },
    CardControl(Icons.AutoMirrored.Rounded.RotateRight, "Rotate right") { session.update(index) { it.copy(rotation = (it.rotation + 90) % 360) } },
)

/** Runs a command whose output is the file `name` (passed as "OUT") in a new folder. */
suspend fun runToFile(session: ToolSession, args: List<String>, name: String, message: (Json) -> String): ToolResult {
    val out = session.outputFolder().resolve(name)
    val json = Engine.run(args.map { if (it == "OUT") out.path else it })
    return ToolResult(message(json), listOf(out))
}

fun percent(v: Float) = "${(v * 100).roundToInt()}%"

/** "All pages" or the pages typed; used by the stamping tools. */
class PageChoice {
    var custom by mutableStateOf(false)
    var text by mutableStateOf("")

    private class Memo(val stamped: List<PageItem>, val ids: Set<String>, val error: String?)

    /** The kept pages the stamp goes on, worked out once per change rather than once per page card. */
    private var memoKey: List<Any> = emptyList()
    private var memo = Memo(emptyList(), emptySet(), null)

    private fun evaluate(session: ToolSession): Memo {
        val kept = session.kept
        val key = listOf(custom, text, session.pdf?.pageCount ?: 0, kept.map { it.id })
        if (key != memoKey) {
            memoKey = key
            memo = if (!custom) {
                Memo(kept, kept.map { it.id }.toSet(), null)
            } else {
                try {
                    val pages = Engine.parseList(text, session.pdf?.pageCount ?: 0).toSet()
                    val stamped = kept.filter { it.page in pages }
                    Memo(stamped, stamped.map { it.id }.toSet(), null)
                } catch (e: EngineException) {
                    Memo(emptyList(), emptySet(), e.message)
                }
            }
        }
        return memo
    }

    /** The chosen pages as positions in the result (after left-out pages are gone); null = all. */
    fun positions(session: ToolSession): List<Int>? {
        if (!custom) return null
        val result = evaluate(session)
        result.error?.let { throw EngineException(it) }
        return session.kept.withIndex().filter { it.value.id in result.ids }.map { it.index + 1 }
    }

    fun problem(session: ToolSession): String? {
        if (!custom) return null
        val result = evaluate(session)
        result.error?.let { return it }
        return if (result.stamped.isEmpty()) "None of the chosen pages are kept." else null
    }

    fun includes(session: ToolSession, item: PageItem) = item.keep && item.id in evaluate(session).ids

    /** The pages the stamp goes on, in order. */
    fun stamped(session: ToolSession) = evaluate(session).stamped

    fun args(session: ToolSession): List<String> = positions(session)?.let { listOf("--pages=${formatList(it)}") } ?: emptyList()
}

@Composable
fun PageChoiceView(choice: PageChoice) {
    Segmented(listOf(false to "All pages", true to "Some pages"), choice.custom) { choice.custom = it }
    if (choice.custom) {
        TextInput("Pages", choice.text, { choice.text = it }, placeholder = "1-3, 5", keyboard = KeyboardType.Uri)
    }
}

enum class Anchor(val value: String) {
    TopLeft("top-left"), TopCenter("top-center"), TopRight("top-right"),
    MiddleLeft("middle-left"), MiddleCenter("middle-center"), MiddleRight("middle-right"),
    BottomLeft("bottom-left"), BottomCenter("bottom-center"), BottomRight("bottom-right");

    val vertical get() = value.substringBefore('-')
    val horizontal get() = value.substringAfter('-')

    /** Where a w×h box goes on a page at this anchor, `margin` from the edges; its centre. */
    fun place(page: Size, box: Size, margin: Float): Offset {
        val x = when (horizontal) {
            "left" -> margin + box.width / 2
            "right" -> page.width - margin - box.width / 2
            else -> page.width / 2
        }
        val y = when (vertical) {
            "top" -> margin + box.height / 2
            "bottom" -> page.height - margin - box.height / 2
            else -> page.height / 2
        }
        return Offset(x, y)
    }

    companion object {
        fun of(value: String) = entries.first { it.value == value }
    }
}

/** A 3×3 (or 3×2) grid of positions on a page. */
@Composable
fun AnchorPicker(anchor: Anchor, middle: Boolean = true, onChange: (Anchor) -> Unit) {
    val rows = if (middle) listOf("top", "middle", "bottom") else listOf("top", "bottom")
    Column(
        Modifier
            .border(1.dp, MaterialTheme.colorScheme.outlineVariant, RoundedCornerShape(8.dp))
            .padding(8.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        for (row in rows) {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                for (column in listOf("left", "center", "right")) {
                    val value = Anchor.of("$row-$column")
                    val selected = value == anchor
                    Spacer(
                        Modifier
                            .size(34.dp, 24.dp)
                            .clip(RoundedCornerShape(4.dp))
                            .background(if (selected) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.surfaceVariant)
                            .clickable { onChange(value) }
                            .semantics {
                                contentDescription = "$row $column"
                                this.selected = selected
                            },
                    )
                }
            }
        }
    }
}

/** A label above a control, like a form field. */
@Composable
fun LabeledField(label: String, content: @Composable () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(label, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
        content()
    }
}

@Composable
fun TextInput(
    label: String,
    value: String,
    onChange: (String) -> Unit,
    placeholder: String = "",
    keyboard: KeyboardType = KeyboardType.Text,
    singleLine: Boolean = true,
    tag: String? = null,
) {
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        label = { Text(label) },
        placeholder = { Text(placeholder) },
        singleLine = singleLine,
        keyboardOptions = KeyboardOptions(keyboardType = keyboard),
        modifier = Modifier.fillMaxWidth().let { if (tag != null) it.testTag(tag) else it },
    )
}

@Composable
fun PasswordInput(label: String, value: String, onChange: (String) -> Unit, tag: String? = null) {
    var shown by rememberSaveable { mutableStateOf(false) }
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        label = { Text(label) },
        singleLine = true,
        visualTransformation = if (shown) VisualTransformation.None else PasswordVisualTransformation(),
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
        trailingIcon = {
            IconButton({ shown = !shown }) {
                Icon(if (shown) Icons.Rounded.VisibilityOff else Icons.Rounded.Visibility, if (shown) "Hide password" else "Show password")
            }
        },
        modifier = Modifier.fillMaxWidth().let { if (tag != null) it.testTag(tag) else it },
    )
}

@Composable
fun SliderField(label: String, value: Float, range: ClosedFloatingPointRange<Float>, step: Float, format: (Float) -> String, onChange: (Float) -> Unit) {
    Column {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(label, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Spacer(Modifier.weight(1f))
            Text(format(value), style = MaterialTheme.typography.labelLarge.copy(fontFamily = FontFamily.Monospace))
        }
        val steps = ((range.endInclusive - range.start) / step).roundToInt() - 1
        Slider(
            value = value,
            onValueChange = { onChange(range.start + ((it - range.start) / step).roundToInt() * step) },
            valueRange = range,
            steps = steps.coerceIn(0, 200),
            // Snaps to the steps without drawing a dot for each.
            colors = SliderDefaults.colors(activeTickColor = Color.Transparent, inactiveTickColor = Color.Transparent),
            modifier = Modifier.semantics { contentDescription = label },
        )
    }
}

@Composable
fun SwitchRow(label: String, checked: Boolean, onChange: (Boolean) -> Unit) {
    Row(
        Modifier.fillMaxWidth().clickable { onChange(!checked) }.padding(vertical = 2.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, Modifier.weight(1f), style = MaterialTheme.typography.bodyLarge)
        Switch(checked, onChange)
    }
}

@Composable
fun StepperRow(label: String, value: Int, range: IntRange, onChange: (Int) -> Unit) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(label, Modifier.weight(1f), style = MaterialTheme.typography.bodyLarge)
        FilledTonalIconButton({ onChange((value - 1).coerceIn(range)) }, enabled = value > range.first) {
            Icon(Icons.Rounded.Remove, "Decrease")
        }
        FilledTonalIconButton({ onChange((value + 1).coerceIn(range)) }, enabled = value < range.last) {
            Icon(Icons.Rounded.Add, "Increase")
        }
    }
}

/** Material's segmented buttons, the counterpart of iOS's segmented pickers. */
@Composable
fun <T> Segmented(options: List<Pair<T, String>>, selected: T, onSelect: (T) -> Unit) {
    SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
        options.forEachIndexed { index, (value, label) ->
            SegmentedButton(
                selected = value == selected,
                onClick = { onSelect(value) },
                shape = SegmentedButtonDefaults.itemShape(index, options.size),
                icon = {},
                // Four options fit a phone's width: long labels shrink rather than clip.
                label = {
                    BasicText(
                        label,
                        maxLines = 1,
                        style = MaterialTheme.typography.labelLarge.copy(color = LocalContentColor.current),
                        autoSize = TextAutoSize.StepBased(minFontSize = 10.sp, maxFontSize = MaterialTheme.typography.labelLarge.fontSize),
                    )
                },
            )
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun <T> DropdownField(label: String, options: List<Pair<T, String>>, selected: T, onSelect: (T) -> Unit, font: ((T) -> FontFamily)? = null) {
    var expanded by remember { mutableStateOf(false) }
    ExposedDropdownMenuBox(expanded, { expanded = it }) {
        OutlinedTextField(
            value = options.firstOrNull { it.first == selected }?.second ?: "",
            onValueChange = {},
            readOnly = true,
            label = { Text(label) },
            textStyle = font?.let { MaterialTheme.typography.bodyLarge.copy(fontFamily = it(selected)) } ?: MaterialTheme.typography.bodyLarge,
            trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(expanded) },
            modifier = Modifier.fillMaxWidth().menuAnchor(ExposedDropdownMenuAnchorType.PrimaryNotEditable),
        )
        ExposedDropdownMenu(expanded, { expanded = false }) {
            for ((value, text) in options) {
                DropdownMenuItem(
                    text = { Text(text, fontFamily = font?.invoke(value)) },
                    onClick = {
                        onSelect(value)
                        expanded = false
                    },
                )
            }
        }
    }
}

/** Colour swatches (the stamps' and signature's ink). */
val swatches = listOf(0xff000000, 0xff424242, 0xff1a237e, 0xff1565c0, 0xff2e7d32, 0xffc62828, 0xffef6c00, 0xff6a1b9a, 0xff9e9e9e).map { Color(it) }

@Composable
fun ColorRow(label: String, color: Color, onChange: (Color) -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(label, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            for (swatch in swatches) {
                val selected = swatch == color
                Box(
                    Modifier
                        .size(32.dp)
                        .clip(CircleShape)
                        .background(swatch)
                        .border(if (selected) 3.dp else 1.dp, if (selected) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outlineVariant, CircleShape)
                        .clickable { onChange(swatch) }
                        .semantics {
                            contentDescription = "Colour ${swatch.hex}"
                            this.selected = selected
                        },
                    contentAlignment = Alignment.Center,
                ) {
                    if (selected) Icon(Icons.Rounded.Check, null, tint = if (swatch.luminance() > 0.5f) Color.Black else Color.White, modifier = Modifier.size(18.dp))
                }
            }
        }
    }
}

@Composable
fun Note(text: String) {
    Text(text, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
}

@Composable
fun SectionTitle(text: String) {
    Text(text, style = MaterialTheme.typography.titleSmall)
}
