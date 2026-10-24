// A page card: the page drawn by the engine, the tool's overlay, a checkbox to
// leave it out and the tool's buttons. Also the full-page viewer.

package io.github.ivadim.pdftools

import android.util.LruCache
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.AddCircleOutline
import androidx.compose.material.icons.rounded.CheckCircle
import androidx.compose.material.icons.rounded.DragHandle
import androidx.compose.material.icons.rounded.RadioButtonUnchecked
import androidx.compose.material.icons.rounded.RemoveCircleOutline
import androidx.compose.material.icons.rounded.Visibility
import androidx.compose.material.icons.rounded.Warning
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import kotlin.math.ceil

@Composable
fun PageCard(
    session: ToolSession,
    item: PageItem,
    index: Int,
    decor: CardDecor,
    reorder: Reorder,
    modifier: Modifier,
    open: () -> Unit,
) {
    val reorderable = session.tool == Tool.Merge || session.tool == Tool.Rearrange
    val dimmed = !item.keep || decor.dimmed
    val dragging = reorder.dragging == item.id
    var menu by remember { mutableStateOf(false) }
    val toggleKeep = { session.update(index) { it.copy(keep = !it.keep) } }

    Column(
        modifier
            .zIndex(if (dragging) 1f else 0f)
            .graphicsLayer {
                if (dragging) {
                    translationX = reorder.offset.x
                    translationY = reorder.offset.y
                    scaleX = 1.04f
                    scaleY = 1.04f
                }
            }
            .onGloballyPositioned { reorder.bounds[item.id] = it.boundsInWindow() }
            .semantics(mergeDescendants = false) { contentDescription = "Page ${item.page}" },
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Box(Modifier.fillMaxWidth().aspectRatio(5f / 6f)) {
            val alpha by animateFloatAsState(if (dimmed) 0.35f else 1f, label = "dim")
            PageImage(session, item, item.rotation, decor.gray, Modifier.fillMaxSize().alpha(alpha), decor.partColor) { points ->
                Box(Modifier.fillMaxSize()) {
                    if (decor.onTap == null) decor.overlay?.invoke(points)
                    BoxWithConstraints(
                        Modifier
                            .fillMaxSize()
                            .testTag("page.${index + 1}")
                            .semantics { role = Role.Button }
                            .pointerInput(decor.onTap) {
                                detectTapGestures(
                                    onLongPress = { menu = true },
                                    onTap = { point ->
                                        val onTap = decor.onTap
                                        if (onTap != null) onTap(Offset(point.x / size.width, point.y / size.height)) else open()
                                    },
                                )
                            },
                    ) {}
                    // Placed signatures stay on top of the tap area, so they can be dragged.
                    if (decor.onTap != null) decor.overlay?.invoke(points)
                }
            }
            if (session.tool.leavesOutPages) {
                IconButton(toggleKeep, Modifier.align(Alignment.TopStart).size(40.dp).testTag("keep.${index + 1}")) {
                    Icon(
                        if (item.keep) Icons.Rounded.CheckCircle else Icons.Rounded.RadioButtonUnchecked,
                        if (item.keep) "Leave out page ${item.page}" else "Include page ${item.page}",
                        tint = if (item.keep) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outline,
                        modifier = Modifier.background(if (item.keep) Color.White else Color.White.copy(alpha = 0.7f), CircleShape),
                    )
                }
            }
            DropdownMenu(menu, { menu = false }) {
                DropdownMenuItem({ Text("View") }, { menu = false; open() }, leadingIcon = { Icon(Icons.Rounded.Visibility, null) })
                if (session.tool.leavesOutPages) {
                    DropdownMenuItem(
                        { Text(if (item.keep) "Leave out" else "Include") },
                        { menu = false; toggleKeep() },
                        leadingIcon = { Icon(if (item.keep) Icons.Rounded.RemoveCircleOutline else Icons.Rounded.AddCircleOutline, null) },
                    )
                }
                for (control in decor.controls) {
                    DropdownMenuItem({ Text(control.label) }, { menu = false; control.action() }, enabled = !control.disabled, leadingIcon = { Icon(control.icon, null) })
                }
            }
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            if (reorderable) {
                var handle by remember { mutableStateOf(Offset.Zero) }
                Icon(
                    Icons.Rounded.DragHandle,
                    "Drag to move page ${item.page}",
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier
                        .size(28.dp)
                        .padding(2.dp)
                        .onGloballyPositioned { handle = it.boundsInWindow().topLeft }
                        .pointerInput(item.id) {
                            detectDragGestures(
                                onDragStart = { reorder.start(item.id, handle + it) },
                                onDrag = { change, delta ->
                                    change.consume()
                                    reorder.drag(delta)
                                },
                                onDragEnd = { reorder.end() },
                                onDragCancel = { reorder.end() },
                            )
                        },
                )
            }
            Text("${item.page}", style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            if (session.tool == Tool.Merge) {
                val file = session.files.indexOfFirst { it === item.file }
                if (file >= 0) Box(Modifier.padding(start = 4.dp).size(7.dp).background(fileColor(file), CircleShape))
            }
            Spacer(Modifier.weight(1f))
            for (control in decor.controls) {
                IconButton(control.action, Modifier.size(30.dp), enabled = !control.disabled) {
                    Icon(control.icon, control.label, Modifier.size(20.dp))
                }
            }
        }
    }
}

/** A page drawn by the engine, fitted into the space it's given, with an overlay sized to it. */
@Composable
fun PageImage(
    session: ToolSession,
    item: PageItem,
    rotation: Int,
    gray: Boolean,
    modifier: Modifier,
    border: Color? = null,
    overlay: @Composable (Size) -> Unit,
) {
    BoxWithConstraints(modifier, contentAlignment = Alignment.Center) {
        val density = LocalDensity.current
        // Rounded up, so small layout changes reuse the same render.
        val width = with(density) { (ceil(maxWidth.value / 40) * 40).dp.roundToPx() }
        val key = "${item.file.file.path}|${item.page}|$width|$rotation|$gray"
        var page by remember(key) { mutableStateOf(Thumbnails.cache.get(key)) }
        var failed by remember(key) { mutableStateOf(false) }
        LaunchedEffect(key) {
            if (page == null) {
                try {
                    val rendered = Engine.render(item.file.file, item.page, width, rotation, gray)
                    Thumbnails.cache.put(key, rendered)
                    page = rendered
                } catch (e: EngineException) {
                    failed = true
                }
            }
            val drawn = page
            if (drawn != null && rotation == 0) session.pagePoints[ToolSession.key(item.file, item.page)] = drawn.points
        }
        val drawn = page
        when {
            drawn != null -> {
                val ratio = drawn.bitmap.width.toFloat() / drawn.bitmap.height
                Box(
                    Modifier
                        .aspectRatio(ratio, matchHeightConstraintsFirst = ratio < maxWidth / maxHeight)
                        .shadow(2.dp)
                        .background(Color.White)
                        .let { if (border != null) it.border(3.dp, border) else it },
                ) {
                    Image(drawn.bitmap.asImageBitmap(), null, Modifier.matchParentSize(), contentScale = ContentScale.Fit)
                    Box(Modifier.matchParentSize()) { overlay(drawn.points) }
                }
            }
            failed -> Icon(Icons.Rounded.Warning, "Couldn’t draw this page", tint = MaterialTheme.colorScheme.onSurfaceVariant)
            else -> CircularProgressIndicator(Modifier.size(24.dp), strokeWidth = 2.dp)
        }
    }
}

object Thumbnails {
    /** Capped by the pixels' size: an eighth of the app's memory, at most 150 MB (about 100 phone-sized pages). */
    val cache = object : LruCache<String, RenderedPage>(minOf(150L shl 20, Runtime.getRuntime().maxMemory() / 8).toInt()) {
        override fun sizeOf(key: String, value: RenderedPage) = value.bitmap.byteCount
    }
}

/** A page at full size; pinch or double-tap to zoom. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PageViewer(item: PageItem, gray: Boolean, onClose: () -> Unit) {
    FullScreenDialog(onClose) {
        Scaffold(
            topBar = {
                TopAppBar(
                    title = { Text("Page ${item.page}") },
                    actions = { TextButton(onClose, Modifier.testTag("viewer.done")) { Text("Done") } },
                )
            },
        ) { padding ->
            BoxWithConstraints(Modifier.fillMaxSize().padding(padding), contentAlignment = Alignment.Center) {
                val width = with(LocalDensity.current) { minOf(3000, maxWidth.roundToPx() * 2) }
                var page by remember { mutableStateOf<RenderedPage?>(null) }
                var failed by remember { mutableStateOf(false) }
                LaunchedEffect(item.id) {
                    try {
                        page = Engine.render(item.file.file, item.page, width, item.rotation, gray)
                    } catch (e: EngineException) {
                        failed = true
                    }
                }
                val drawn = page
                when {
                    drawn != null -> ZoomableImage(drawn)
                    failed -> Text("Couldn’t draw this page", color = MaterialTheme.colorScheme.onSurfaceVariant)
                    else -> CircularProgressIndicator()
                }
            }
        }
    }
}

@Composable
private fun ZoomableImage(page: RenderedPage) {
    var scale by remember { mutableFloatStateOf(1f) }
    var pan by remember { mutableStateOf(Offset.Zero) }
    Box(
        Modifier
            .fillMaxSize()
            .pointerInput(Unit) {
                detectTapGestures(onDoubleTap = { point ->
                    if (scale > 1f) {
                        scale = 1f
                        pan = Offset.Zero
                    } else {
                        scale = 2.5f
                        pan = (Offset(size.width / 2f, size.height / 2f) - point) * 2.5f
                    }
                })
            }
            .pointerInput(Unit) {
                detectTransformGestures { _, move, zoom, _ ->
                    scale = (scale * zoom).coerceIn(1f, 6f)
                    pan = if (scale == 1f) Offset.Zero else pan + move
                }
            },
        contentAlignment = Alignment.Center,
    ) {
        val ratio = page.bitmap.width.toFloat() / page.bitmap.height
        BoxWithConstraints(Modifier.fillMaxSize().padding(16.dp), contentAlignment = Alignment.Center) {
            Image(
                page.bitmap.asImageBitmap(),
                "Page",
                Modifier
                    .aspectRatio(ratio, matchHeightConstraintsFirst = ratio < maxWidth / maxHeight)
                    .graphicsLayer {
                        scaleX = scale
                        scaleY = scale
                        translationX = pan.x
                        translationY = pan.y
                    }
                    .shadow(3.dp)
                    .background(Color.White),
                contentScale = ContentScale.Fit,
            )
        }
    }
}
