// Mark areas on an enlarged page and create an image-only redacted copy.
package io.github.ivadim.pdftools

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size as ComposeSize
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import java.util.Locale
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.min

class RedactOptions : ToolOptions {
    data class Mark(val page: Int, val x: Float, val y: Float, val width: Float, val height: Float)
    val marks = mutableStateListOf<Mark>()
    var editing by mutableStateOf<PageItem?>(null)
    var smaller by mutableStateOf(false)

    override fun reset(session: ToolSession) { marks.clear(); editing = null }

    @Composable override fun Panel(session: ToolSession) {
        Note("Choose a page, then drag over each area to remove. Review the marks before creating the copy.")
        Text("The exported PDF converts every page to an image. Text will not be selectable; links, forms and document details are removed.",
            style = MaterialTheme.typography.bodyMedium)
        Row(Modifier.fillMaxWidth().semantics(mergeDescendants = true) { contentDescription = "Smaller file" }
            .toggleable(value = smaller, enabled = !session.running, role = Role.Switch, onValueChange = { smaller = it }),
            verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
            Text("Smaller file", Modifier.weight(1f))
            Switch(checked = smaller, onCheckedChange = null, enabled = !session.running)
        }
        Note("Smaller file lowers resolution and may use JPEG; text may be less sharp.")
        Text("${plural(marks.size, "area")} marked.")
        OutlinedButton(onClick = { marks.clear() }, enabled = marks.isNotEmpty() && !session.running) { Text("Clear all marks") }
        editing?.let { item -> RedactEditor(this, session, item) { editing = null } }
    }
    override fun card(session: ToolSession, item: PageItem, index: Int): CardDecor = CardDecor(
        overlay = { Marks(marks.filter { it.page == item.page }) },
        onTap = { if (!session.running) editing = item },
    )
    override fun problem(session: ToolSession): String? = if (marks.isEmpty()) "Choose a page and mark an area to remove." else null
    override suspend fun run(session: ToolSession): ToolResult {
        val pdf = session.pdf ?: throw EngineException("Choose a PDF.")
        val areas = marks.joinToString(",") { m ->
            "${m.page}:" + listOf(m.x, m.y, m.width, m.height).joinToString(":") { String.format(Locale.ROOT, "%.8f", it) }
        }
        return runToFile(session, listOf("redact", pdf.file.path, "OUT", "--areas=$areas", "--quality=${if (smaller) "smaller" else "higher"}"), "${pdf.baseName}-redacted.pdf") { json ->
            "Created a redacted copy of ${plural(json["pages"].int, "page")}."
        }
    }
}

@Composable
private fun Marks(marks: List<RedactOptions.Mark>) {
    Canvas(Modifier.fillMaxSize()) {
        marks.forEach { m ->
            drawRect(Color.Black, topLeft = Offset(m.x * size.width, m.y * size.height), size = ComposeSize(m.width * size.width, m.height * size.height))
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun RedactEditor(options: RedactOptions, session: ToolSession, item: PageItem, close: () -> Unit) {
    var showFields by remember { mutableStateOf(false) }
    var left by remember { mutableStateOf("0") }
    var top by remember { mutableStateOf("0") }
    var width by remember { mutableStateOf("10") }
    var height by remember { mutableStateOf("10") }
    val values = listOf(left, top, width, height).map { it.toFloatOrNull()?.div(100f) }
    val valid = values.all { it != null && it.isFinite() } && values.let { v ->
        val (x, y, w, h) = v.map { it ?: 0f }
        x >= 0 && y >= 0 && w > 0 && h > 0 && x + w <= 1 && y + h <= 1
    }
    Dialog(onDismissRequest = close, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        Surface(Modifier.fillMaxSize()) {
            Scaffold(topBar = {
                TopAppBar(title = { Text("Page ${item.page}") }, actions = {
                    TextButton(onClick = {
                        val index = options.marks.indexOfLast { it.page == item.page }
                        if (index >= 0) options.marks.removeAt(index)
                    }) { Text("Undo") }
                    TextButton(onClick = close) { Text("Done") }
                })
            }) { padding ->
                Column(Modifier.fillMaxSize().padding(padding).padding(12.dp)) {
                    Text("Drag across an area to mark it.", style = MaterialTheme.typography.bodyMedium)
                    PageImage(session, item, 0, false, Modifier.fillMaxWidth().weight(1f)) {
                        var start by remember { mutableStateOf<Offset?>(null) }
                        var end by remember { mutableStateOf<Offset?>(null) }
                        Canvas(Modifier.fillMaxSize().semantics { contentDescription = "Page marking area" }.testTag("redact.stage").pointerInput(item.id) {
                            detectDragGestures(
                                onDragStart = { p -> start = p; end = p },
                                onDrag = { change, _ -> change.consume(); end = change.position },
                                onDragCancel = { start = null; end = null },
                                onDragEnd = {
                                    val a = start; val b = end
                                    if (a != null && b != null && size.width > 0 && size.height > 0) {
                                        val x1 = a.x.coerceIn(0f, size.width.toFloat()); val x2 = b.x.coerceIn(0f, size.width.toFloat())
                                        val y1 = a.y.coerceIn(0f, size.height.toFloat()); val y2 = b.y.coerceIn(0f, size.height.toFloat())
                                        if (abs(x2-x1) > 1 && abs(y2-y1) > 1) options.marks.add(RedactOptions.Mark(item.page,
                                            min(x1, x2) / size.width, min(y1, y2) / size.height, abs(x2-x1) / size.width, abs(y2-y1) / size.height))
                                    }
                                    start = null; end = null
                                },
                            )
                        }) {
                            options.marks.filter { it.page == item.page }.forEach { m ->
                                drawRect(Color.Black, topLeft = Offset(m.x * size.width, m.y * size.height), size = ComposeSize(m.width * size.width, m.height * size.height))
                            }
                            val a = start; val b = end
                            if (a != null && b != null) drawRect(Color.Black.copy(alpha = .65f), topLeft = Offset(min(a.x, b.x), min(a.y, b.y)), size = ComposeSize(abs(a.x-b.x), abs(a.y-b.y)))
                        }
                    }
                    TextButton(onClick = { showFields = !showFields }) { Text("Enter an area") }
                    if (showFields) {
                        listOf(Triple("Left %", left, { v: String -> left = v }), Triple("Top %", top, { v: String -> top = v }),
                            Triple("Width %", width, { v: String -> width = v }), Triple("Height %", height, { v: String -> height = v }))
                            .chunked(2).forEach { pair ->
                                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                    pair.forEach { (label, value, update) ->
                                        OutlinedTextField(value = value, onValueChange = update, label = { Text(label) },
                                            modifier = Modifier.weight(1f), singleLine = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal))
                                    }
                                }
                            }
                        TextButton(enabled = valid, onClick = {
                            val (x, y, w, h) = values.map { it ?: 0f }
                            options.marks.add(RedactOptions.Mark(item.page, x, y, w, h))
                        }) { Text("Add area") }
                    }
                }
            }
        }
    }
}
