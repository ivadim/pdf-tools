// The thirteen tools. Each describes its options panel, what it adds to the
// page cards, what's missing before it can run, and the command it runs.
//
// `ctx.state` holds the open file(s) and `items` (one per page card); tools
// keep their own options in `ctx.state.opts`. `ctx.refresh()` redraws the
// pages and the run button, `ctx.rebuild()` also redraws the panel.

import { run, parseGroups, parseList, formatList, formatBytes, put, range } from "./engine.js";
import {
  h,
  icon,
  iconButton,
  textField,
  numberField,
  rangeField,
  selectField,
  checkField,
  segmented,
  anchorPicker,
  place,
  pickFiles,
  textPicture,
  canvasPicture,
  filePicture,
  trim,
  field,
} from "./ui.js";

const PART_COLORS = ["#2f6fed", "#e0752d", "#1f9d6a", "#b8459e", "#c9a227", "#3aa7c9", "#d24b4b", "#7a5cd6"];
export const fileColor = (i) => PART_COLORS[i % PART_COLORS.length];

const plural = (n, word) => `${n} ${word}${n === 1 ? "" : "s"}`;
const kept = (state) => state.items.filter((i) => i.keep);

/** `--keep` when some pages are left out. */
function keepArgs(state) {
  const pages = kept(state).map((i) => i.n);
  return pages.length === state.items.length ? [] : [`--keep=${formatList(pages)}`];
}

/** The number each original page will have once left-out pages are gone. */
function finalPositions(state) {
  return new Map(kept(state).map((item, index) => [item.n, index + 1]));
}

function keepProblem(state) {
  return kept(state).length ? null : "Keep at least one page.";
}

const removedNote = (state) => {
  const removed = state.items.length - kept(state).length;
  return removed ? ` (${plural(removed, "page")} left out)` : "";
};

/** Runs a single-file command writing `{out}/name` and returns it for download. */
async function runToFile(args, name, message) {
  const { json, files } = await run(args.map((a) => (a === "OUT" ? `{out}/${name}` : a)), { outputs: true });
  return { json, files, message: typeof message === "function" ? message(json, files) : message };
}

/** Page selection typed by the person, as final positions; null = all. */
function stampedPages(ctx) {
  const { state } = ctx;
  if (!state.opts.custom) return null;
  const positions = finalPositions(state);
  return parseList(state.opts.pagesText, state.pdf.pageCount)
    .map((p) => positions.get(p))
    .filter(Boolean);
}

function pageChoice(ctx) {
  const { opts } = ctx.state;
  return [
    segmented(
      [
        [false, "All pages"],
        [true, "Some pages"],
      ],
      opts.custom,
      (v) => {
        opts.custom = v;
        ctx.rebuild();
      },
    ),
    opts.custom ? textField("Pages", opts.pagesText, (v) => ((opts.pagesText = v), ctx.refresh()), { placeholder: "1-3, 5" }) : null,
  ];
}

function pageChoiceProblem(ctx) {
  const { state } = ctx;
  if (!state.opts.custom) return null;
  try {
    return stampedPages(ctx).length ? null : "None of the chosen pages are kept.";
  } catch (error) {
    return error.message;
  }
}

/** Is this item's page among those the stamp goes on? */
function isStamped(ctx, item) {
  if (!item.keep) return false;
  try {
    const pages = stampedPages(ctx);
    return !pages || pages.includes(finalPositions(ctx.state).get(item.n));
  } catch {
    return false;
  }
}

const moveControls = (ctx, index) => [
  iconButton("chevron-left", "Move earlier", () => ctx.move(index, -1), index === 0),
  iconButton("chevron-right", "Move later", () => ctx.move(index, 1), index === ctx.state.items.length - 1),
];

const rotateControls = (ctx, item) => [
  iconButton("rotate-ccw", "Rotate left", () => ((item.rotation = (item.rotation + 270) % 360), ctx.refresh())),
  iconButton("rotate-cw", "Rotate right", () => ((item.rotation = (item.rotation + 90) % 360), ctx.refresh())),
];

// ── Multi-file ─────────────────────────────────────────────────────────

