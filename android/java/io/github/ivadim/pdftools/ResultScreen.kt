// A finished run: preview, rename, share or save the files.

package io.github.ivadim.pdftools

import android.content.ActivityNotFoundException
import android.content.ClipData
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.OpenInNew
import androidx.compose.material.icons.rounded.CheckCircle
import androidx.compose.material.icons.rounded.Description
import androidx.compose.material.icons.rounded.Edit
import androidx.compose.material.icons.rounded.Folder
import androidx.compose.material.icons.rounded.Share
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.content.FileProvider
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ResultScreen(result: ToolResult, onClose: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    /** The outputs, under their current names. */
    val files = remember(result) { mutableStateListOf(*result.files.toTypedArray()) }
    var preview by remember { mutableStateOf<File?>(null) }
    var renaming by remember { mutableStateOf<File?>(null) }
    var saved by remember { mutableStateOf<String?>(null) }
    var error by remember { mutableStateOf<String?>(null) }

    fun report(count: Int) {
        saved = if (count > 0) "Saved ${plural(count, "file")}." else null
    }
    val saveOne = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/pdf")) { uri: Uri? ->
        val file = files.firstOrNull() ?: return@rememberLauncherForActivityResult
        if (uri != null) scope.launch { report(OutputFiles.save(context, listOf(file), uri = uri, error = { error = it })) }
    }
    val saveMany = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree()) { tree: Uri? ->
        if (tree != null) scope.launch { report(OutputFiles.save(context, files.toList(), tree = tree, error = { error = it })) }
    }

    FullScreenDialog(onClose) {
        Scaffold(
            topBar = {
                TopAppBar(
                    title = { Text("Done") },
                    actions = { TextButton(onClose, Modifier.testTag("result.close")) { Text("Close") } },
                    colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.surface),
                )
            },
            bottomBar = {
                Surface(color = MaterialTheme.colorScheme.surfaceContainer) {
                    Column {
                        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                        Row(
                            Modifier.fillMaxWidth().navigationBarsPadding().padding(horizontal = 16.dp, vertical = 10.dp),
                            horizontalArrangement = Arrangement.spacedBy(12.dp),
                        ) {
                            OutlinedButton(
                                { if (files.size == 1) saveOne.launch(files[0].name) else saveMany.launch(null) },
                                Modifier.weight(1f).testTag("result.save"),
                            ) {
                                Icon(Icons.Rounded.Folder, null, Modifier.size(18.dp))
                                Spacer(Modifier.width(8.dp))
                                Text("Save")
                            }
                            Button({ OutputFiles.share(context, files.toList()) }, Modifier.weight(1f).testTag("result.share")) {
                                Icon(Icons.Rounded.Share, null, Modifier.size(18.dp))
                                Spacer(Modifier.width(8.dp))
                                Text("Share")
                            }
                        }
                    }
                }
            },
        ) { padding ->
            LazyColumn(
                Modifier.fillMaxSize().padding(padding),
                contentPadding = PaddingValues(16.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                item {
                    Group {
                        Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                            Icon(Icons.Rounded.CheckCircle, null, tint = MaterialTheme.colorScheme.success)
                            Text(result.message, Modifier.testTag("result.message"), color = MaterialTheme.colorScheme.success)
                        }
                    }
                }
                item {
                    Text(
                        if (files.size == 1) "FILE" else "FILES",
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(start = 16.dp, bottom = 0.dp),
                    )
                    Group {
                        Column {
                            files.forEachIndexed { index, file ->
                                if (index > 0) HorizontalDivider(Modifier.padding(start = 56.dp), color = MaterialTheme.colorScheme.outlineVariant)
                                FileRow(file, onOpen = { preview = file }, onRename = { renaming = file }, tag = "result.file.${index + 1}")
                            }
                        }
                    }
                }
                saved?.let { message ->
                    item {
                        Group {
                            Row(Modifier.padding(16.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                                Icon(Icons.Rounded.Folder, null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
                                Text(message, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            }
                        }
                    }
                }
            }
        }

        preview?.let { file -> PdfPreview(file, onClose = { preview = null }) }
        renaming?.let { file ->
            RenameDialog(
                file,
                onRename = { name ->
                    try {
                        val renamed = OutputFiles.rename(file, name)
                        files[files.indexOf(file)] = renamed
                    } catch (e: EngineException) {
                        error = e.message
                    }
                    renaming = null
                },
                onCancel = { renaming = null },
            )
        }
        error?.let { message ->
            AlertDialog(
                modifier = Modifier.testIds(),
                onDismissRequest = { error = null },
                title = { Text("Couldn’t do that") },
                text = { Text(message) },
                confirmButton = { TextButton({ error = null }) { Text("OK") } },
            )
        }
    }
}

/** A rounded card on the grouped background, like an iOS list section. */
@Composable
fun Group(content: @Composable () -> Unit) {
    Card(
        shape = RoundedCornerShape(12.dp),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.card),
        modifier = Modifier.fillMaxWidth(),
    ) { content() }
}

