// Compress, Protect, Unlock, Metadata and Grayscale: one PDF in, one out.

package io.github.ivadim.pdftools

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Lock
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import kotlin.math.abs
import kotlin.math.roundToInt
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch

/** Background work an options object starts itself (counting images). */
private val background = CoroutineScope(SupervisorJob() + Dispatchers.Main)

private fun pdfOf(session: ToolSession) = session.pdf ?: throw EngineException("Choose a PDF.")

class CompressOptions : ToolOptions {
    enum class Level { Lossless, Balanced, Strong }

    var level by mutableStateOf(Level.Balanced)
    /** The file's images and their stored size: counting them reads the whole file, so only Compress does it, after opening. */
    var imageStats by mutableStateOf<Pair<Int, Long>?>(null)

    override fun reset(session: ToolSession) {
        imageStats = null
        val pdf = session.pdf?.takeIf { !it.locked } ?: return
        background.launch {
            val info = runCatching { Engine.run(listOf("info", pdf.file.path, "--images")) }.getOrNull() ?: return@launch
            if (session.pdf === pdf) imageStats = info["images"].int to info["image_bytes"].long
        }
    }

    @Composable
    override fun Panel(session: ToolSession) {
        Segmented(Level.entries.map { it to it.name }, level) { level = it }
        Note(
            when (level) {
                Level.Lossless -> "Drops unused data. Page content is untouched."
                Level.Balanced -> "Also downscales images to 2000 px and re-encodes them as JPEG (quality 75)."
                Level.Strong -> "Also downscales images to 1200 px and re-encodes them as JPEG (quality 50)."
            },
        )
        session.pdf?.let { pdf ->
            val stats = imageStats
            Note(if (stats != null) "${formatBytes(pdf.size)} · ${plural(stats.first, "image")} (${formatBytes(stats.second)})" else "${formatBytes(pdf.size)} · counting images…")
        }
    }

    override fun problem(session: ToolSession) = session.keepProblem

    override suspend fun run(session: ToolSession): ToolResult {
        val pdf = pdfOf(session)
        val note = session.removedNote
        val args = listOf("compress", pdf.file.path, "OUT", "--level=${level.name.lowercase()}") + session.keepArgs
        return runToFile(session, args, "${pdf.baseName}-compressed.pdf") { json ->
            val (before, after) = json["before"].long to json["after"].long
            val saved = if (before > 0) ((1 - after.toDouble() / before) * 100).roundToInt() else 0
            "${formatBytes(before)} → ${formatBytes(after)} (${if (saved >= 0) "−" else "+"}${abs(saved)}%)$note."
        }
    }
}

class ProtectOptions : ToolOptions {
    var password by mutableStateOf("")
    var confirm by mutableStateOf("")
    var owner by mutableStateOf("")
    var print by mutableStateOf(true)
    var copy by mutableStateOf(true)
    var modify by mutableStateOf(true)
    var annotate by mutableStateOf(true)

    @Composable
    override fun Panel(session: ToolSession) {
        PasswordInput("Password to open", password, { password = it }, tag = "password")
        PasswordInput("Repeat the password", confirm, { confirm = it }, tag = "confirm")
        SectionTitle("Allow")
        SwitchRow("Printing", print) { print = it }
        SwitchRow("Copying text and images", copy) { copy = it }
        SwitchRow("Editing", modify) { modify = it }
        SwitchRow("Comments and forms", annotate) { annotate = it }
        PasswordInput("Owner password (optional)", owner, { owner = it })
        Note("The owner password lifts the restrictions.")
    }

    override fun problem(session: ToolSession) = when {
        password.isEmpty() -> "Enter a password."
        password != confirm -> "The passwords don’t match."
        else -> session.keepProblem
    }

