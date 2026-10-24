// Merge, Split, Rotate and Rearrange: tools about which pages go where.

package io.github.ivadim.pdftools

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.RotateLeft
import androidx.compose.material.icons.automirrored.rounded.RotateRight
import androidx.compose.material.icons.automirrored.rounded.Undo
import androidx.compose.material.icons.rounded.ContentCopy
import androidx.compose.material.icons.rounded.Delete
import androidx.compose.material.icons.rounded.SwapHoriz
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.text.input.KeyboardType
import java.io.File

class MergeOptions : ToolOptions {
    @Composable
    override fun Panel(session: ToolSession) {
        Note("${plural(session.files.size, "file")}, ${plural(session.kept.size, "page")} in the result. Drag a page by its ≡ handle to move it.")
    }

    override fun gridActions(session: ToolSession) = listOf(
        CardControl(Icons.AutoMirrored.Rounded.Undo, "Reset order") {
            val sorted = session.items.sortedWith(compareBy({ item -> session.files.indexOfFirst { it === item.file } }, { it.page }))
            session.items.clear()
            session.items += sorted
        },
    )

    override fun card(session: ToolSession, item: PageItem, index: Int): CardDecor {
        val file = session.files.indexOfFirst { it === item.file }.coerceAtLeast(0)
        return CardDecor(controls = moveControls(session, index) + rotateControls(session, index), partColor = fileColor(file))
    }

    override fun problem(session: ToolSession) = if (session.files.size < 2) "Add at least two PDFs." else session.keepProblem

    override suspend fun run(session: ToolSession): ToolResult {
        val pages = session.kept.map { item -> "${session.files.indexOfFirst { it === item.file }.coerceAtLeast(0)}:${item.page}:${item.rotation}" }
        val args = listOf("merge", "OUT") + session.files.map { it.file.path } + "--pages=${pages.joinToString(",")}"
        return runToFile(session, args, "merged.pdf") { json -> "Merged ${plural(json["files"].int, "file")} into ${plural(json["pages"].int, "page")}." }
    }
}

class SplitOptions : ToolOptions {
    enum class Mode(val label: String) { Ranges("Ranges"), Every("Every N"), Each("Each page"), Extract("Extract") }

    var mode by mutableStateOf(Mode.Ranges)
    var ranges by mutableStateOf("")
    var every by mutableIntStateOf(2)
    var extract by mutableStateOf("")
    var base by mutableStateOf("part")

    override fun reset(session: ToolSession) {
        base = session.pdf?.baseName ?: "part"
    }

    /** Each output file's pages. */
    fun parts(session: ToolSession): List<List<Int>> {
        val total = session.pdf?.pageCount ?: 0
        return when (mode) {
            Mode.Ranges -> Engine.parseGroups(ranges, total)
            Mode.Every -> (1..total step every.coerceAtLeast(1)).map { start -> (start..minOf(total, start + every - 1)).toList() }
            Mode.Each -> (1..total.coerceAtLeast(1)).map { listOf(it) }
            Mode.Extract -> listOf(Engine.parseList(extract, total))
        }
    }

    @Composable
    override fun Panel(session: ToolSession) {
        Segmented(Mode.entries.map { it to it.label }, mode) { mode = it }
        when (mode) {
            Mode.Ranges -> TextInput("One file per range", ranges, { ranges = it }, placeholder = "1-3, 4-6, 7-", keyboard = KeyboardType.Uri, tag = "split.ranges")
            Mode.Every -> StepperRow("Pages per file: $every", every, 1..999) { every = it }
            Mode.Extract -> TextInput("Pages to extract into one file", extract, { extract = it }, placeholder = "1, 3-5", keyboard = KeyboardType.Uri, tag = "split.extract")
            Mode.Each -> {}
        }
        if (mode != Mode.Extract) TextInput("File names", base, { base = it }, placeholder = "part", tag = "split.name")
        Note("Pages are coloured by the file they’ll end up in.")
    }

    override fun card(session: ToolSession, item: PageItem, index: Int): CardDecor {
        val parts = runCatching { parts(session) }.getOrNull() ?: return CardDecor()
        val part = parts.indexOfFirst { item.page in it }
        return CardDecor(partColor = if (part >= 0) fileColor(part) else null, dimmed = part < 0)
    }

    override fun problem(session: ToolSession): String? = try {
        val parts = parts(session)
        when {
            mode != Mode.Extract && base.isBlank() -> "Enter a name for the files."
            parts.isEmpty() -> "Nothing to split."
            else -> null
        }
    } catch (e: EngineException) {
        e.message
    }