const merge = {
  id: "merge",
  name: "Merge",
  icon: "merge",
  desc: "Combine PDFs; reorder and rotate pages.",
  kind: "merge",
  keep: true,
  defaults: () => ({}),
  panel: (ctx) => [
    h("p", { className: "status" }, `${plural(ctx.state.files.length, "file")}, ${plural(kept(ctx.state).length, "page")} in the result.`),
  ],
  toolbar: (ctx) => [
    h("button", { type: "button", onClick: () => ctx.addFiles() }, "Add PDFs…"),
    h(
      "button",
      {
        type: "button",
        onClick: () => {
          ctx.state.items.sort((a, b) => ctx.state.files.indexOf(a.pdf) - ctx.state.files.indexOf(b.pdf) || a.n - b.n);
          ctx.refresh();
        },
      },
      "Reset order",
    ),
  ],
  card: (ctx, item, index) => ({ controls: [...moveControls(ctx, index), ...rotateControls(ctx, item)] }),
  problem: (ctx) => (ctx.state.files.length < 2 ? "Add at least two PDFs." : keepProblem(ctx.state)),
  async run(ctx) {
    const { files, items } = ctx.state;
    const pages = items.filter((i) => i.keep).map((i) => `${files.indexOf(i.pdf)}:${i.n}:${i.rotation}`);
    return runToFile(["merge", "OUT", ...files.map((f) => f.path), `--pages=${pages.join(",")}`], "merged.pdf", (json) =>
      `Merged ${plural(json.files, "file")} into ${plural(json.pages, "page")}.`,
    );
  },
};

const PAGE_SIZES = [
  ["a4", "A4"],
  ["letter", "Letter"],
  ["fit", "Image size"],
];

const images = {
  id: "images",
  name: "Images to PDF",
  icon: "images",
  desc: "Turn JPEG and PNG images into a PDF, one per page.",
  kind: "images",
  defaults: () => ({ size: "a4", orientation: "auto", margin: 0 }),
  panel(ctx) {
    const { opts } = ctx.state;
    const set = (key) => (v) => ((opts[key] = v), ctx.refresh());
    return [
      selectField("Page size", opts.size, PAGE_SIZES, set("size")),
      selectField(
        "Orientation",
        opts.orientation,
        [
          ["auto", "Automatic"],
          ["portrait", "Portrait"],
          ["landscape", "Landscape"],
        ],
        set("orientation"),
      ),
      selectField(
        "Margin",
        String(opts.margin),
        [
          ["0", "None"],
          ["18", "Small"],
          ["36", "Large"],
        ],
        (v) => set("margin")(Number(v)),
      ),
    ];
  },
  toolbar: (ctx) => [h("button", { type: "button", onClick: () => ctx.addFiles() }, "Add images…")],
  /** The page an image lands on, as the core lays it out (in points). */
  layout(opts, image) {
    const quarter = image.rotation % 180 === 90;
    const [iw, ih] = quarter ? [image.height, image.width] : [image.width, image.height];
    const m = opts.margin;
    let page;
    if (opts.size === "fit") page = [iw * 0.75 + 2 * m, ih * 0.75 + 2 * m];
    else {
      const [w, hgt] = opts.size === "letter" ? [612, 792] : [595.28, 841.89];
      const landscape = opts.orientation === "auto" ? iw > ih : opts.orientation === "landscape";
      page = landscape ? [hgt, w] : [w, hgt];
    }
    const scale = Math.min(Math.max(1, page[0] - 2 * m) / iw, Math.max(1, page[1] - 2 * m) / ih);
    return { page, width: iw * scale, height: ih * scale };
  },
  problem: (ctx) => (ctx.state.items.length ? null : "Add at least one image."),
  async run(ctx) {
    const { items, opts } = ctx.state;
    const args = ["images", "OUT", ...items.map((i) => `${i.path}:${i.rotation}`)];
    args.push(`--size=${opts.size}`, `--orientation=${opts.orientation}`, `--margin=${opts.margin}`);
    return runToFile(args, "images.pdf", (json) => `Created a PDF with ${plural(json.pages, "page")}.`);
  },
};

// ── Single file ────────────────────────────────────────────────────────

