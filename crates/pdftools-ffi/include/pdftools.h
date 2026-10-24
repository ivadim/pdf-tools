// C interface to the PDF Tools engine (crates/pdftools-ffi).
//
// Strings are UTF-8. Every string or page the library returns is freed with
// the matching pdftools_free_* function. All functions are thread-safe.

#ifndef PDFTOOLS_H
#define PDFTOOLS_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// A rendered page: premultiplied RGBA, drawn on white.
typedef struct PdftoolsPage {
    uint32_t width;
    uint32_t height;
    uint8_t *pixels;
    size_t len;
    // The page's displayed size in points, before any extra rotation.
    float points_width;
    float points_height;
} PdftoolsPage;

// Runs a pdftools-cli command (argv[0] is the command, e.g. "merge").
// Returns 0 and the result as JSON in *out, or 1 and an error message.
int32_t pdftools_run(const char *const *argv, size_t argc, char **out);

// Renders page `page` (1-based) of the PDF at `path`, `width` pixels wide,
// turned clockwise by `rotation` degrees. Returns 0 and fills *out, or 1 and
// sets *error.
int32_t pdftools_render(const char *path, uint32_t page, uint32_t width, int32_t rotation, bool gray,
                        PdftoolsPage *out, char **error);

// Caps the threads tools use to process images and streams at once
// (4 by default); fewer saves memory.
void pdftools_set_threads(size_t threads);

// Forgets a cached document, e.g. after its file changed.
void pdftools_forget(const char *path);

// Parses page ranges ("1-3, 5, 8-") for a document of `total` pages.
// Returns 0 and JSON groups ([[1,2,3],[5],[8,9]]) in *out, or 1 and an error.
int32_t pdftools_parse_groups(const char *text, size_t total, char **out);

// Formats pages compactly: {1, 2, 3, 5} → "1-3, 5".
char *pdftools_format_list(const size_t *pages, size_t count);

void pdftools_free_string(char *string);
void pdftools_free_page(PdftoolsPage *page);

#ifdef __cplusplus
}
#endif

#endif
