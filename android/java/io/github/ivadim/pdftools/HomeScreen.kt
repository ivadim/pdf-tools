// The thirteen tools as tiles, like the iOS app's home screen.

package io.github.ivadim.pdftools

import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material.icons.rounded.Code
import androidx.compose.material.icons.rounded.Coffee
import androidx.compose.material.icons.rounded.Info
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LargeTopAppBar
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HomeScreen(onOpen: (Tool) -> Unit) {
    val scroll = TopAppBarDefaults.exitUntilCollapsedScrollBehavior()
    Scaffold(
        modifier = Modifier.nestedScroll(scroll.nestedScrollConnection),
        topBar = {
            LargeTopAppBar(
                title = { Text("PDF Tools") },
                actions = { AboutMenu() },
                scrollBehavior = scroll,
                colors = TopAppBarDefaults.topAppBarColors(scrolledContainerColor = MaterialTheme.colorScheme.surfaceContainer),
            )
        },
    ) { padding ->
        LazyVerticalGrid(
            columns = GridCells.Adaptive(280.dp),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = padding.calculateTopPadding(), bottom = padding.calculateBottomPadding() + 16.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            item(span = { GridItemSpan(maxLineSpan) }) {
                Text(
                    "Everything runs on this device. Your files never leave it.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(bottom = 4.dp),
                )
            }
            items(Tool.entries) { tool -> ToolTile(tool) { onOpen(tool) } }
        }
    }
}

@Composable
private fun ToolTile(tool: Tool, onClick: () -> Unit) {
    Card(
        onClick = onClick,
        shape = RoundedCornerShape(14.dp),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.card),
        modifier = Modifier.fillMaxWidth().testTag("tool.${tool.name.lowercase()}"),
    ) {
        Row(Modifier.padding(12.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            ToolIcon(tool, 24)
            Column {
                Text(tool.title, style = MaterialTheme.typography.titleMedium)
                Text(
                    tool.summary,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

/** A quiet tool symbol; the document and the action carry the accent. */
@Composable
fun ToolIcon(tool: Tool, size: Int) {
    Box(
        Modifier.size(size.dp),
        contentAlignment = Alignment.Center,
    ) {
        Icon(tool.icon, null, tint = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.size((size * 0.85).dp))
    }
}

@Composable
private fun AboutMenu() {
    val context = LocalContext.current
    var open by remember { mutableStateOf(false) }
    Box {
        IconButton({ open = true }) { Icon(Icons.Outlined.Info, "About") }
        DropdownMenu(open, { open = false }) {
            fun link(url: String) {
                open = false
                context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
            }
            DropdownMenuItem({ Text("Source code") }, { link("https://github.com/ivadim/pdf-tools") }, leadingIcon = { Icon(Icons.Rounded.Code, null) })
            DropdownMenuItem({ Text("Buy me a coffee") }, { link("https://buymeacoffee.com/ivadim") }, leadingIcon = { Icon(Icons.Rounded.Coffee, null) })
            val version = context.packageManager.getPackageInfo(context.packageName, 0).versionName
            DropdownMenuItem({ Text("Version $version", fontWeight = FontWeight.Normal) }, {}, enabled = false, leadingIcon = { Icon(Icons.Rounded.Info, null) })
        }
    }
}