const split = {
  id: "split",
  name: "Split",
  icon: "split",
  desc: "Split by ranges, every N pages or each page, or extract pages.",
  defaults: (state) => ({ mode: "ranges", ranges: "", every: 2, extract: "", base: state.pdf.baseName }),
  /** Each output file's pages, or an error message. */
  parts(ctx) {
    const { opts, pdf } = ctx.state;
    const total = pdf.pageCount;
    switch (opts.mode) {
      case "ranges":
        return parseGroups(opts.ranges, total);
      case "every": {
        const n = Math.max(1, Math.floor(opts.every) || 1);
        return range(0, Math.ceil(total / n) - 1).map((i) => range(i * n + 1, Math.min(total, (i + 1) * n)));
      }
      case "each":
        return range(1, total).map((p) => [p]);
      default:
        return [parseList(opts.extract, total)];
    }
  },
  panel(ctx) {
    const { opts } = ctx.state;
    const set = (key) => (v) => ((opts[key] = v), ctx.refresh());
    return [
      segmented(
        [
          ["ranges", "Ranges"],
          ["every", "Every N"],
          ["each", "Each page"],
          ["extract", "Extract"],
        ],
        opts.mode,
        (v) => ((opts.mode = v), ctx.rebuild()),
      ),
      opts.mode === "ranges" ? textField("One file per range", opts.ranges, set("ranges"), { placeholder: "1-3, 4-6, 7-" }) : null,
      opts.mode === "every" ? numberField("Pages per file", opts.every, set("every"), { min: 1 }) : null,
      opts.mode === "extract" ? textField("Pages to extract into one file", opts.extract, set("extract"), { placeholder: "1, 3-5" }) : null,
      opts.mode !== "extract" ? textField("File names", opts.base, set("base")) : null,
    ];
  },
  card(ctx, item) {
    let parts;
    try {
      parts = this.parts(ctx);
    } catch {
      return {};
    }
    const index = parts.findIndex((pages) => pages.includes(item.n));
    return { part: index >= 0 ? PART_COLORS[index % PART_COLORS.length] : undefined, removed: index < 0 && ctx.state.opts.mode !== "each" };
  },
  problem(ctx) {
    try {
      const parts = this.parts(ctx);
      if (ctx.state.opts.mode !== "extract" && !ctx.state.opts.base.trim()) return "Enter a name for the files.";
      return parts.length ? null : "Nothing to split.";
    } catch (error) {
      return error.message;
    }
  },
  async run(ctx) {
    const { opts, pdf } = ctx.state;
    const parts = this.parts(ctx);
    const base = opts.mode === "extract" ? "part" : opts.base.trim().replaceAll("/", "_");
    const { json, files } = await run(
      ["split", pdf.path, "{out}", `--parts=${parts.map(formatList).join(";")}`, `--name=${base}`],
      { outputs: true },
    );
    if (opts.mode === "extract") {
      files[0].name = `${pdf.baseName}-pages-${formatList(parts[0]).replaceAll(", ", "_")}.pdf`;
      return { files, message: `Extracted ${plural(parts[0].length, "page")}.` };
    }
    files.sort((a, b) => parseInt(a.name.slice(base.length + 1), 10) - parseInt(b.name.slice(base.length + 1), 10));
    return { files, message: `Created ${plural(json.count, "file")}.` };
  },
};

const compress = {
  id: "compress",
  name: "Compress",
  icon: "compress",
  desc: "Make files smaller; optionally downscale images.",
  keep: true,
  defaults: () => ({ level: "balanced" }),
  panel(ctx) {
    const { opts, pdf } = ctx.state;
    const notes = {
      lossless: "Drops unused data. Page content is untouched.",
      balanced: "Also downscales images to 2000 px and re-encodes them as JPEG (quality 75).",
      strong: "Also downscales images to 1200 px and re-encodes them as JPEG (quality 50).",
    };
    return [
      segmented(
        [
          ["lossless", "Lossless"],
          ["balanced", "Balanced"],
          ["strong", "Strong"],
        ],
        opts.level,
        (v) => ((opts.level = v), ctx.rebuild()),
      ),
      h("p", { className: "status" }, notes[opts.level]),
      h(
        "p",
        { className: "status" },
        `${formatBytes(pdf.size)} · ${plural(pdf.info.images, "image")} (${formatBytes(pdf.info.image_bytes)})`,
      ),
    ];
  },
  problem: (ctx) => keepProblem(ctx.state),
  run(ctx) {
    const { opts, pdf } = ctx.state;
    return runToFile(["compress", pdf.path, "OUT", `--level=${opts.level}`, ...keepArgs(ctx.state)], `${pdf.baseName}-compressed.pdf`, (json) => {
      const saved = json.before ? Math.round((1 - json.after / json.before) * 100) : 0;
      return `${formatBytes(json.before)} → ${formatBytes(json.after)} (${saved >= 0 ? "−" : "+"}${Math.abs(saved)}%)${removedNote(ctx.state)}.`;
    });
  },
};

const protect = {
  id: "protect",
  name: "Protect",
  icon: "protect",
  desc: "Add an AES-256 password and restrictions.",
  keep: true,
  defaults: () => ({ password: "", confirm: "", owner: "", print: true, copy: true, modify: true, annotate: true }),
  panel(ctx) {
    const { opts } = ctx.state;
    const set = (key) => (v) => ((opts[key] = v), ctx.refresh());
    return [
      textField("Password to open", opts.password, set("password"), { type: "password" }),
      textField("Repeat the password", opts.confirm, set("confirm"), { type: "password" }),
      h("h2", {}, "Allow"),
      checkField("Printing", opts.print, set("print")),
      checkField("Copying text and images", opts.copy, set("copy")),
      checkField("Editing", opts.modify, set("modify")),
      checkField("Comments and forms", opts.annotate, set("annotate")),
      textField("Owner password (optional)", opts.owner, set("owner"), { type: "password", placeholder: "Lifts the restrictions" }),
    ];
  },
  problem(ctx) {
    const { opts } = ctx.state;
    if (!opts.password) return "Enter a password.";
    if (opts.password !== opts.confirm) return "The passwords don’t match.";
    return keepProblem(ctx.state);
  },
  run(ctx) {
    const { opts, pdf } = ctx.state;
    const args = ["protect", pdf.path, "OUT", `--password=${opts.password}`, ...keepArgs(ctx.state)];
    if (opts.owner) args.push(`--owner=${opts.owner}`);
    for (const key of ["print", "copy", "modify", "annotate"]) if (!opts[key]) args.push(`--no-${key}`);
    return runToFile(args, `${pdf.baseName}-protected.pdf`, `Protected with a password${removedNote(ctx.state)}.`);
  },
};

