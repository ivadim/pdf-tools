// Material 3 colours close to the iOS app's: its blue accent, grouped grey
// backgrounds with white (or dark grey) cards, in light and dark.

package io.github.ivadim.pdftools

import androidx.compose.foundation.background
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.window.DialogWindowProvider
import androidx.core.view.WindowCompat

private val Light = lightColorScheme(
    primary = Color(0xff2f5bd3),
    onPrimary = Color.White,
    primaryContainer = Color(0xffdce4fb),
    onPrimaryContainer = Color(0xff0b2a7a),
    secondaryContainer = Color(0xffdce4fb),
    onSecondaryContainer = Color(0xff0b2a7a),
    background = Color(0xfff2f2f7),
    surface = Color(0xfff2f2f7),
    surfaceContainerLowest = Color.White,
    surfaceContainerLow = Color.White,
    surfaceContainer = Color(0xfff9f9fb),
    surfaceContainerHigh = Color.White,
    surfaceContainerHighest = Color(0xffe5e5ea),
    surfaceVariant = Color(0xffe5e5ea),
    onSurfaceVariant = Color(0xff6c6c70),
    outline = Color(0xff8e8e93),
    outlineVariant = Color(0xffd1d1d6),
)

private val Dark = darkColorScheme(
    primary = Color(0xff5b82f0),
    onPrimary = Color.White,
    primaryContainer = Color(0xff1d3a8f),
    onPrimaryContainer = Color(0xffdce4fb),
    secondaryContainer = Color(0xff1d3a8f),
    onSecondaryContainer = Color(0xffdce4fb),
    background = Color.Black,
    surface = Color.Black,
    surfaceContainerLowest = Color(0xff1c1c1e),
    surfaceContainerLow = Color(0xff1c1c1e),
    surfaceContainer = Color(0xff121214),
    surfaceContainerHigh = Color(0xff2c2c2e),
    surfaceContainerHighest = Color(0xff3a3a3c),
    surfaceVariant = Color(0xff2c2c2e),
    onSurfaceVariant = Color(0xff98989f),
    outline = Color(0xff8e8e93),
    outlineVariant = Color(0xff3a3a3c),
)

/** The cards on grouped backgrounds. */
val androidx.compose.material3.ColorScheme.card get() = surfaceContainerLowest

/** Success, as in the result's check mark. */
val androidx.compose.material3.ColorScheme.success get() = Color(0xff34c759)

/** Warnings, as for damaged files. */
val androidx.compose.material3.ColorScheme.warning get() = Color(0xffff9500)

@Composable
fun PdfToolsTheme(modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = if (isSystemInDarkTheme()) Dark else Light) {
        Box(modifier.fillMaxSize().background(MaterialTheme.colorScheme.background)) { content() }
    }
}

/**
 * A dialog covering the screen (results, previews, the page viewer), with
 * status bar icons that suit the theme and, in debug builds, test tags as
 * UI Automator ids like the main window.
 */
@OptIn(ExperimentalComposeUiApi::class)
@Composable
fun FullScreenDialog(onClose: () -> Unit, content: @Composable () -> Unit) {
    Dialog(onClose, DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        val window = (LocalView.current.parent as? DialogWindowProvider)?.window
        val dark = isSystemInDarkTheme()
        val view = LocalView.current
        SideEffect {
            window?.let { WindowCompat.getInsetsController(it, view).isAppearanceLightStatusBars = !dark }
            window?.let { WindowCompat.getInsetsController(it, view).isAppearanceLightNavigationBars = !dark }
        }
        Box(Modifier.fillMaxSize().semantics { testTagsAsResourceId = TestHooks.enabled }) { content() }
    }
}

/** Test tags as UI Automator ids in debug builds, for content in its own window (alerts). */
@OptIn(ExperimentalComposeUiApi::class)
fun Modifier.testIds() = semantics { testTagsAsResourceId = TestHooks.enabled }
