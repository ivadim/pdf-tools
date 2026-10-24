// The shared tool screen: choose files, set options, check the pages, run.

package io.github.ivadim.pdftools

import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.union
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyGridScope
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.automirrored.rounded.NoteAdd
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.Cancel
import androidx.compose.material.icons.rounded.Description
import androidx.compose.material.icons.rounded.Folder
import androidx.compose.material.icons.rounded.Lock
import androidx.compose.material.icons.rounded.PhotoLibrary
import androidx.compose.material.icons.rounded.Warning
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ToolScreen(session: ToolSession, onBack: () -> Unit) {
    val scope = rememberCoroutineScope()
    var viewing by remember { mutableStateOf<PageItem?>(null) }
    val pickers = rememberPickers(session, scope)
    val empty = when (session.tool.kind) {
        Tool.Kind.Single -> session.pdf == null
        Tool.Kind.Merge -> session.files.isEmpty()
        Tool.Kind.Images -> session.images.isEmpty()
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(session.tool.title) },
                navigationIcon = { IconButton(onBack) { Icon(Icons.AutoMirrored.Rounded.ArrowBack, "Back") } },
                actions = { if (!empty) AddButton(session, pickers) },
                colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.surface),
            )
        },
        bottomBar = { if (!empty) RunBar(session, scope) },
    ) { padding ->
        Box(Modifier.fillMaxSize().padding(padding).let { if (empty) it.imePadding() else it }) {
            if (empty) {
                EmptyTool(session, pickers)
            } else {
                BoxWithConstraints(Modifier.fillMaxSize()) {
                    if (maxWidth >= 720.dp) {
                        Row(Modifier.fillMaxSize()) {
                            Column(Modifier.width(380.dp).fillMaxHeight().verticalScroll(rememberScrollState()).padding(16.dp)) {
                                OptionsPanel(session)
                            }
                            VerticalDivider()
                            PageGrid(session, Modifier.weight(1f), onView = { viewing = it })
                        }
                    } else {
                        PageGrid(session, Modifier.fillMaxSize(), header = { OptionsPanel(session) }, onView = { viewing = it })
                    }
                }
            }
            if (session.loading) {
                Card(Modifier.align(Alignment.Center), colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerHigh)) {
                    Row(Modifier.padding(20.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                        CircularProgressIndicator(Modifier.size(24.dp), strokeWidth = 3.dp)
                        Text("Opening…")
                    }
                }
            }
        }
    }

    session.result?.let { ResultScreen(it, onClose = { session.result = null }) }
    viewing?.let { PageViewer(it, gray = session.tool == Tool.Grayscale, onClose = { viewing = null }) }
    session.error?.let { message ->
        AlertDialog(
            modifier = Modifier.testIds(),
            onDismissRequest = { session.error = null },
            title = { Text("Couldn’t do that") },
            text = { Text(message) },
            confirmButton = { TextButton({ session.error = null }) { Text("OK") } },
        )
    }
    session.lockedFile?.let { file ->
        AlertDialog(
            modifier = Modifier.testIds(),
            onDismissRequest = { session.lockedFile = null },
            icon = { Icon(Icons.Rounded.Lock, null) },
            title = { Text("This PDF has a password") },
            text = { Text("Remove the password with Unlock first; you need to know it.") },
            confirmButton = { TextButton({ session.switchToUnlock(file) }) { Text("Unlock it") } },
            dismissButton = { TextButton({ session.lockedFile = null }) { Text("Cancel") } },
        )
    }
}

/** The system's pickers for PDFs, image files and photos. */
class Pickers(val files: () -> Unit, val photos: () -> Unit)

@Composable
private fun rememberPickers(session: ToolSession, scope: CoroutineScope): Pickers {
    fun add(sources: List<Any>) {
        if (sources.isEmpty()) return
        scope.launch {
            if (session.tool.kind == Tool.Kind.Images) ImagesOptions.add(session, sources) else session.open(sources)
        }
    }
    val one = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri: Uri? -> add(listOfNotNull(uri)) }
    val many = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris: List<Uri> -> add(uris) }
    val photos = rememberLauncherForActivityResult(ActivityResultContracts.PickMultipleVisualMedia()) { uris: List<Uri> -> add(uris) }
    return Pickers(
        files = {
            val test = TestHooks.files
            when {
                test != null -> add(if (session.tool.kind == Tool.Kind.Single) test.take(1) else test)
                session.tool.kind == Tool.Kind.Images -> many.launch(arrayOf("image/jpeg", "image/png", "image/*"))
                session.tool.kind == Tool.Kind.Merge -> many.launch(arrayOf("application/pdf"))
                else -> one.launch(arrayOf("application/pdf"))
            }
        },
        photos = {
            val test = TestHooks.files
            if (test != null) add(test) else photos.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly))
        },
    )
}