const unlock = {
  id: "unlock",
  name: "Unlock",
  icon: "unlock",
  desc: "Remove a password (you need to know it) or restrictions.",
  allowLocked: true,
  defaults: () => ({ password: "" }),
  panel(ctx) {
    const { opts, pdf } = ctx.state;
    if (!pdf.info.encrypted) return [h("p", { className: "status" }, "This file has no password or restrictions.")];
    if (!pdf.locked) return [h("p", { className: "status" }, "This file only restricts what can be done with it; no password is needed to remove that.")];
    return [textField("Password", opts.password, (v) => ((opts.password = v), ctx.refresh()), { type: "password" })];
  },
  problem(ctx) {
    const { pdf, opts } = ctx.state;
    if (!pdf.info.encrypted) return "Nothing to unlock.";
    return pdf.locked && !opts.password ? "Enter the password." : null;
  },
  run(ctx) {
    const { opts, pdf } = ctx.state;
    return runToFile(["unlock", pdf.path, "OUT", `--password=${opts.password}`], `${pdf.baseName}-unlocked.pdf`, "Removed the protection.");
  },
};

const rotate = {
  id: "rotate",
  name: "Rotate",
  icon: "rotate",
  desc: "Rotate single pages or all pages.",
  keep: true,
  defaults: () => ({}),
  panel: (ctx) => [h("p", { className: "status" }, "Use the arrows under each page, or rotate them all at once.")],
  toolbar: (ctx) => [
    h("button", { type: "button", onClick: () => (ctx.state.items.forEach((i) => (i.rotation = (i.rotation + 270) % 360)), ctx.refresh()) }, icon("rotate-ccw"), " All left"),
    h("button", { type: "button", onClick: () => (ctx.state.items.forEach((i) => (i.rotation = (i.rotation + 90) % 360)), ctx.refresh()) }, icon("rotate-cw"), " All right"),
    h("button", { type: "button", onClick: () => (ctx.state.items.forEach((i) => (i.rotation = 0)), ctx.refresh()) }, "Reset"),
  ],
  card: (ctx, item) => ({ controls: rotateControls(ctx, item) }),
  problem(ctx) {
    if (ctx.state.items.every((i) => i.keep && i.rotation === 0)) return "Rotate a page first.";
    return keepProblem(ctx.state);
  },
  run(ctx) {
    const { pdf } = ctx.state;
    const pages = kept(ctx.state).map((i) => `${i.n}:${i.rotation}`);
    const turned = kept(ctx.state).filter((i) => i.rotation).length;
    return runToFile(["organize", pdf.path, "OUT", `--pages=${pages.join(",")}`], `${pdf.baseName}-rotated.pdf`, `Rotated ${plural(turned, "page")}${removedNote(ctx.state)}.`);
  },
};

const rearrange = {
  id: "rearrange",
  name: "Rearrange",
  icon: "rearrange",
  desc: "Reorder, duplicate or delete pages.",
  defaults: () => ({}),
  panel: (ctx) => [h("p", { className: "status" }, `${plural(ctx.state.items.length, "page")} in the result.`)],
  toolbar: (ctx) => [
    h("button", { type: "button", onClick: () => (ctx.state.items.reverse(), ctx.refresh()) }, icon("reverse"), " Reverse"),
    h("button", { type: "button", onClick: () => ctx.resetItems() }, icon("reset"), " Reset"),
  ],
  card: (ctx, item, index) => ({
    controls: [
      ...moveControls(ctx, index),
      iconButton("copy", "Duplicate", () => {
        ctx.state.items.splice(index + 1, 0, ctx.newItem(item.pdf, item.n));
        ctx.refresh();
      }),
      iconButton("trash", "Delete", () => (ctx.state.items.splice(index, 1), ctx.refresh()), ctx.state.items.length === 1),
    ],
  }),
  problem: (ctx) => (ctx.state.items.length ? null : "Keep at least one page."),
  run(ctx) {
    const { pdf, items } = ctx.state;
    return runToFile(["organize", pdf.path, "OUT", `--pages=${items.map((i) => i.n).join(",")}`], `${pdf.baseName}-rearranged.pdf`, `Saved ${plural(items.length, "page")}.`);
  },
};

