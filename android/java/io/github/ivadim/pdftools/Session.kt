// One tool's screen: its files, its page cards and its options.

package io.github.ivadim.pdftools

import android.graphics.Bitmap
import android.net.Uri
import android.provider.OpenableColumns
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import java.io.File
import java.util.UUID

/** An opened PDF, copied into the app's workspace. */
class PdfFile(val file: File, val name: String, val info: Json) {
    val id: String = UUID.randomUUID().toString()
    val pageCount get() = info["pages"].int
    val locked get() = info["needs_password"].bool
    val encrypted get() = info["encrypted"].bool
    val repaired get() = info["repaired"].bool
    val size get() = info["file_size"].long
    val baseName get() = name.substringBeforeLast('.', name)

    companion object {
        /** Copies a picked file into the workspace and inspects it. */
        suspend fun open(source: Uri): PdfFile {
            val (local, name) = Workspace.importFile(source)
            return PdfFile(local, name, Engine.run(listOf("info", local.path)))
        }

        /** A file already on the device (the UI tests' fixtures). */
        suspend fun open(source: File): PdfFile {
            val local = Workspace.newFolder("Inputs").resolve(source.name)
            source.copyTo(local)
            return PdfFile(local, source.name, Engine.run(listOf("info", local.path)))
        }
    }
}

/** One page card. */
data class PageItem(
    val file: PdfFile,
    /** 1-based page in `file`. */
    val page: Int,
    val rotation: Int = 0,
    val keep: Boolean = true,
    val id: String = UUID.randomUUID().toString(),
)

/** An image for Images to PDF. */
data class ImageItem(
    val file: File,
    val name: String,
    /** As displayed (camera rotation applied), in pixels. */
    val size: Size,
    val thumbnail: Bitmap,
    val rotation: Int = 0,
    val id: String = UUID.randomUUID().toString(),
)

/** What a finished run produced. */
class ToolResult(val message: String, val files: List<File>)

class ToolSession(tool: Tool) {
    var tool by mutableStateOf(tool)
    var options by mutableStateOf(tool.makeOptions())
    val files = mutableStateListOf<PdfFile>()
    val items = mutableStateListOf<PageItem>()
    val images = mutableStateListOf<ImageItem>()
    /** Each page's size in points, once its thumbnail is drawn (key: file id + page). */
    val pagePoints = mutableStateMapOf<String, Size>()
    var loading by mutableStateOf(false)
    var running by mutableStateOf(false)
    var result by mutableStateOf<ToolResult?>(null)
    var error by mutableStateOf<String?>(null)
    /** A locked file opened in a single-file tool other than Unlock. */
    var lockedFile by mutableStateOf<PdfFile?>(null)

    val pdf: PdfFile? get() = files.firstOrNull()
    val kept: List<PageItem> get() = items.filter { it.keep }

    fun points(item: PageItem): Size? = pagePoints[key(item.file, item.page)]

    // Files

    suspend fun open(sources: List<Any>) {
        loading = true
        try {
            for (source in sources) {
                val name = (source as? File)?.name ?: Workspace.displayName(source as Uri)
                try {
                    val file = if (source is File) PdfFile.open(source) else PdfFile.open(source as Uri)
                    if (file.locked && tool != Tool.Unlock) {
                        if (tool.kind == Tool.Kind.Merge) {
                            // Switching to Unlock would drop the other files chosen here.
                            error = "${file.name} has a password, so it was left out. Remove the password with Unlock first."
                        } else {
                            lockedFile = file
                        }
                        continue
                    }
                    if (tool.kind == Tool.Kind.Merge) {
                        files += file
                        items += newItems(file)
                    } else {
                        files.forEach { Engine.forget(it.file) }
                        files.clear()
                        files += file
                        items.clear()
                        items += newItems(file)
                        options.reset(this)
                    }
                } catch (e: Exception) {
                    error = "$name: ${e.message}"
                }
            }
        } finally {
            loading = false
        }
    }

    fun newItems(file: PdfFile): List<PageItem> =
        if (file.locked) emptyList() else (1..file.pageCount).map { PageItem(file, it) }

    fun resetItems() {
        val fresh = files.flatMap(::newItems)
        items.clear()
        items += fresh
    }

    fun removeFile(file: PdfFile) {
        files.remove(file)
        items.removeAll { it.file === file }
        Engine.forget(file.file)
    }

    /** Opens the locked file in Unlock instead. */
    fun switchToUnlock(file: PdfFile) {
        tool = Tool.Unlock
        options = Tool.Unlock.makeOptions()
        files.clear()
        files += file
        items.clear()
        lockedFile = null
    }

    fun move(index: Int, by: Int) {
        val target = index + by
        if (index !in items.indices || target !in items.indices) return
        items[index] = items[target].also { items[target] = items[index] }
    }

    /** Moves the dragged card to where another one is. */
    fun move(id: String, to: String) {
        val from = items.indexOfFirst { it.id == id }
        val target = items.indexOfFirst { it.id == to }
        if (from < 0 || target < 0 || from == target) return
        items.add(target, items.removeAt(from))
    }

    fun update(index: Int, change: (PageItem) -> PageItem) {
        if (index in items.indices) items[index] = change(items[index])
    }

    // Pages left out

    /** `--keep` when some pages are left out. */
    val keepArgs: List<String>
        get() = kept.let { kept -> if (kept.size == items.size) emptyList() else listOf("--keep=${formatList(kept.map { it.page })}") }

    val keepProblem: String? get() = if (kept.isEmpty()) "Keep at least one page." else null

    val removedNote: String
        get() = (items.size - kept.size).let { if (it > 0) " (${plural(it, "page")} left out)" else "" }

    // Running

    val problem: String?
        get() = if (tool.kind == Tool.Kind.Single && pdf == null) "Choose a PDF." else options.problem(this)

    suspend fun run() {
        if (problem != null) return
        running = true
        try {
            result = options.run(this)
        } catch (e: Exception) {
            error = e.message ?: "Something went wrong."
        } finally {
            running = false
        }
    }

    /** A new folder for one run's outputs. */
    fun outputFolder(): File = Workspace.newFolder("Outputs")

    companion object {
        fun key(file: PdfFile, page: Int) = "${file.id}-$page"
    }
}

/** Imported files and outputs, kept in the app's cache folder. */
object Workspace {
    val root: File get() = File(App.context.cacheDir, "Workspace")

    fun newFolder(kind: String): File = File(root, "$kind/${UUID.randomUUID()}").apply { mkdirs() }

    /** The name a picked document shows in the picker. */
    fun displayName(uri: Uri): String =
        App.context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
            if (cursor.moveToFirst()) cursor.getString(0) else null
        } ?: uri.lastPathSegment?.substringAfterLast('/') ?: "file"

    /** Copies a picked document into the workspace; returns it and its name. */
    fun importFile(source: Uri): Pair<File, String> {
        val name = safeName(displayName(source))
        val local = File(newFolder("Inputs"), name)
        val input = App.context.contentResolver.openInputStream(source) ?: throw EngineException("couldn’t read the file")
        input.use { from -> local.outputStream().use { from.copyTo(it) } }
        return local to name
    }

    fun write(data: ByteArray, name: String): File = File(newFolder("Inputs"), name).apply { writeBytes(data) }

    /** Clears what earlier launches left behind. */
    fun clean() {
        root.deleteRecursively()
    }

    private fun safeName(name: String) = name.replace('/', '_').ifBlank { "file" }
}