@Composable
private fun AddButton(session: ToolSession, pickers: Pickers) {
    when (session.tool.kind) {
        Tool.Kind.Single -> IconButton(pickers.files, Modifier.testTag("add")) { Icon(Icons.AutoMirrored.Rounded.NoteAdd, "Other PDF") }
        Tool.Kind.Merge -> IconButton(pickers.files, Modifier.testTag("add")) { Icon(Icons.Rounded.Add, "Add PDFs") }
        Tool.Kind.Images -> {
            var open by remember { mutableStateOf(false) }
            Box {
                IconButton({ open = true }, Modifier.testTag("add")) { Icon(Icons.Rounded.Add, "Add images") }
                DropdownMenu(open, { open = false }) {
                    DropdownMenuItem({ Text("Photos") }, { open = false; pickers.photos() }, leadingIcon = { Icon(Icons.Rounded.PhotoLibrary, null) })
                    DropdownMenuItem({ Text("Files") }, { open = false; pickers.files() }, leadingIcon = { Icon(Icons.Rounded.Folder, null) })
                }
            }
        }
    }
}

@Composable
private fun EmptyTool(session: ToolSession, pickers: Pickers) {
    Column(
        Modifier.fillMaxSize().padding(32.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        ToolIcon(session.tool, 64)
        Text(session.tool.title, style = MaterialTheme.typography.headlineSmall, modifier = Modifier.padding(top = 16.dp, bottom = 8.dp))
        Text(
            session.tool.summary,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = androidx.compose.ui.text.style.TextAlign.Center,
            modifier = Modifier.widthIn(max = 360.dp),
        )
        Spacer(Modifier.size(24.dp))
        when (session.tool.kind) {
            Tool.Kind.Single -> Button(pickers.files, Modifier.testTag("choose")) { Text("Choose a PDF") }
            Tool.Kind.Merge -> Button(pickers.files, Modifier.testTag("choose")) { Text("Choose PDFs") }
            Tool.Kind.Images -> {
                Button(pickers.photos, Modifier.testTag("choose")) { Text("Choose photos") }
                OutlinedButton(pickers.files, Modifier.padding(top = 8.dp).testTag("choose.files")) { Text("Choose image files") }
            }
        }
    }
}

@Composable
private fun OptionsPanel(session: ToolSession) {
    Card(
        shape = RoundedCornerShape(14.dp),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.card),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
            FilesHeader(session)
            session.options.Panel(session)
        }
    }
}

