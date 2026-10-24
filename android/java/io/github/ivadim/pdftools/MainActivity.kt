package io.github.ivadim.pdftools

import android.content.pm.ApplicationInfo
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import java.io.File

@OptIn(ExperimentalComposeUiApi::class)
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        TestHooks.read(this)
        setContent {
            PdfToolsTheme(Modifier.semantics { testTagsAsResourceId = TestHooks.enabled }) {
                var tool by rememberSaveable { mutableStateOf<Tool?>(TestHooks.tool) }
                BackHandler(tool != null) { tool = null }
                AnimatedContent(
                    tool,
                    transitionSpec = {
                        val forward = targetState != null
                        (slideInHorizontally(tween(300)) { if (forward) it else -it / 3 } + fadeIn())
                            .togetherWith(slideOutHorizontally(tween(300)) { if (forward) -it / 3 else it } + fadeOut())
                    },
                    label = "navigation",
                ) { current ->
                    if (current == null) {
                        HomeScreen(onOpen = { tool = it })
                    } else {
                        val session = remember(current) { ToolSession(current) }
                        ToolScreen(session, onBack = { tool = null })
                    }
                }
            }
        }
    }
}

/**
 * Hooks for testing on an emulator. Debug builds only: the system's document
 * picker runs outside the app, so tests pass fixture paths instead:
 *
 *     adb shell am start -n io.github.ivadim.pdftools/.MainActivity \
 *         --es tool merge --es files "/data/local/tmp/a.pdf,/data/local/tmp/b.pdf"
 */
object TestHooks {
    /** Files to open instead of showing the picker. */
    var files: List<File>? = null
        private set

    /** The tool to open at launch. */
    var tool: Tool? = null
        private set

    /** Debug builds: test tags are also UI Automator resource ids. */
    var enabled = false
        private set

    fun read(activity: ComponentActivity) {
        enabled = activity.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0
        if (!enabled) return
        val extras = activity.intent.extras ?: return
        files = extras.getString("files")?.split(',')?.filter { it.isNotBlank() }?.map(::File)
        tool = extras.getString("tool")?.let { name -> Tool.entries.firstOrNull { it.name.equals(name, ignoreCase = true) } }
    }
}