const numbers = {
  id: "numbers",
  name: "Page numbers",
  icon: "page-numbers",
  desc: "Add page numbers with formats like “Page {n} of {total}”.",
  keep: true,
  defaults: () => ({ format: "{n}", position: "bottom-center", start: 1, size: 11, margin: 28, color: "#000000", custom: false, pagesText: "" }),
  panel(ctx) {
    const { opts } = ctx.state;
    const set = (key) => (v) => ((opts[key] = v), ctx.refresh());
    return [
      textField("Format", opts.format, set("format"), { list: "number-formats" }),
      h("datalist", { id: "number-formats" }, ["{n}", "Page {n}", "Page {n} of {total}", "{n} / {total}", "- {n} -"].map((v) => h("option", { value: v }))),
      field("Position", anchorPicker(opts.position, (v) => ((opts.position = v), ctx.rebuild()), { middle: false })),
      numberField("Start at", opts.start, set("start")),
      numberField("Font size (pt)", opts.size, set("size"), { min: 4, max: 72 }),
      numberField("Margin (pt)", opts.margin, set("margin"), { min: 0, max: 200 }),
      field("Colour", h("input", { type: "color", value: opts.color, onInput: (e) => set("color")(e.target.value) })),
      ...pageChoice(ctx),
      h("p", { className: "status" }, "Uses Helvetica, so Latin characters only."),
    ];
  },
  card(ctx, item) {
    if (!isStamped(ctx, item)) return {};
    const { opts } = ctx.state;
    return {
      overlay({ width, points }) {
        const scale = width / points[0];
        const stamped = ctx.state.items.filter((i) => isStamped(ctx, i));
        const index = stamped.indexOf(item);
        const last = opts.start + stamped.length - 1;
        const text = opts.format.replaceAll("{n}", opts.start + index).replaceAll("{total}", last);
        const [v, hz] = opts.position.split("-");
        const style = { fontSize: `${opts.size * scale}px`, color: opts.color };
        const margin = `${opts.margin * scale}px`;
        style[v === "top" ? "top" : "bottom"] = margin;
        if (hz === "center") Object.assign(style, { left: "50%", transform: "translateX(-50%)" });
        else style[hz] = margin;
        return [h("div", { className: "number", style }, text)];
      },
    };
  },
  problem(ctx) {
    const { opts } = ctx.state;
    if (!opts.format.trim()) return "Enter a format.";
    if (/[^\x20-\x7e -ÿ–—‘’“”•…€]/.test(opts.format)) return "The format can only use Latin characters.";
    if (!(opts.size > 0)) return "Enter a font size.";
    return keepProblem(ctx.state) ?? pageChoiceProblem(ctx);
  },
  run(ctx) {
    const { opts, pdf } = ctx.state;
    const pages = stampedPages(ctx);
    const args = ["numbers", pdf.path, "OUT", `--format=${opts.format}`, `--position=${opts.position}`, `--start=${opts.start}`];
    args.push(`--size=${opts.size}`, `--margin=${opts.margin}`, `--color=${opts.color.slice(1)}`, ...keepArgs(ctx.state));
    if (pages) args.push(`--pages=${formatList(pages)}`);
    return runToFile(args, `${pdf.baseName}-numbered.pdf`, (json) => `Numbered ${plural(json.pages, "page")}${removedNote(ctx.state)}.`);
  },
};

const WATERMARK_MARGIN = 0.05;
const FONTS = ["sans-serif", "serif", "monospace", "Helvetica", "Arial", "Georgia", "Times New Roman", "Courier New", "Verdana"];

/** Draws an image (a w×h fraction-of-page-width stamp) where the core puts it. */
function stampOverlay(opts, picture, { width, height, points }) {
  const scale = width / points[0];
  const w = opts.width * points[0];
  const hgt = (w * picture.height) / picture.width;
  const rad = (opts.angle * Math.PI) / 180;
  const [bw, bh] = [Math.abs(w * Math.cos(rad)) + Math.abs(hgt * Math.sin(rad)), Math.abs(w * Math.sin(rad)) + Math.abs(hgt * Math.cos(rad))];
  const margin = WATERMARK_MARGIN * Math.min(points[0], points[1]);
  const [cx, cy] = place(opts.position, points[0], points[1], bw, bh, margin);
  return [
    h("img", {
      className: "stamp",
      src: picture.url,
      style: {
        width: `${w * scale}px`,
        height: `${hgt * scale}px`,
        left: `${(cx - w / 2) * scale}px`,
        top: `${(cy - hgt / 2) * scale}px`,
        opacity: opts.opacity,
        transform: `rotate(${-opts.angle}deg)`,
        mixBlendMode: opts.under ? "multiply" : "normal",
      },
    }),
  ];
}