/** The open file(s), with page counts and notes. */
@Composable
private fun FilesHeader(session: ToolSession) {
    when (session.tool.kind) {
        Tool.Kind.Single -> session.pdf?.let { pdf ->
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Icon(if (pdf.locked) Icons.Rounded.Lock else Icons.Rounded.Description, null, tint = session.tool.tint, modifier = Modifier.size(30.dp))
                Column {
                    Text(pdf.name, style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
                    Text(
                        if (pdf.locked) "Locked · ${formatBytes(pdf.size)}" else "${plural(pdf.pageCount, "page")} · ${formatBytes(pdf.size)}",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            if (pdf.repaired) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Icon(Icons.Rounded.Warning, null, tint = MaterialTheme.colorScheme.warning, modifier = Modifier.size(18.dp))
                    Text(
                        "This file is damaged; only the pages that could be recovered are shown.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.warning,
                    )
                }
            }
        }
        Tool.Kind.Merge -> Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
            session.files.forEachIndexed { index, file ->
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    Box(Modifier.size(10.dp).background(fileColor(index), CircleShape))
                    Text(file.name, Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
                    Text(plural(file.pageCount, "page"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    IconButton({ session.removeFile(file) }, Modifier.size(32.dp)) {
                        Icon(Icons.Rounded.Cancel, "Remove ${file.name}", tint = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.size(20.dp))
                    }
                }
            }
        }
        Tool.Kind.Images -> Text(plural(session.images.size, "image"), style = MaterialTheme.typography.titleMedium)
    }
}

/** The page cards (or images), with the tool's buttons above them; on phones the options scroll with them. */
@Composable
private fun PageGrid(session: ToolSession, modifier: Modifier, header: (@Composable () -> Unit)? = null, onView: (PageItem) -> Unit) {
    val reorder = remember(session) { Reorder(session) }
    LazyVerticalGrid(
        columns = GridCells.Adaptive(150.dp),
        modifier = modifier.testTag("pages"),
        contentPadding = PaddingValues(16.dp),
        horizontalArrangement = Arrangement.spacedBy(14.dp),
        verticalArrangement = Arrangement.spacedBy(18.dp),
    ) {
        if (header != null) item(span = { GridItemSpan(maxLineSpan) }, key = "options") { header() }
        val actions = session.options.gridActions(session)
        if (actions.isNotEmpty()) {
            item(span = { GridItemSpan(maxLineSpan) }, key = "actions") {
                Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    for (action in actions) {
                        FilledTonalButton(action.action, enabled = !action.disabled) {
                            Icon(action.icon, null, Modifier.size(18.dp))
                            Spacer(Modifier.width(6.dp))
                            Text(action.label)
                        }
                    }
                }
            }
        }
        cards(session, reorder, onView)
    }
}

private fun LazyGridScope.cards(session: ToolSession, reorder: Reorder, onView: (PageItem) -> Unit) {
    val options = session.options
    if (session.tool.kind == Tool.Kind.Images && options is ImagesOptions) {
        itemsIndexed(session.images, key = { _, item -> item.id }) { index, item ->
            ImageCard(session, options, item, index, Modifier.animateItem())
        }
    } else {
        itemsIndexed(session.items, key = { _, item -> item.id }) { index, item ->
            PageCard(session, item, index, options.card(session, item, index), reorder, Modifier.animateItem()) { onView(item) }
        }
    }
}

/** Dragging a page by its handle onto another page moves it there (Merge and Rearrange). */
class Reorder(private val session: ToolSession) {
    /** Each card's bounds in the window. */
    val bounds = mutableStateMapOf<String, Rect>()
    var dragging by mutableStateOf<String?>(null)
        private set
    var offset by mutableStateOf(Offset.Zero)
        private set
    /** Where the finger went down, in the window. */
    private var origin = Offset.Zero

    fun start(id: String, at: Offset) {
        dragging = id
        origin = at
        offset = Offset.Zero
    }

    fun drag(delta: Offset) {
        offset += delta
    }

    /** Drops the page on the card under the finger. */
    fun end() {
        val id = dragging ?: return
        val point = origin + offset
        val target = bounds.entries.firstOrNull { (key, rect) -> key != id && rect.contains(point) }?.key
        if (target != null) session.move(id, target)
        dragging = null
        offset = Offset.Zero
    }
}

/** What's missing, or the button that runs the tool. */
@Composable
private fun RunBar(session: ToolSession, scope: CoroutineScope) {
    val problem = session.problem
    Surface(color = MaterialTheme.colorScheme.surfaceContainer) {
        Column {
            HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
            Row(
                // Above the keyboard while typing, as on iOS, so the tool can run without closing it.
                Modifier.fillMaxWidth().windowInsetsPadding(WindowInsets.navigationBars.union(WindowInsets.ime)).padding(horizontal = 16.dp, vertical = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Text(
                    problem ?: "",
                    Modifier.weight(1f).testTag("problem"),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                Button(
                    onClick = { scope.launch { session.run() } },
                    enabled = problem == null && !session.running,
                    contentPadding = PaddingValues(horizontal = 24.dp, vertical = 12.dp),
                    modifier = Modifier.widthIn(min = 120.dp).testTag("run"),
                ) {
                    if (session.running) {
                        CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp, color = MaterialTheme.colorScheme.onPrimary)
                    } else {
                        Text(session.tool.action)
                    }
                }
            }
        }
    }
}
