// The Rust engine (crates/pdftools-ffi, src/android.rs): tools run the same
// commands as pdftools-cli and the web version; pages are rendered straight
// into memory.

package io.github.ivadim.pdftools

import android.graphics.Bitmap
import android.text.format.Formatter
import java.io.File
import java.nio.ByteBuffer
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.sync.withPermit
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import org.json.JSONTokener

/** An error from the engine, worded for the person using the app. */
class EngineException(message: String) : Exception(message)

/** A page as the engine returns it: premultiplied RGBA drawn on white. */
class NativePage(val width: Int, val height: Int, val pointsWidth: Float, val pointsHeight: Float, val pixels: ByteArray)

/** A size in points (or pixels). */
data class Size(val width: Float, val height: Float)

/** A drawn page and its displayed size in points, before any extra rotation. */
class RenderedPage(val bitmap: Bitmap, val points: Size)

/** A command's result, as parsed JSON. */
class Json(val value: Any?) {
    operator fun get(key: String) = Json((value as? JSONObject)?.opt(key))
    val int: Int get() = (value as? Number)?.toInt() ?: 0
    val long: Long get() = (value as? Number)?.toLong() ?: 0
    val bool: Boolean get() = value as? Boolean ?: false
    val string: String? get() = value as? String
    val array: List<Json>
        get() = (value as? JSONArray)?.let { a -> (0 until a.length()).map { Json(a.opt(it)) } } ?: emptyList()

    companion object {
        fun parse(text: String) = Json(runCatching { JSONTokener(text).nextValue() }.getOrNull())
    }
}

object Engine {
    init {
        System.loadLibrary("pdftools_ffi")
    }

    @JvmStatic private external fun nativeRun(args: Array<String>): String

    @JvmStatic private external fun nativeRender(path: String, page: Int, width: Int, rotation: Int, gray: Boolean): NativePage

    @JvmStatic private external fun nativeForget(path: String)

    @JvmStatic private external fun nativeParseGroups(text: String, total: Int): String

    @JvmStatic private external fun nativeSetThreads(threads: Int)

    @JvmStatic private external fun nativeFormatList(pages: IntArray): String

    /**
     * Images and streams the engine processes at once. Each image in flight
     * holds its decoded pixels, so devices with about 3 GB of memory or less use two.
     */
    fun threads(memory: Long): Int = if (memory < 3_500_000_000L) 2 else 4

    /** Sets the engine up for this device; called once at launch. */
    fun configure(memory: Long) = nativeSetThreads(threads(memory))

    /** Thumbnails render a few at a time (the engine keeps three document caches). */
    private val renders = Semaphore(3)

    /** Runs a command, e.g. `["merge", out, a, b]`, off the main thread. */
    suspend fun run(args: List<String>): Json = withContext(Dispatchers.Default) { Json.parse(nativeRun(args.toTypedArray())) }

    suspend fun render(file: File, page: Int, width: Int, rotation: Int = 0, gray: Boolean = false): RenderedPage =
        renders.withPermit {
            withContext(Dispatchers.Default) {
                val result = nativeRender(file.path, page, width.coerceAtLeast(1), rotation, gray)
                val bitmap = Bitmap.createBitmap(result.width, result.height, Bitmap.Config.ARGB_8888)
                bitmap.copyPixelsFromBuffer(ByteBuffer.wrap(result.pixels))
                RenderedPage(bitmap, Size(result.pointsWidth, result.pointsHeight))
            }
        }

    /**
     * Drops a cached document whose file went away. In the background: it
     * waits for any page of it still rendering.
     */
    fun forget(file: File) {
        Thread { nativeForget(file.path) }.start()
    }

    /** [1, 2, 3, 5] → "1-3, 5", as the engine writes page lists. */
    fun formatList(pages: Collection<Int>): String = nativeFormatList(pages.toIntArray())

    /** "1-3, 5, 8-" → [[1, 2, 3], [5], [8, …, total]]; throws a message for the person. */
    fun parseGroups(text: String, total: Int): List<List<Int>> =
        Json.parse(nativeParseGroups(text, total)).array.map { group -> group.array.map { it.int } }

    /** The distinct pages named, in order. */
    fun parseList(text: String, total: Int): List<Int> = parseGroups(text, total).flatten().toSortedSet().toList()
}

/** [1, 2, 3, 5] → "1-3, 5". */
fun formatList(pages: Collection<Int>): String = Engine.formatList(pages)

fun formatBytes(bytes: Long): String = Formatter.formatShortFileSize(App.context, bytes)

fun plural(n: Int, word: String) = "$n $word${if (n == 1) "" else "s"}"