const watermark = {
  id: "watermark",
  name: "Watermark",
  icon: "watermark",
  desc: "Text in any font, or an image, over or under the content.",
  keep: true,
  defaults: () => ({
    mode: "text",
    text: "CONFIDENTIAL",
    font: "sans-serif",
    bold: true,
    color: "#c62828",
    image: null,
    picture: null,
    width: 0.7,
    angle: 45,
    opacity: 0.3,
    position: "middle-center",
    under: false,
    custom: false,
    pagesText: "",
  }),
  /** Re-draws the text stamp after its text or style changes. */
  async updatePicture(ctx) {
    const { opts } = ctx.state;
    const stamp = (opts.stampVersion = (opts.stampVersion ?? 0) + 1);
    const picture = opts.mode === "image" ? opts.image : opts.text.trim() ? await textPicture(opts.text, opts) : null;
    if (stamp === opts.stampVersion) {
      opts.picture = picture;
      ctx.refresh();
    }
  },
  panel(ctx) {
    const { opts } = ctx.state;
    if (opts.picture === null && opts.mode === "text" && !opts.stampVersion) this.updatePicture(ctx);
    const set = (key) => (v) => ((opts[key] = v), ctx.refresh());
    const restyle = (key) => (v) => ((opts[key] = v), this.updatePicture(ctx));
    const percent = (v) => `${Math.round(v * 100)}%`;
    return [
      segmented(
        [
          ["text", "Text"],
          ["image", "Image"],
        ],
        opts.mode,
        (v) => {
          opts.mode = v;
          this.updatePicture(ctx);
          ctx.rebuild();
        },
      ),
      ...(opts.mode === "text"
        ? [
            textField("Text", opts.text, restyle("text")),
            textField("Font (any installed font)", opts.font, restyle("font"), { list: "fonts" }),
            h("datalist", { id: "fonts" }, FONTS.map((f) => h("option", { value: f }))),
            checkField("Bold", opts.bold, restyle("bold")),
            field("Colour", h("input", { type: "color", value: opts.color, onInput: (e) => restyle("color")(e.target.value) })),
          ]
        : [
            h(
              "button",
              {
                type: "button",
                className: "secondary",
                onClick: async () => {
                  const [file] = await pickFiles("image/png,image/jpeg", false);
                  if (!file) return;
                  opts.image = await filePicture(file);
                  opts.imageName = file.name;
                  this.updatePicture(ctx);
                  ctx.rebuild();
                },
              },
              opts.imageName ? `Image: ${opts.imageName}` : "Choose an image…",
            ),
          ]),
      rangeField("Size", opts.width, set("width"), { min: 0.1, max: 1, step: 0.05, format: percent }),
      rangeField("Angle", opts.angle, set("angle"), { min: -90, max: 90, step: 5, format: (v) => `${v}°` }),
      rangeField("Opacity", opts.opacity, set("opacity"), { min: 0.05, max: 1, step: 0.05, format: percent }),
      field("Position", anchorPicker(opts.position, (v) => ((opts.position = v), ctx.rebuild()))),
      checkField("Behind the page content", opts.under, set("under")),
      ...pageChoice(ctx),
    ];
  },
  card(ctx, item) {
    const { opts } = ctx.state;
    if (!opts.picture || !isStamped(ctx, item)) return {};
    return { overlay: (frame) => stampOverlay(opts, opts.picture, frame) };
  },
  problem(ctx) {
    const { opts } = ctx.state;
    if (opts.mode === "text" && !opts.text.trim()) return "Enter the watermark text.";
    if (opts.mode === "image" && !opts.image) return "Choose an image.";
    if (!opts.picture) return "Preparing the watermark…";
    return keepProblem(ctx.state) ?? pageChoiceProblem(ctx);
  },
  async run(ctx) {
    const { opts, pdf } = ctx.state;
    await put("/stamp/watermark.png", opts.picture.png);
    const pages = stampedPages(ctx);
    const args = ["stamp", pdf.path, "OUT", "--image=/stamp/watermark.png", `--position=${opts.position}`, `--margin=${WATERMARK_MARGIN}`];
    args.push(`--width=${opts.width}`, `--angle=${opts.angle}`, `--opacity=${opts.opacity}`, ...keepArgs(ctx.state));
    if (opts.under) args.push("--under");
    if (pages) args.push(`--pages=${formatList(pages)}`);
    return runToFile(args, `${pdf.baseName}-watermarked.pdf`, (json) => `Watermarked ${plural(json.pages, "page")}${removedNote(ctx.state)}.`);
  },
};

const METADATA_FIELDS = [
  ["title", "Title"],
  ["author", "Author"],
  ["subject", "Subject"],
  ["keywords", "Keywords"],
  ["creator", "Creator (the original application)"],
  ["producer", "Producer (the PDF library)"],
];

