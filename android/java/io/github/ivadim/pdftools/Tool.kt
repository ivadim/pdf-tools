// The thirteen tools, in the desktop app's order.

package io.github.ivadim.pdftools

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.MergeType
import androidx.compose.material.icons.automirrored.rounded.RotateRight
import androidx.compose.material.icons.rounded.Collections
import androidx.compose.material.icons.rounded.Compress
import androidx.compose.material.icons.rounded.ContentCut
import androidx.compose.material.icons.rounded.Contrast
import androidx.compose.material.icons.rounded.Draw
import androidx.compose.material.icons.rounded.GridView
import androidx.compose.material.icons.rounded.Info
import androidx.compose.material.icons.rounded.Lock
import androidx.compose.material.icons.rounded.LockOpen
import androidx.compose.material.icons.rounded.Numbers
import androidx.compose.material.icons.rounded.WaterDrop
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector

enum class Tool(
    val title: String,
    val summary: String,
    val icon: ImageVector,
    val tint: Color,
) {
    Merge("Merge", "Combine PDFs into one, and reorder or rotate their pages on the way", Icons.AutoMirrored.Rounded.MergeType, Color(0xfff43f5e)),
    Split("Split", "Divide a PDF into several files, or pull out the pages you need", Icons.Rounded.ContentCut, Color(0xff3b82f6)),
    Compress("Compress", "Reduce file size by cleaning up and re-encoding large images", Icons.Rounded.Compress, Color(0xfff59e0b)),
    Protect("Protect", "Encrypt a PDF with a password (AES-256) and limit what others can do", Icons.Rounded.Lock, Color(0xff6366f1)),
    Unlock("Unlock", "Remove the password and restrictions from a PDF you have access to", Icons.Rounded.LockOpen, Color(0xffa855f7)),
    Rotate("Rotate", "Turn pages upright, individually or all at once", Icons.AutoMirrored.Rounded.RotateRight, Color(0xfff97316)),
    Rearrange("Rearrange", "Reorder, duplicate or delete pages; bookmarks and forms are kept", Icons.Rounded.GridView, Color(0xff22c55e)),
    Numbers("Page numbers", "Add page numbers in the position and style you choose", Icons.Rounded.Numbers, Color(0xff38bdf8)),
    Watermark("Watermark", "Stamp text or a logo across your pages", Icons.Rounded.WaterDrop, Color(0xffc084fc)),
    Metadata("Metadata", "View and edit the title, author, keywords and other properties", Icons.Rounded.Info, Color(0xff22d3ee)),
    Signature("Signature", "Draw, type or upload your signature and place it on the page", Icons.Rounded.Draw, Color(0xffec4899)),
    Grayscale("Grayscale", "Convert colours to shades of grey; text stays sharp and selectable", Icons.Rounded.Contrast, Color(0xffa1a1aa)),
    Images("Images to PDF", "Turn photos and images into a PDF, one image per page", Icons.Rounded.Collections, Color(0xff14b8a6));

    enum class Kind { Single, Merge, Images }

    val kind: Kind
        get() = when (this) {
            Merge -> Kind.Merge
            Images -> Kind.Images
            else -> Kind.Single
        }

    /** Pages can be left out of the result (their checkbox). */
    val leavesOutPages: Boolean
        get() = this in setOf(Merge, Compress, Protect, Rotate, Numbers, Watermark, Metadata, Grayscale)

    /** The run button's label. */
    val action: String
        get() = when (this) {
            Numbers -> "Add numbers"
            Images -> "Create PDF"
            Metadata -> "Save details"
            Signature -> "Sign"
            else -> title
        }

    fun makeOptions(): ToolOptions = when (this) {
        Merge -> MergeOptions()
        Split -> SplitOptions()
        Compress -> CompressOptions()
        Protect -> ProtectOptions()
        Unlock -> UnlockOptions()
        Rotate -> RotateOptions()
        Rearrange -> RearrangeOptions()
        Numbers -> NumbersOptions()
        Watermark -> WatermarkOptions()
        Metadata -> MetadataOptions()
        Signature -> SignatureOptions()
        Grayscale -> GrayscaleOptions()
        Images -> ImagesOptions()
    }
}

/** "RRGGBB" for the engine. */
val Color.hex: String
    get() = "%02x%02x%02x".format((red * 255).toInt(), (green * 255).toInt(), (blue * 255).toInt())
