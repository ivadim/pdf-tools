# PDF Tools design language

**Status:** design direction for the four interfaces (desktop, web, iOS, Android). Each interface implements this locally with its own components and conventions. The shared contract is the user's understanding of a tool, its input, preview, action, and result.

## Product idea

**A clear workbench for documents.** The document is the main visual material. Controls explain a precise operation on it; a preview makes the outcome legible before the user commits. The product should feel dependable and private, not like a colorful catalog of utilities.

Someone moving between platforms should immediately recognize the tool names, page thumbnails, input-to-result sequence, blue action and selection state, and the plain-language promise that processing happens on the device. They should also immediately recognize the host platform's navigation, typography, menus, file picker, and share/save behavior.

## Screenshot review (5 November 2026)

Source: [the screenshot collection](design-reviews/screenshots/), as it appeared on 5 November 2026. I compared the four home screens and Merge, Split, and Protect tool screens; the folder also contains other tools and states. The findings below record that review and may no longer describe the screenshots as they are updated. These are screenshot observations, not interaction or accessibility test results.

| Area | What works | Drift to resolve |
| --- | --- | --- |
| Identity | All four say PDF Tools and use the same core tool names. Merge, Split, and Protect are recognizable. | Desktop and mobile give every tool a saturated category color; web makes all tools blue. iOS and Android use different category pictograms for several tools. The tile treatment currently carries more identity than the document itself. |
| Home | Tool descriptions help discovery. The device-local promise is visible on every platform. | Desktop's oversized question and long descriptions make the home feel like a landing page. Phone cards take considerable vertical space. Web's small four-column cards are comparatively dense. The same task catalog has four different visual weights. |
| Workflow | Pages are shown as real previews. Merge exposes file origins and page counts; Split exposes modes; Protect keeps permissions explicit. | Desktop puts options left and preview right; web puts a long preview grid before a narrow form rail; phones place form before pages. This is appropriate by screen size, but the step order and summary wording are inconsistent. |
| Actions | A single blue primary action is broadly established. Disabled states are visible. | Web originally said “Run Merge/Protect/Split,” while desktop action labels carried trailing dots and phones used the verb alone. Platform-specific save dialogs are fine; the action name and result language should agree. |
| Page marks | Merge uses source-file colors and page numbering across implementations. | The active blue frame around **every** selected phone page is visually loud. Desktop's Merge preview is much smaller than the available work area. Source color can be confused with selection color. |
| Native fit | iOS uses switches and a segmented Split choice; Android uses Material-like text fields and switches; desktop uses a persistent sidebar. | iOS has custom circular toolbar controls and a heavy translucent bottom dock. Android's Split choice looks like a bordered iOS segmented control. These should be expressed through each platform's standard patterns. |
| Accessibility risk | Text labels accompany many icons and controls. | Thumbnail actions are icon-heavy; the screenshots cannot establish labels, focus order, screen reader semantics, target size, or text scaling. Treat those as required review points. |

### Decisions, in priority order

1. Make the **document and its proposed output** the visual center on every tool screen. A page preview must be large enough to recognize its layout; controls should not consume more visual emphasis than the page.
2. Use **one product accent blue** for the primary action, active choice, and focus/selection. Use other hues only to distinguish source files or output groups, with a text or pattern cue as well. Remove the rainbow tool tiles as the main identity device.
3. Give all tools the same conceptual sequence: **choose input → set options → inspect effect → create result → save/share**. A platform may arrange the steps differently in space, but the labels, state, and result summary must agree.
4. Keep native components and typography. Shared design values are semantic roles and behavior, not pixel-identical controls.

## Shared contract

### Names and voice

- Product name: **PDF Tools**. Tool titles use the same short verb or noun on all platforms: Merge, Split, Compress, Protect, Unlock, Rotate, Rearrange, Page numbers, Watermark, Metadata, Signature, Grayscale, Images to PDF.
- Describe an operation in one concrete sentence. Prefer “Combine PDFs into one” to marketing language. Use “on this device” where true; do not imply upload. Keep the privacy message visible on the home/about surface, not repeated as permanent tool chrome.
- The primary button is the operation verb: **Merge**, **Split**, **Protect**. When a result exists, the next action is **Save** or **Share** according to platform. Loading states say what is happening (“Merging 5 pages…”). Success states say what happened (“Merged 2 files into 16 pages.”). Errors name the failed input and a next step.
- Preserve meaningful nouns and units in summaries: “2 files · 16 pages,” “3 output files · 14 pages,” “Password required.” Keep page counts and generated names consistent with the preview.
- Do not use color alone to identify a source file, selected page, output group, warning, or error.

### Visual roles

| Role | Shared intent | Local implementation |
| --- | --- | --- |
| Canvas | Quiet neutral ground; white PDF pages read as paper. | Use platform semantic backgrounds and dark-mode surfaces. Do not force the same hex across operating systems. |
| Surface | One restrained step from canvas for options, file lists, or sidebars. | Native grouped sections on iOS, Material surface containers on Android, restrained panels on desktop/web. Avoid a panel around every thumbnail. |
| Ink | Strong text for tool, file, and result; muted text only for supporting details. | System text colors and dynamic type/scaling. Never rely on pale placeholder text for an instruction. |
| Accent | Blue identifies the current action or choice. Current implementations use nearby blues (`web: #2f6fed`, Android light `#2f5bd3`); treat these as a family, not an exact shared token. | Map to an accessible native tint or primary color in both appearances. One filled primary action per active stage. |
| Source markers | Distinguish files in Merge and output groups in Split. | Assign stable colors plus file name/number; repeat the marker beside thumbnails and in the file list. Do not reuse selection blue as the only origin cue. |
| Warning/error | State requiring attention. | Native semantic colors with a written explanation and recovery action. Never make an empty required field look like an error before interaction. |

