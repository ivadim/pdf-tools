# Agent guidance for PDF Tools

For any desktop, web, iOS, or Android interface work, read [docs/design-language.md](docs/design-language.md) first. Preserve its shared product contract while using native components and interaction patterns for the platform you are changing. Do not share UI implementation code across platforms to achieve visual consistency.

Inspect the current interface when reviewing or changing it. When a change affects the UX, update all related screenshots in `docs/design-reviews/screenshots` to reflect the changed screens and states. Also update any other affected screenshots maintained in the repository, such as `docs/screenshots`. Screenshots document the interface; they are not a required pixel match for a redesign. In your change description, note how the result preserves tool names, document preview meaning, primary action and result language, where it follows platform conventions, and which screenshots were updated. If a screenshot cannot be regenerated, report which one and why.

The review archive keeps a complete light and dark set for every app. For each tool, keep both its empty and configured screen; include home and About wherever that surface exists. See [the screenshot index](docs/design-reviews/screenshots/README.md) for the coverage list and platform capture steps.