    override suspend fun run(session: ToolSession): ToolResult {
        val pdf = pdfOf(session)
        val args = listOf("protect", pdf.file.path, "OUT", "--password=$password") + session.keepArgs
        val extra = buildList {
            if (owner.isNotEmpty()) add("--owner=$owner")
            for ((allowed, name) in listOf(print to "print", copy to "copy", modify to "modify", annotate to "annotate")) {
                if (!allowed) add("--no-$name")
            }
        }
        val note = session.removedNote
        return runToFile(session, args + extra, "${pdf.baseName}-protected.pdf") { "Protected with a password$note." }
    }
}

class UnlockOptions : ToolOptions {
    var password by mutableStateOf("")

    override fun reset(session: ToolSession) {
        password = ""
    }

    @Composable
    override fun Panel(session: ToolSession) {
        val pdf = session.pdf ?: return
        when {
            !pdf.encrypted -> Note("This file has no password or restrictions.")
            !pdf.locked -> Note("This file only restricts what can be done with it; no password is needed to remove that.")
            else -> {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Icon(Icons.Rounded.Lock, null, Modifier.size(18.dp))
                    Text("This file is protected with a password.", style = MaterialTheme.typography.bodyMedium)
                }
                PasswordInput("Password", password, { password = it }, tag = "password")
            }
        }
    }

    override fun problem(session: ToolSession): String? {
        val pdf = session.pdf ?: return "Choose a PDF."
        if (!pdf.encrypted) return "Nothing to unlock."
        return if (pdf.locked && password.isEmpty()) "Enter the password." else null
    }

    override suspend fun run(session: ToolSession): ToolResult {
        val pdf = pdfOf(session)
        return runToFile(session, listOf("unlock", pdf.file.path, "OUT", "--password=$password"), "${pdf.baseName}-unlocked.pdf") {
            "Removed the protection."
        }
    }
}

class MetadataOptions : ToolOptions {
    companion object {
        val fields = listOf(
            "title" to "Title",
            "author" to "Author",
            "subject" to "Subject",
            "keywords" to "Keywords",
            "creator" to "Creator (the original application)",
            "producer" to "Producer (the PDF library)",
        )
    }

    val values = mutableStateMapOf<String, String>()

    override fun reset(session: ToolSession) {
        values.clear()
        for ((key, _) in fields) values[key] = session.pdf?.info?.get("metadata")?.get(key)?.string ?: ""
    }

    @Composable
    override fun Panel(session: ToolSession) {
        for ((key, label) in fields) {
            TextInput(label, values[key] ?: "", { values[key] = it }, tag = "metadata.$key")
        }
    }

    override fun problem(session: ToolSession) = session.keepProblem

    override suspend fun run(session: ToolSession): ToolResult {
        val pdf = pdfOf(session)
        val args = listOf("metadata", pdf.file.path, "OUT") + fields.map { (key, _) -> "--$key=${values[key] ?: ""}" } + session.keepArgs
        val note = session.removedNote
        return runToFile(session, args, "${pdf.baseName}-metadata.pdf") { "Saved the details$note." }
    }
}

class GrayscaleOptions : ToolOptions {
    var images by mutableStateOf(true)

    @Composable
    override fun Panel(session: ToolSession) {
        SwitchRow("Convert images too", images) { images = it }
        Note("Text stays real text. Gradients and spot colours are left as they are.")
    }

    override fun card(session: ToolSession, item: PageItem, index: Int) = CardDecor(gray = true)

    override fun problem(session: ToolSession) = session.keepProblem

    override suspend fun run(session: ToolSession): ToolResult {
        val pdf = pdfOf(session)
        val args = listOf("grayscale", pdf.file.path, "OUT") + session.keepArgs + if (images) emptyList() else listOf("--text-only")
        val note = session.removedNote
        return runToFile(session, args, "${pdf.baseName}-grayscale.pdf") { json ->
            "Converted ${plural(json["streams"].int, "content stream")} and ${plural(json["images"].int, "image")}$note."
        }
    }
}