const metadata = {
  id: "metadata",
  name: "Metadata",
  icon: "metadata",
  desc: "Edit title, author, subject, keywords, creator and producer.",
  keep: true,
  defaults: (state) => Object.fromEntries(METADATA_FIELDS.map(([key]) => [key, state.pdf.info.metadata?.[key] ?? ""])),
  panel(ctx) {
    const { opts } = ctx.state;
    return METADATA_FIELDS.map(([key, label]) => textField(label, opts[key], (v) => ((opts[key] = v), ctx.refresh())));
  },
  problem: (ctx) => keepProblem(ctx.state),
  run(ctx) {
    const { opts, pdf } = ctx.state;
    const args = ["metadata", pdf.path, "OUT", ...METADATA_FIELDS.map(([key]) => `--${key}=${opts[key]}`), ...keepArgs(ctx.state)];
    return runToFile(args, `${pdf.baseName}-metadata.pdf`, `Saved the details${removedNote(ctx.state)}.`);
  },
};

const SIGNATURE_FONTS = ["cursive", "Snell Roundhand", "Brush Script MT", "Segoe Script", "Bradley Hand", "serif"];

/** The drawing pad; strokes are kept so the pad can be redrawn after a rebuild. */
function signaturePad(ctx) {
  const { opts } = ctx.state;
  const pad = h("canvas", { className: "pad", width: 720, height: 300 });
  const g = pad.getContext("2d");
  const paint = () => {
    g.clearRect(0, 0, pad.width, pad.height);
    g.lineWidth = 5;
    g.lineCap = g.lineJoin = "round";
    g.strokeStyle = opts.ink;
    for (const stroke of opts.strokes) {
      g.beginPath();
      stroke.forEach(([x, y], i) => (i ? g.lineTo(x, y) : g.moveTo(x, y)));
      if (stroke.length === 1) g.lineTo(stroke[0][0] + 0.1, stroke[0][1]);
      g.stroke();
    }
  };
  const point = (e) => {
    const box = pad.getBoundingClientRect();
    return [((e.clientX - box.left) / box.width) * pad.width, ((e.clientY - box.top) / box.height) * pad.height];
  };
  pad.addEventListener("pointerdown", (e) => {
    pad.setPointerCapture(e.pointerId);
    opts.strokes.push([point(e)]);
    paint();
  });
  pad.addEventListener("pointermove", (e) => {
    if (!pad.hasPointerCapture(e.pointerId)) return;
    opts.strokes.at(-1).push(point(e));
    paint();
  });
  pad.addEventListener("pointerup", async () => {
    paint();
    opts.picture = await canvasPicture(trim(pad, 6));
    ctx.refresh();
  });
  paint();
  // After a rebuild (e.g. a new ink colour) the signature is redrawn too.
  if (opts.strokes.length) canvasPicture(trim(pad, 6)).then((picture) => ((opts.picture = picture), ctx.refresh()));
  return pad;
}