    override suspend fun run(session: ToolSession): ToolResult {
        val pdf = session.pdf ?: throw EngineException("Choose a PDF.")
        val parts = parts(session)
        val name = if (mode == Mode.Extract) "part" else base.trim().replace('/', '_')
        val folder = session.outputFolder()
        val json = Engine.run(listOf("split", pdf.file.path, folder.path, "--parts=${parts.joinToString(";") { formatList(it) }}", "--name=$name"))
        val files = json["files"].array.mapNotNull { it.string }.map(::File)
        if (mode == Mode.Extract && files.isNotEmpty()) {
            val renamed = File(folder, "${pdf.baseName}-pages-${formatList(parts[0]).replace(", ", "_")}.pdf")
            files[0].renameTo(renamed)
            return ToolResult("Extracted ${plural(parts[0].size, "page")}.", listOf(renamed))
        }
        val sorted = files.sortedWith(compareBy<File>({ it.nameWithoutExtension.substringAfterLast('-').toIntOrNull() ?: 0 }, { it.name }))
        return ToolResult("Created ${plural(json["count"].int, "file")}.", sorted)
    }
}

class RotateOptions : ToolOptions {
    @Composable
    override fun Panel(session: ToolSession) {
        Note("Rotate pages with the buttons under each one, or all at once.")
    }

    private fun turnAll(session: ToolSession, change: (Int) -> Int) {
        for (i in session.items.indices) session.update(i) { it.copy(rotation = change(it.rotation)) }
    }

    override fun gridActions(session: ToolSession) = listOf(
        CardControl(Icons.AutoMirrored.Rounded.RotateLeft, "All left") { turnAll(session) { (it + 270) % 360 } },
        CardControl(Icons.AutoMirrored.Rounded.RotateRight, "All right") { turnAll(session) { (it + 90) % 360 } },
        CardControl(Icons.AutoMirrored.Rounded.Undo, "Reset") { turnAll(session) { 0 } },
    )

    override fun card(session: ToolSession, item: PageItem, index: Int) = CardDecor(controls = rotateControls(session, index))

    override fun problem(session: ToolSession): String? {
        if (session.items.all { it.keep && it.rotation == 0 }) return "Rotate a page first."
        return session.keepProblem
    }

    override suspend fun run(session: ToolSession): ToolResult {
        val pdf = session.pdf ?: throw EngineException("Choose a PDF.")
        val pages = session.kept.joinToString(",") { "${it.page}:${it.rotation}" }
        val turned = session.kept.count { it.rotation != 0 }
        val note = session.removedNote
        return runToFile(session, listOf("organize", pdf.file.path, "OUT", "--pages=$pages"), "${pdf.baseName}-rotated.pdf") {
            "Rotated ${plural(turned, "page")}$note."
        }
    }
}

class RearrangeOptions : ToolOptions {
    @Composable
    override fun Panel(session: ToolSession) {
        Note("${plural(session.items.size, "page")} in the result. Drag a page by its ≡ handle to move it.")
    }

    override fun gridActions(session: ToolSession) = listOf(
        CardControl(Icons.Rounded.SwapHoriz, "Reverse") {
            val reversed = session.items.reversed()
            session.items.clear()
            session.items += reversed
        },
        CardControl(Icons.AutoMirrored.Rounded.Undo, "Reset") { session.resetItems() },
    )

    override fun card(session: ToolSession, item: PageItem, index: Int) = CardDecor(
        controls = moveControls(session, index) + listOf(
            CardControl(Icons.Rounded.ContentCopy, "Duplicate") { session.items.add(index + 1, PageItem(item.file, item.page)) },
            CardControl(Icons.Rounded.Delete, "Delete", disabled = session.items.size == 1) { session.items.removeAt(index) },
        ),
    )

    override fun problem(session: ToolSession) = if (session.items.isEmpty()) "Keep at least one page." else null

    override suspend fun run(session: ToolSession): ToolResult {
        val pdf = session.pdf ?: throw EngineException("Choose a PDF.")
        val pages = session.items.joinToString(",") { "${it.page}" }
        val count = session.items.size
        return runToFile(session, listOf("organize", pdf.file.path, "OUT", "--pages=$pages"), "${pdf.baseName}-rearranged.pdf") {
            "Saved ${plural(count, "page")}."
        }
    }
}
