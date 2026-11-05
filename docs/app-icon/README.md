# PDF Tools app icon — Paperform

The selected direction is a continuous strip of folded paper forming a capital P.
Its open counter and broad diagonal fold remain visible when the material shading
disappears at small sizes. The background is vermilion (`#e52b16`); the paper uses
warm whites and quiet fold shading. There is no document frame, text badge, or
separate tool symbol.

Three concepts were explored with the built-in image-generation tool in
[`concepts.png`](concepts.png). The user selected the red folded-paper P. The final
artwork was then constructed as native SVG paths in
[`packaging/icon.svg`](../../packaging/icon.svg), rather than shipping a crop of
the generated concept sheet. [`preview.png`](preview.png) shows the actual exported
assets at small sizes and in platform masks.

## Regeneration

From the repository root:

```sh
bazel run //crates/pdftools-app:render_icons -- "$PWD"
```

The full-bleed SVG is the master. Keep its `background` element ID: the renderer
hides that layer when producing the transparent Android foreground. Paper shading
is part of the user-selected folded material treatment; webpage styling rules are
not applied to this standalone app mark.

The renderer exports:

- Desktop: transparent 1024px and 256px PNGs with an inset 824px tile. CI derives
  the macOS ICNS from the 1024px PNG.
- Windows: one ICO with 16, 24, 32, 48, 64, 128, and 256px frames.
- iOS: opaque, full-bleed 1024px RGB PNG, with no alpha channel or baked-in corners.
- Web: full-bleed 192px and 512px PNGs.
- Android: separate transparent foregrounds at 108, 162, 216, 324, and 432px.
  The background color is declared in `android/res/values/colors.xml`; update it
  alongside the SVG if changing the brand color.

## Review and validation

The production generator compiled and ran successfully. Its formatting check and
`git diff --check` passed. Every exported image dimension and every decoded ICO
frame was checked. The iOS file has three RGB channels. At xxxhdpi, all visible
Android foreground pixels fit within a radius of 128.3px, inside the 132px circular
safe zone. The preview was inspected at 16–128px and on light and dark surfaces.

Visual review: the counter stays open at 16px, the diagonal remains legible at
32px, and the upper turn provides material detail at larger sizes. At the smallest
sizes, the shading intentionally becomes secondary to the P silhouette. The
generic document-and-badge and diagonal-cut-document directions were rejected.

These are assets for the existing platform packaging pipelines, not an App Store
submission or a layered Icon Composer document. Store submission validation was
not run.

Platform references: [Apple app icon guidance](https://developer.apple.com/design/human-interface-guidelines/app-icons),
[Apple asset catalogs](https://developer.apple.com/documentation/xcode/configuring-your-app-icon),
and [Android adaptive icon guidance](https://developer.android.com/develop/ui/compose/system/icon_design_adaptive).

## Concept-generation prompt

Built-in image-generation tool; preview-only concept sheet. Final production SVG
is independently editable. No stock photos, fonts, or external brand marks are
embedded in the production icon.

> Use case: logo-brand. Create an exquisite designer's concept sheet for a real app called PDF Tools, an independent native offline PDF editing app (combine, split, rearrange, rotate, protect, sign). The current generic document with a PDF badge and a later document cut diagonally failed. We need a truly authored, beautiful, modern app identity. Landscape contact sheet on a very pale cool gray background, three equally large rounded-square app icons in a single horizontal row with generous space. Under each main icon show the SAME mark tiny at launcher size, and one short elegant black label: 'Paperform', 'Interleave', 'Pagecraft'. These are THREE DIFFERENT fully resolved design directions, not color variations. First: a magnificent geometric capital P created from a wide folded white paper strip, structurally exact with a real open counter, slight fold tonal planes, deep vivid vermilion ground; the P is the entire symbol, no page outline around it. Second: a clever single bold geometric monogram made of two interlocking white page-corner shapes with negative space, blue ground; no boring document silhouette, no diagonal slash. Third: three ivory paper leaves precisely fanned around a lower pivot to form a compact sculptural fan with a dark teal ground, just 3 broad distinct leaves and one vivid turquoise face. The marks should be large, crisp, balanced and uniquely identifiable at 32px. High craft, sophisticated restrained three-dimensional paper-fold modelling using flat tonal planes, NOT plastic, NOT glass, NOT blobs, NOT complex gradients, NOT photography, NOT excessive texture, no drop shadow behind the icons. Simple clean silhouettes with perfect optical balance. No letters except intentional P monogram and the labels below; absolutely no PDF badge, wrench, tools, sparkles, AI stars, shield or checkmark. Consistent precise superellipse corners on backgrounds. This is a premium production app icon design exploration, not a marketing page. Give each concept a thoughtful and genuinely different silhouette.