const signature = {
  id: "signature",
  name: "Signature",
  icon: "signature",
  desc: "Draw, type or upload a signature and place it on pages.",
  defaults: () => ({ source: "draw", strokes: [], ink: "#1a237e", name: "", font: "cursive", picture: null, upload: null, width: 0.3, placed: [] }),
  async typed(ctx) {
    const { opts } = ctx.state;
    const version = (opts.version = (opts.version ?? 0) + 1);
    const picture = opts.name.trim() ? await textPicture(opts.name, { font: opts.font, color: opts.ink, size: 140 }) : null;
    if (version === opts.version) {
      opts.picture = picture;
      ctx.refresh();
    }
  },
  panel(ctx) {
    const { opts } = ctx.state;
    const choose = (source) => {
      opts.source = source;
      opts.picture = source === "upload" ? opts.upload : null;
      if (source === "type") this.typed(ctx);
      if (source === "draw" && opts.strokes.length) opts.strokes = [];
      ctx.rebuild();
    };
    const source = {
      draw: () => [
        signaturePad(ctx),
        h("button", { type: "button", className: "secondary", onClick: () => ((opts.strokes = []), (opts.picture = null), ctx.rebuild()) }, "Clear"),
      ],
      type: () => [
        textField("Your name", opts.name, (v) => ((opts.name = v), this.typed(ctx))),
        selectField("Style", opts.font, SIGNATURE_FONTS.map((f) => [f, f]), (v) => ((opts.font = v), this.typed(ctx))),
      ],
      upload: () => [
        h(
          "button",
          {
            type: "button",
            className: "secondary",
            onClick: async () => {
              const [file] = await pickFiles("image/png,image/jpeg", false);
              if (!file) return;
              opts.upload = opts.picture = await filePicture(file);
              ctx.rebuild();
            },
          },
          opts.upload ? "Choose another image…" : "Choose an image…",
        ),
      ],
    }[opts.source]();
    return [
      segmented(
        [
          ["draw", "Draw"],
          ["type", "Type"],
          ["upload", "Upload"],
        ],
        opts.source,
        choose,
      ),
      ...source,
      opts.source !== "upload"
        ? field(
            "Ink",
            h("input", {
              type: "color",
              value: opts.ink,
              onInput: async (e) => {
                opts.ink = e.target.value;
                if (opts.source === "type") this.typed(ctx);
                else ctx.rebuild();
              },
            }),
          )
        : null,
      rangeField("Size", opts.width, (v) => ((opts.width = v), ctx.refresh()), { min: 0.08, max: 0.8, step: 0.02, format: (v) => `${Math.round(v * 100)}%` }),
      h("p", { className: "status" }, "Click a page to place the signature; drag it to move it."),
    ];
  },
  /** A placement's box, as fractions of the displayed page. */
  box(opts, placement, points) {
    const w = opts.width;
    const hgt = opts.picture ? (w * opts.picture.height * points[0]) / (opts.picture.width * points[1]) : w / 3;
    const x = Math.min(Math.max(0, placement.cx - w / 2), 1 - w);
    const y = Math.min(Math.max(0, placement.cy - hgt / 2), Math.max(0, 1 - hgt));
    return { x, y, width: w, height: hgt };
  },
  card(ctx, item) {
    const { opts } = ctx.state;
    return {
      onSheetClick: opts.picture
        ? (cx, cy) => {
            opts.placed.push({ page: item.n, cx, cy });
            ctx.refresh();
          }
        : undefined,
      overlay: ({ width, height, points }) => {
        item.points = points;
        return opts.placed
          .filter((p) => p.page === item.n && opts.picture)
          .map((placement) => {
            const box = this.box(opts, placement, points);
            const el = h(
              "div",
              {
                className: "placed",
                style: { left: `${box.x * width}px`, top: `${box.y * height}px`, width: `${box.width * width}px`, height: `${box.height * height}px` },
                onClick: (e) => e.stopPropagation(),
              },
              h("img", { src: opts.picture.url }),
              h("button", { type: "button", title: "Remove", onClick: () => (opts.placed.splice(opts.placed.indexOf(placement), 1), ctx.refresh()) }, "×"),
            );
            el.addEventListener("pointerdown", (e) => {
              if (e.target.tagName === "BUTTON") return;
              el.setPointerCapture(e.pointerId);
              const start = [e.clientX, e.clientY, placement.cx, placement.cy];
              const move = (m) => {
                placement.cx = start[2] + (m.clientX - start[0]) / width;
                placement.cy = start[3] + (m.clientY - start[1]) / height;
                const b = this.box(opts, placement, points);
                Object.assign(el.style, { left: `${b.x * width}px`, top: `${b.y * height}px` });
              };
              el.addEventListener("pointermove", move);
              el.addEventListener("pointerup", () => el.removeEventListener("pointermove", move), { once: true });
            });
            return el;
          });
      },
    };
  },
  problem(ctx) {
    const { opts } = ctx.state;
    if (!opts.picture) return { draw: "Draw your signature.", type: "Type your name.", upload: "Choose an image." }[opts.source];
    return opts.placed.length ? null : "Click a page to place the signature.";
  },
  async run(ctx) {
    const { opts, pdf, items } = ctx.state;
    await put("/stamp/signature.png", opts.picture.png);
    const points = new Map(items.map((i) => [i.n, i.points ?? [612, 792]]));
    const places = opts.placed.map((p) => {
      const b = this.box(opts, p, points.get(p.page));
      return [p.page, b.x, b.y, b.width, b.height].map((v) => +v.toFixed(5)).join(":");
    });
    return runToFile(["signature", pdf.path, "OUT", "--image=/stamp/signature.png", `--place=${places.join(",")}`], `${pdf.baseName}-signed.pdf`, (json) =>
      `Signed ${plural(json.pages, "page")}.`,
    );
  },
};

const grayscale = {
  id: "grayscale",
  name: "Grayscale",
  icon: "grayscale",
  desc: "Convert colours in content, forms, annotations and images.",
  keep: true,
  defaults: () => ({ images: true }),
  panel(ctx) {
    const { opts } = ctx.state;
    return [
      checkField("Convert images too", opts.images, (v) => ((opts.images = v), ctx.refresh())),
      h("p", { className: "status" }, "Text stays real text. Gradients and spot colours are left as they are."),
    ];
  },
  card: () => ({ gray: true }),
  problem: (ctx) => keepProblem(ctx.state),
  run(ctx) {
    const { opts, pdf } = ctx.state;
    const args = ["grayscale", pdf.path, "OUT", ...keepArgs(ctx.state)];
    if (!opts.images) args.push("--text-only");
    return runToFile(args, `${pdf.baseName}-grayscale.pdf`, (json) =>
      `Converted ${plural(json.streams, "content stream")} and ${plural(json.images, "image")}${removedNote(ctx.state)}.`,
    );
  },
};

export const TOOLS = [merge, split, compress, protect, unlock, rotate, rearrange, numbers, watermark, metadata, signature, grayscale, images];