Use restrained radii and flat surfaces. A PDF preview may have a subtle edge or shadow to separate white paper from a white surface. Avoid gradients, glass effects, glow, and saturated colored icon containers throughout a working screen. Tool icons can aid scanning, but their shape and label should carry meaning without a category rainbow.

### Tool screen anatomy

1. **Identity:** tool title and, if needed, one-line purpose. The current file name and page count are easy to find.
2. **Input:** native file selection and replacement. Multiple inputs have a stable order and clearly labeled remove/reorder actions.
3. **Options:** only settings needed for the task, grouped by meaning. Default selections are explicit. Password and permission controls explain their effect.
4. **Preview:** show the actual pages and proposed order, inclusion, rotation, or output grouping. Make page number and source available without tiny text inside the thumbnail. Provide a larger inspection view where needed.
5. **Outcome:** a compact summary close to the primary action. Invalid settings explain the reason beside the field and in the action area. Once complete, provide a clear Save/Share action and an accurate success summary.

This anatomy is a behavior and information contract. Desktop may use a two-pane workbench; web may use a responsive rail; phones may stack options above a two-column page grid. Do not transfer the same layout or component code between them.

### Interaction rules

- Reordering has a visible handle or move controls, plus keyboard and accessibility actions. Rotation and inclusion need named controls and reversible feedback. Selection is a check or other explicit mark; reserve the blue page outline for the actively focused page or a true selection, not every included page.
- Show only actions that apply to the current tool and state. Keep secondary actions discoverable in a menu or toolbar. Protect destructive actions with an undo or confirmation appropriate to the platform.
- Keep the primary action in a stable, reachable place while scrolling, but let a sticky bar use the native safe area and avoid hiding page content or the keyboard. Disabled actions have an adjacent reason in words.
- Support light/dark appearance, large text, high contrast, reduced motion, keyboard focus, and screen readers. For touch, use the host platform's minimum target guidance. Preview images need accessible page number and file context; purely decorative icons do not need duplicate announcements.

## Platform instructions

| Platform | Follow its conventions | Apply the PDF Tools contract |
| --- | --- | --- |
| **iOS** | SwiftUI navigation, system type and Dynamic Type, semantic grouped surfaces, native toolbars, menus, sheets, switches, and share sheet. Respect safe areas and the back gesture. | Keep the document grid and one reachable primary action. Reduce custom circular top buttons and the heavy bottom dock when native toolbar controls communicate the same actions. Use a native selection control for Split where it fits; move overflow modes to a menu if large text makes the segment cramped. |
| **Android** | Material 3 color roles and components, top app bar, native text fields, switches, sheets, and system back. Adapt to edge-to-edge insets, larger screens, font scaling, and 48 dp touch targets. | Keep the same names, output summaries, and preview semantics. Express Split mode through Material controls rather than mimicking iOS segmented styling. Consider a list/detail arrangement on tablets and foldables. |
| **Desktop** | Native window controls, keyboard shortcuts, menus, drag and drop, resizable panes, and platform file dialogs. A persistent tool sidebar is useful at desktop width. | The PDF preview is the work surface. Let users resize preview and options panes; do not leave page thumbnails tiny beside a mostly empty panel. Keep actions named and keyboard reachable. Desktop runs on multiple operating systems, so do not apply macOS-only chrome everywhere. |
| **Web** | Semantic HTML, visible keyboard focus, responsive layout, browser file picker and download behavior, WCAG color contrast, and system appearance preference. | Use the same input/options/preview/result language. Avoid a full-height thumbnail wall that separates the settings rail from the pages being affected. At narrow widths move options into the reading flow, preserve the primary action, and test at phone widths. |

Apple's current guidance emphasizes task focus, adaptive appearance and type, expected navigation, and judicious color. Android emphasizes adaptive layouts, legible contrast, touch targets, screen reader descriptions, and controls that remain clear around system bars. See [Apple Designing for iOS](https://developer.apple.com/design/human-interface-guidelines/designing-for-ios/), [Apple Color](https://developer.apple.com/design/human-interface-guidelines/color), [Apple Branding](https://developer.apple.com/design/human-interface-guidelines/branding), [Android Design & Plan](https://developer.android.com/design), [Android Accessibility](https://developer.android.com/design/ui/mobile/guides/foundations/accessibility), and [Android System bars](https://developer.android.com/design/ui/mobile/guides/foundations/system-bars).

## Agent handoff

When changing one interface:

1. Read this guide and inspect the current interface, including empty, configured, loading/error, and completed states where available. Screenshots document the interface and are not a pixel template for a redesign.
2. State the tool's main job and the user's next action. Keep the shared names, state model, preview meaning, and result language above.
3. Use that platform's local design system and native components. Own local semantic tokens; do not add a cross-platform UI package or duplicate another platform's layout code.
4. Review the changed screen at normal and large text, light and dark, narrow and wide sizes as applicable. Confirm keyboard/screen reader names and that the primary action and page content are not obscured.
5. Compare the result against at least one other platform's current interface for product recognition and against the host platform's guidance for native fit. Update all screenshots affected by the UX change in the screenshot collection and elsewhere in the repository. Record intentional platform differences and the screenshots updated in the change description; report any screenshots that could not be regenerated and why.

The maintained review archive is indexed in [screenshots/README.md](design-reviews/screenshots/README.md). It includes light and dark home and tool-state captures for each interface, plus About where available. Keep both appearances current whenever a shared UX change affects a screen. The screenshot names and coverage requirements do not imply shared layouts or UI code.

The first candidates are the home catalog's category color treatment, Merge's page inclusion and source markers, and Split/Protect action-area messaging. These offer the most visible gain without requiring identical layouts.