@Composable
private fun FileRow(file: File, onOpen: () -> Unit, onRename: () -> Unit, tag: String) {
    Row(
        Modifier.fillMaxWidth().clickable(onClick = onOpen).padding(start = 16.dp, end = 4.dp, top = 10.dp, bottom = 10.dp).testTag(tag),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Icon(Icons.Rounded.Description, null, tint = MaterialTheme.colorScheme.primary)
        Column(Modifier.weight(1f)) {
            Text(file.name, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
            Text(formatBytes(file.length()), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        IconButton(onRename, Modifier.testTag("$tag.rename")) { Icon(Icons.Rounded.Edit, "Rename ${file.name}", tint = MaterialTheme.colorScheme.primary) }
    }
}

@Composable
private fun RenameDialog(file: File, onRename: (String) -> Unit, onCancel: () -> Unit) {
    var name by remember { mutableStateOf(TextFieldValue(file.nameWithoutExtension, TextRange(file.nameWithoutExtension.length))) }
    AlertDialog(
        modifier = Modifier.testIds(),
        onDismissRequest = onCancel,
        title = { Text("Rename") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text("“${file.name}” will keep its .${file.extension} extension.", style = MaterialTheme.typography.bodyMedium)
                OutlinedTextField(name, { name = it }, singleLine = true, label = { Text("Name") }, modifier = Modifier.testTag("rename.name"))
            }
        },
        confirmButton = { TextButton({ onRename(name.text) }, Modifier.testTag("rename.ok")) { Text("Rename") } },
        dismissButton = { TextButton(onCancel) { Text("Cancel") } },
    )
}

/** Every page of an output, drawn by the engine; "Open with" hands it to another app. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun PdfPreview(file: File, onClose: () -> Unit) {
    val context = LocalContext.current
    var pages by remember { mutableStateOf<Int?>(null) }
    LaunchedEffect(file) {
        pages = runCatching { Engine.run(listOf("info", file.path))["pages"].int }.getOrDefault(0)
    }
    FullScreenDialog(onClose) {
        Scaffold(
            topBar = {
                TopAppBar(
                    title = { Text(file.name, maxLines = 1, overflow = TextOverflow.MiddleEllipsis) },
                    actions = {
                        IconButton({ OutputFiles.open(context, file) }) { Icon(Icons.AutoMirrored.Rounded.OpenInNew, "Open with") }
                        TextButton(onClose, Modifier.testTag("preview.done")) { Text("Done") }
                    },
                )
            },
        ) { padding ->
            val count = pages
            if (count == null) {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
            } else {
                BoxWithConstraints(Modifier.fillMaxSize().padding(padding)) {
                    val width = with(LocalDensity.current) { (maxWidth - 32.dp).roundToPx() }
                    LazyColumn(contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                        items((1..count).toList()) { page -> PreviewPage(file, page, width) }
                    }
                }
            }
        }
    }
}

@Composable
private fun PreviewPage(file: File, page: Int, width: Int) {
    var drawn by remember { mutableStateOf<RenderedPage?>(null) }
    LaunchedEffect(file, page) { drawn = runCatching { Engine.render(file, page, width) }.getOrNull() }
    val current = drawn
    val ratio = current?.let { it.bitmap.width.toFloat() / it.bitmap.height } ?: (1 / 1.414f)
    Box(Modifier.fillMaxWidth().aspectRatio(ratio).shadow(2.dp).background(Color.White), contentAlignment = Alignment.Center) {
        if (current != null) Image(current.bitmap.asImageBitmap(), "Page $page", Modifier.fillMaxSize()) else CircularProgressIndicator(Modifier.size(24.dp))
    }
}

object OutputFiles {
    /**
     * Renames an output in its folder, keeping its extension (typing it again
     * is fine). Names can't contain “/” or “:” or start with a dot.
     */
    fun rename(file: File, name: String): File {
        val ext = file.extension
        var base = name.trim()
        if (ext.isNotEmpty() && base.lowercase().endsWith(".${ext.lowercase()}")) base = base.dropLast(ext.length + 1).trim()
        if (base.isEmpty() || base.startsWith(".") || "/" in base || ":" in base) throw EngineException("Enter a name without “/” or “:”.")
        val target = File(file.parentFile, if (ext.isEmpty()) base else "$base.$ext")
        if (target.name == file.name) return file
        if (target.exists() && !target.name.equals(file.name, ignoreCase = true)) throw EngineException("There’s already a file called “${target.name}”.")
        if (!file.renameTo(target)) throw EngineException("Couldn’t rename “${file.name}”.")
        return target
    }

    private fun uri(context: Context, file: File): Uri = FileProvider.getUriForFile(context, "${context.packageName}.files", file)

    fun share(context: Context, files: List<File>) {
        val uris = ArrayList(files.map { uri(context, it) })
        val intent = if (uris.size == 1) {
            Intent(Intent.ACTION_SEND).putExtra(Intent.EXTRA_STREAM, uris[0])
        } else {
            Intent(Intent.ACTION_SEND_MULTIPLE).putParcelableArrayListExtra(Intent.EXTRA_STREAM, uris)
        }
        intent.setType("application/pdf").addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        // Lets the share sheet read the files for its preview.
        intent.clipData = ClipData.newRawUri(files[0].name, uris[0]).apply { uris.drop(1).forEach { addItem(ClipData.Item(it)) } }
        context.startActivity(Intent.createChooser(intent, null))
    }

    fun open(context: Context, file: File) {
        val intent = Intent(Intent.ACTION_VIEW).setDataAndType(uri(context, file), "application/pdf").addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        try {
            context.startActivity(intent)
        } catch (e: ActivityNotFoundException) {
            context.startActivity(Intent.createChooser(intent, null))
        }
    }

    /** Copies outputs to a document the person chose, or into a folder they chose; returns how many were saved. */
    suspend fun save(context: Context, files: List<File>, uri: Uri? = null, tree: Uri? = null, error: (String) -> Unit): Int =
        withContext(Dispatchers.IO) {
            val resolver = context.contentResolver
            var count = 0
            try {
                if (uri != null) {
                    resolver.openOutputStream(uri, "wt")?.use { out -> files[0].inputStream().use { it.copyTo(out) } }
                    count = 1
                } else if (tree != null) {
                    val folder = DocumentsContract.buildDocumentUriUsingTree(tree, DocumentsContract.getTreeDocumentId(tree))
                    for (file in files) {
                        val target = DocumentsContract.createDocument(resolver, folder, "application/pdf", file.name) ?: continue
                        resolver.openOutputStream(target)?.use { out -> file.inputStream().use { it.copyTo(out) } }
                        count++
                    }
                }
            } catch (e: Exception) {
                withContext(Dispatchers.Main) { error(e.message ?: "Couldn’t save the files.") }
            }
            count
        }
}
