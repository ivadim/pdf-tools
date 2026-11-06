// The page shell: the sidebar and home screen, opening files, the page grid
// and running the chosen tool.

import { ready, Pdf, put } from "./engine.js";
import { TOOLS, fileColor } from "./tools.js";
import { h, icon, pageCard, sheetStyle, zoom, pickFiles, download } from "./ui.js";
import { VERSION } from "./version.js";

const main = document.getElementById("main");
const nav = document.getElementById("nav");

/** The PDF the single-file tools work on; it stays open when switching tools. */
let sharedPdf = null;
/** The open tool's ctx, or null on the home screen. */
let current = null;
let currentPage = "home";

/** replaceChildren, skipping the nulls left by conditional parts. */
const fill = (el, ...children) => el.replaceChildren(...children.flat().filter((c) => c !== null && c !== undefined && c !== false));

const isPdf = (file) => file.type === "application/pdf" || /\.pdf$/i.test(file.name);
const isImage = (file) => /^image\/(png|jpeg)$/.test(file.type) || /\.(png|jpe?g)$/i.test(file.name);

function renderNav() {
  nav.replaceChildren(
    navButton("home", "Home", currentPage === "home", showHome),
    ...TOOLS.map((tool) => navButton(tool.icon, tool.name, current?.tool === tool, () => openTool(tool))),
    navButton("info", "About", currentPage === "about", showAbout),
  );
}

function navButton(iconName, label, active, onClick) {
  return h("button", { type: "button", className: active ? "active" : "", onClick }, icon(iconName), label);
}

function showHome(message, { push = true } = {}) {
  current = null;
  currentPage = "home";
  setRoute(null, push, "home");
  const toolButton = (tool) => h(
    "button",
    { type: "button", onClick: () => openTool(tool) },
    h("span", { className: "tile-icon" }, icon(tool.icon)),
    h("span", { className: "tile-text" }, h("strong", {}, tool.name), h("span", { className: "desc" }, tool.desc)),
  );
  main.replaceChildren(
    ...[
    h("header", { className: "page-head" }, h("h1", {}, "Choose a tool"), h("p", { className: "subtitle" }, "Everything runs on this device; nothing is uploaded.")),
    sharedPdf ? h("div", { className: "notice" }, `${sharedPdf.name} is open. Choose what to do with it.`) : null,
    typeof message === "string" ? h("div", { className: "notice" }, message) : null,
    h("section", { className: "home-section" },
      h("h2", {}, "Common tools"),
      h("div", { className: "home" }, TOOLS.slice(0, 4).map(toolButton)),
    ),
    h("section", { className: "home-section" },
      h("h2", {}, "More tools"),
      h("div", { className: "home home-more" }, TOOLS.slice(4).map(toolButton)),
    ),
    ].filter(Boolean),
  );
}

function showAbout({ push = true } = {}) {
  current = null;
  currentPage = "about";
  setRoute(null, push, "about");
  const copyText = `PDF Tools ${VERSION} (Web)`;
  const copyStatus = h("span", { className: "about-copy-status", role: "status", "aria-live": "polite" });
  const copyButton = h("button", {
    type: "button",
    className: "about-copy",
    onClick: async () => {
      try {
        await navigator.clipboard.writeText(copyText);
        copyButton.textContent = "Copied";
        copyStatus.textContent = "Version copied to clipboard.";
      } catch {
        copyStatus.textContent = "Clipboard access isn't available in this browser.";
      }
    },
  }, "Copy");
  const externalLink = (label, url, iconName = "external") =>
    h("a", { href: url, target: "_blank", rel: "noopener noreferrer" }, icon(iconName), label);
  const credits = [
    ["lopdf", "Reading, editing and writing PDF files", "https://crates.io/crates/lopdf"],
    ["hayro", "Rendering page previews", "https://crates.io/crates/hayro"],
    ["image", "Reading and writing JPEG and PNG images", "https://crates.io/crates/image"],
    ["Parley", "Text layout for watermarks and signatures", "https://crates.io/crates/parley"],
    ["tiny-skia", "Drawing and transforming document content", "https://crates.io/crates/tiny-skia"],
  ];
  main.replaceChildren(
    h("article", { className: "about" },
      h("header", { className: "about-header" },
        h("img", { className: "about-logo", src: "icon.png", alt: "" }),
        h("div", { className: "about-identity" },
          h("h1", {}, "PDF Tools"),
          h("div", { className: "about-version" }, `Version ${VERSION} · Web`, copyButton, copyStatus),
          h("p", {}, "Merge, split, protect and edit PDFs — on your computer."),
        ),
      ),
      h("div", { className: "about-actions" },
        h("a", { className: "primary", href: "https://github.com/ivadim/pdf-tools", target: "_blank", rel: "noopener noreferrer" }, icon("github"), "GitHub"),
        h("a", { className: "secondary", href: "https://github.com/ivadim/pdf-tools/releases", target: "_blank", rel: "noopener noreferrer" }, icon("external"), "Releases and updates"),
        h("a", { className: "secondary", href: "https://github.com/ivadim/pdf-tools/issues/new", target: "_blank", rel: "noopener noreferrer" }, icon("external"), "Report a problem"),
        h("a", { className: "secondary", href: "https://buymeacoffee.com/ivadim", target: "_blank", rel: "noopener noreferrer" }, icon("coffee"), "Buy me a coffee"),
      ),
      h("section", { className: "about-section" },
        h("h2", {}, "Privacy"),
        h("p", {}, "Everything runs on your computer. PDF Tools never uploads your files. The Web version uses WebAssembly to process documents in your browser; your browser connects to this site to load and update the app."),
      ),
      h("section", { className: "about-section" },
        h("h2", {}, "Licence"),
        h("p", {}, "PDF Tools is free and open-source software, released under the MIT licence. Copyright © 2026 Dmitry Ivanov."),
        h("div", { className: "about-links" },
          externalLink("MIT licence", "https://github.com/ivadim/pdf-tools/blob/main/LICENSE"),
          externalLink("Source code", "https://github.com/ivadim/pdf-tools", "github"),
        ),
      ),
      h("section", { className: "about-section" },
        h("h2", {}, "Built with"),
        h("p", {}, "PDF Tools stands on these open-source projects, among others:"),
        h("dl", { className: "about-credits" }, credits.map(([name, purpose, url]) =>
          h("div", { className: "about-credit" },
            h("dt", {}, externalLink(name, url)),
            h("dd", {}, purpose),
          ),
        )),
      ),
    ),
  );
}

// ── Navigation ─────────────────────────────────────────────────────────

const back = document.getElementById("back");
const barTitle = document.getElementById("bar-title");
const menuButton = document.getElementById("menu");
const scrim = document.getElementById("scrim");

/** Each tool gets a history entry, so Back (browser, phone gesture or app bar) returns home. */
function setRoute(tool, push, page = "tool") {
  const hash = tool ? `#${tool.id}` : page === "about" ? "#about" : "";
  const url = hash || location.pathname + location.search;
  if (push && location.hash !== hash) history.pushState({ fromHome: main.dataset.view === "home" }, "", url);
  else history.replaceState(history.state, "", url);
  document.title = tool ? `${tool.name} · PDF Tools` : page === "about" ? "About · PDF Tools" : "PDF Tools";
  barTitle.textContent = tool ? tool.name : page === "about" ? "About" : "PDF Tools";
  back.hidden = !tool && page !== "about";
  document.getElementById("bar-logo").hidden = Boolean(tool);
  main.dataset.view = tool ? "tool" : page;
  document.body.classList.remove("has-actions");
  setMenu(false);
  renderNav();
  window.scrollTo(0, 0);
}

window.addEventListener("popstate", () => {
  const tool = TOOLS.find((t) => t.id === location.hash.slice(1));
  if (tool) openTool(tool, { push: false });
  else if (location.hash === "#about") showAbout({ push: false });
  else showHome(undefined, { push: false });
});

back.addEventListener("click", () => {
  // Return to the home entry this tool was opened from, if there is one.
  if (history.state?.fromHome) history.back();
  else showHome();
});

// The options can be collapsed on small screens; wide layouts always show them.
matchMedia("(min-width: 900px)").addEventListener("change", (event) => {
  if (event.matches) for (const panel of document.querySelectorAll("details.panel")) panel.open = true;
  setMenu(false);
});

function setMenu(open) {
  document.body.classList.toggle("menu-open", open);
  menuButton.setAttribute("aria-expanded", String(open));
  scrim.hidden = !open;
}
menuButton.addEventListener("click", () => setMenu(!document.body.classList.contains("menu-open")));
scrim.addEventListener("click", () => setMenu(false));
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") setMenu(false);
});

// ── Opening a tool ─────────────────────────────────────────────────────

let nextImage = 0;

function openTool(tool, { push = true } = {}) {
  const state = { tool, pdf: null, files: [], items: [], opts: {}, result: null, error: null, busy: false, editedFields: new Set() };
  const pagesBox = h("div", { className: "workspace" });
  const optionsBox = h("div", { className: "options" });
  // Collapsible on small screens, where the options sit above the pages.
  const panelBox = h("details", { className: "panel", open: true }, h("summary", {}, "Options"), optionsBox);
  const actionsBox = h("div", { className: "actions", hidden: true });
  // On small screens the actions are pinned over the page; leave room for them.
  new ResizeObserver(() => document.documentElement.style.setProperty("--actions-h", `${actionsBox.offsetHeight}px`)).observe(actionsBox);
  const ctx = {
    tool,
    state,
    refresh: () => draw(ctx, { panel: tool.id === "merge" || tool.id === "rearrange" || tool.id === "redact" }),
    rebuild: () => draw(ctx, { panel: true }),
    move(index, delta) {
      const [item] = state.items.splice(index, 1);
      state.items.splice(index + delta, 0, item);
      ctx.refresh();
    },
    newItem: (pdf, n) => ({ pdf, n, rotation: 0, keep: true, cache: {} }),
    resetItems() {
      state.items = [];
      for (const pdf of state.pdf ? [state.pdf] : state.files) {
        for (let n = 1; n <= pdf.pageCount; n++) state.items.push(ctx.newItem(pdf, n));
      }
      ctx.refresh();
    },
    async addFiles(list) {
      if (tool.id === "redact" && state.busy) return;
      const files = list ?? (await pickFiles(tool.kind === "images" ? "image/png,image/jpeg" : "application/pdf,.pdf", tool.kind !== undefined));
      await addFiles(ctx, files);
    },
    pagesBox,
    panelBox,
    optionsBox,
    footer: actionsBox,
  };
  if (tool.kind) state.opts = tool.defaults(state);
  current = ctx;
  currentPage = "tool";
  setRoute(tool, push);
  main.replaceChildren(
    h(
      "div",
      { className: "tool" },
      h("header", { className: "page-head" }, h("h1", {}, tool.name), h("p", { className: "subtitle" }, tool.desc)),
      pagesBox,
      h("div", { className: "side" }, panelBox, actionsBox),
    ),
  );
  if (!tool.kind && sharedPdf) usePdf(ctx, sharedPdf);
  else if (tool.kind === "merge" && sharedPdf && !sharedPdf.locked) {
    state.files.push(sharedPdf);
    ctx.resetItems();
  }
  ctx.rebuild();
}

function usePdf(ctx, pdf) {
  const { state, tool } = ctx;
  sharedPdf = pdf;
  state.pdf = pdf;
  state.result = state.error = null;
  state.editedFields.clear();
  state.items = [];
  // Before the pages are drawn: drawing checks the options.
  state.opts = !pdf.locked || tool.allowLocked ? tool.defaults(state) : {};
  ctx.resetItems();
}

async function addFiles(ctx, files) {
  const { state, tool } = ctx;
  state.error = null;
  try {
    if (tool.kind === "images") {
      for (const file of files.filter(isImage)) {
        const path = `/img/${++nextImage}`;
        await put(path, await file.arrayBuffer());
        const url = URL.createObjectURL(file);
        const img = new Image();
        img.src = url;
        await img.decode();
        state.items.push({ name: file.name, path, url, width: img.naturalWidth, height: img.naturalHeight, rotation: 0 });
      }
    } else {
      const pdfs = files.filter(isPdf);
      if (!pdfs.length) throw new Error("Choose a PDF file.");
      if (tool.kind === "merge") {
        for (const file of pdfs) {
          const pdf = await Pdf.open(file);
          if (pdf.locked) throw new Error(`${file.name} is password-protected. Unlock it first.`);
          state.files.push(pdf);
          for (let n = 1; n <= pdf.pageCount; n++) state.items.push(ctx.newItem(pdf, n));
        }
      } else {
        usePdf(ctx, await Pdf.open(pdfs[0]));
      }
    }
  } catch (error) {
    state.error = error.message;
  }
  state.result = null;
  ctx.rebuild();
}

// ── Drawing ────────────────────────────────────────────────────────────

function draw(ctx, { panel }) {
  const { state, tool } = ctx;
  const hasInput = tool.kind ? state.items.length || state.files.length : state.pdf;
  ctx.pagesBox.replaceChildren(...(hasInput ? pagesView(ctx) : [dropZone(ctx)]));
  if (panel) {
    const options = hasInput && (!state.pdf?.locked || tool.allowLocked) ? tool.panel(ctx).flat().filter(Boolean) : [];
    ctx.optionsBox.replaceChildren(...options);
    ctx.panelBox.hidden = !options.length;
  }
  drawFooter(ctx, hasInput);
}

function dropZone(ctx) {
  const { tool, state } = ctx;
  const what = tool.kind === "images" ? "JPEG or PNG images" : tool.kind === "merge" ? "PDF files" : "a PDF file";
  return h(
    "div",
    {},
    state.error ? h("p", { className: "error" }, state.error) : null,
    h("div", {
      className: "drop", role: "button", tabIndex: 0,
      onClick: () => ctx.addFiles(),
      onKeyDown: (event) => {
        if (event.key === "Enter" || event.key === " ") { event.preventDefault(); ctx.addFiles(); }
      },
    }, h("strong", {}, `Choose ${what}`), h("span", { className: "drop-hint" }, "or drop them anywhere on this page")),
  );
}

function pagesView(ctx) {
  const { state, tool } = ctx;
  const out = [];
  if (state.error) out.push(h("p", { className: "error" }, state.error));

  if (tool.kind === "merge") {
    out.push(
      h(
        "div",
        { className: "files" },
        state.files.map((pdf, i) =>
          h(
            "span",
            { className: "chip" },
            h("span", { className: "swatch", style: { background: fileColor(i) } }),
            pdf.name,
            h(
              "button",
              {
                type: "button",
                className: "icon-button",
                title: "Remove",
                onClick: () => {
                  state.files.splice(i, 1);
                  state.items = state.items.filter((item) => item.pdf !== pdf);
                  ctx.rebuild();
                },
              },
              "×",
            ),
          ),
        ),
      ),
    );
  } else if (state.pdf) {
    const { pdf } = state;
    out.push(
      h(
        "div",
        { className: "files" },
        h("span", { className: "chip" }, `${pdf.name} · ${pdf.locked ? "locked" : `${pdf.pageCount} pages`}`),
        h("button", { type: "button", className: "secondary", onClick: () => ctx.addFiles() }, "Choose another…"),
      ),
    );
    if (pdf.locked && !tool.allowLocked) {
      out.push(
        h(
          "div",
          { className: "notice" },
          `${pdf.name} is password-protected.`,
          h("button", { type: "button", className: "primary", onClick: () => openTool(TOOLS.find((t) => t.id === "unlock")) }, "Unlock it"),
        ),
      );
      return out;
    }
    if (pdf.locked) {
      out.push(h("p", { className: "status" }, "Pages are shown once the file is unlocked."));
      return out;
    }
    if (pdf.info.repaired) out.push(h("div", { className: "notice" }, "The file is damaged; the pages that could be recovered are shown."));
  }

  const toolbar = tool.toolbar?.(ctx) ?? [];
  if (tool.keep && state.items.length > 1) {
    const all = state.items.every((i) => i.keep);
    toolbar.push(
      h("button", { type: "button", onClick: () => (state.items.forEach((i) => (i.keep = !all)), ctx.refresh()) }, icon("select-all"), all ? " Leave all out" : " Keep all"),
    );
  }
  if (toolbar.length) out.push(h("div", { className: "toolbar" }, toolbar));

  const grid = h("div", { className: "pages" });
  state.items.forEach((item, index) => grid.append(tool.kind === "images" ? imageCard(ctx, item, index) : pdfCard(ctx, item, index)));
  out.push(grid);
  return out;
}

let lastClicked = null;

function pdfCard(ctx, item, index) {
  const { state, tool } = ctx;
  const extra = tool.card?.(ctx, item, index) ?? {};
  const label = [];
  if (tool.keep) {
    label.push(
      h("input", {
        type: "checkbox",
        checked: item.keep,
        title: "Keep this page (Shift-click for a range)",
        onClick: (event) => {
          const keep = event.target.checked;
          if (event.shiftKey && lastClicked !== null) {
            const [from, to] = [Math.min(lastClicked, index), Math.max(lastClicked, index)];
            for (let i = from; i <= to; i++) state.items[i].keep = keep;
          } else item.keep = keep;
          lastClicked = index;
          ctx.refresh();
        },
      }),
    );
  }
  if (tool.kind === "merge") label.push(h("span", { className: "swatch", style: { width: "9px", height: "9px", borderRadius: "2px", background: fileColor(state.files.indexOf(item.pdf)) } }));
  label.push(`Page ${item.n}`);
  if (tool.id === "split") {
    try {
      const output = tool.parts(ctx).findIndex((pages) => pages.includes(item.n));
      if (output >= 0) label.push(` · File ${output + 1}`);
    } catch { /* The range field is incomplete; the preview has no output group yet. */ }
  }
  return pageCard({
    picture: item.pdf.thumbs.get(item.n),
    rotation: item.rotation,
    cache: item.cache,
    label,
    removed: tool.keep && !item.keep,
    onZoom: () => zoom(item.pdf, item.n, item.rotation),
    ...extra,
  });
}

/** An image on the page it will become. */
function imageCard(ctx, item, index) {
  const { state, tool } = ctx;
  const layout = tool.layout(state.opts, item);
  const [pw, ph] = layout.page;
  // The image's box before it's turned, as a share of the page.
  const quarter = item.rotation % 180 === 90;
  const [bw, bh] = quarter ? [layout.height, layout.width] : [layout.width, layout.height];
  const img = h("img", {
    src: item.url,
    alt: "",
    style: {
      position: "absolute",
      width: `${(bw / pw) * 100}%`,
      height: `${(bh / ph) * 100}%`,
      left: `${((pw - bw) / 2 / pw) * 100}%`,
      top: `${((ph - bh) / 2 / ph) * 100}%`,
      transform: `rotate(${item.rotation}deg)`,
    },
  });
  return h(
    "div",
    { className: "card" },
    h("div", { className: "thumb" }, h("div", { className: "sheet still", style: sheetStyle(pw, ph) }, img)),
    h("div", { className: "label", title: item.name }, h("span", { className: "name" }, item.name)),
    h(
      "div",
      { className: "controls" },
      iconBtn("chevron-left", "Move earlier", () => ctx.move(index, -1), index === 0),
      iconBtn("chevron-right", "Move later", () => ctx.move(index, 1), index === state.items.length - 1),
      iconBtn("rotate-ccw", "Rotate left", () => ((item.rotation = (item.rotation + 270) % 360), ctx.refresh())),
      iconBtn("rotate-cw", "Rotate right", () => ((item.rotation = (item.rotation + 90) % 360), ctx.refresh())),
      iconBtn("trash", "Remove", () => (state.items.splice(index, 1), ctx.rebuild())),
    ),
  );
}

const iconBtn = (name, title, onClick, disabled = false) =>
  h("button", { type: "button", className: "icon-button", title, ariaLabel: title, disabled, onClick }, icon(name));

function drawFooter(ctx, hasInput) {
  const { state, tool } = ctx;
  ctx.footer.hidden = !hasInput || (state.pdf?.locked && !tool.allowLocked);
  document.body.classList.toggle("has-actions", !ctx.footer.hidden);
  if (ctx.footer.hidden) return;
  const problem = state.busy ? null : tool.problem(ctx);
  const isFieldError = (tool.id === "split" && (state.editedFields.has("ranges") || state.editedFields.has("extract") || state.editedFields.has("base")))
    || (tool.id === "protect" && (state.editedFields.has("password") || state.editedFields.has("confirm")));
  const summary = tool.id === "split" && !problem
      ? (() => {
          const parts = tool.parts(ctx);
          const included = new Set(parts.flat()).size;
          const omitted = Math.max(0, (state.pdf?.pageCount ?? 0) - included);
          const result = `${parts.length} output files · ${parts.reduce((n, pages) => n + pages.length, 0)} pages`;
          return omitted ? `${result} · ${omitted} ${omitted === 1 ? "page" : "pages"} left out` : result;
        })()
      : tool.id === "protect" && state.pdf
        ? `${state.pdf.pageCount} pages · Password to open`
        : null;
  const actionLabel = { numbers: "Add page numbers", images: "Create PDF", metadata: "Save metadata", signature: "Sign", grayscale: "Convert to grayscale" }[tool.id] ?? tool.name;
  const button = h("button", { type: "button", className: "primary", disabled: Boolean(problem) || state.busy, onClick: () => runTool(ctx) }, state.busy ? `${actionLabel} in progress…` : actionLabel);
  fill(ctx.footer, 
    h("div", { className: "field" }, summary ? h("p", { className: "status" }, summary) : null, problem ? h("p", { className: isFieldError ? "problem is-error" : "problem" }, problem) : null, button),
    state.result ? results(ctx) : null,
    state.runError ? h("p", { className: "error" }, state.runError) : null,
  );
}

/** "a/b:c" → "a_b_c": characters file names can't contain. */
const cleanName = (name) => name.replace(/[/\\:*?"<>|]/g, "_").trim();

/** A result file: its name, editable (the extension stays), and its download link. */
function resultFile(file) {
  const ext = /\.[^./]+$/.exec(file.name)?.[0] ?? "";
  const original = file.name.slice(0, file.name.length - ext.length);
  const link = download(file.name, file.data);
  const rename = (base) => {
    file.name = (cleanName(base) || original) + ext;
    link.download = file.name;
    link.textContent = `Download ${file.name}`;
  };
  const input = h("input", { type: "text", value: original, spellcheck: false, "aria-label": "File name", onInput: (e) => rename(e.target.value) });
  return { link, row: h("div", { className: "result-file" }, h("label", { className: "name" }, input, h("span", { className: "ext" }, ext)), link) };
}

function results(ctx) {
  const { files, message } = ctx.state.result;
  const rows = files.map(resultFile);
  const links = rows.map((r) => r.link);
  return h(
    "div",
    { className: "results" },
    h("span", { className: "done" }, message),
    rows.map((r) => r.row),
    links.length > 1
      ? h("button", { type: "button", className: "secondary", onClick: () => links.forEach((a, i) => setTimeout(() => a.click(), i * 250)) }, "Download all")
      : null,
    links.length === 1
      ? h(
          "button",
          {
            type: "button",
            className: "secondary",
            onClick: async () => {
              const pdf = await Pdf.open(new File([files[0].data], files[0].name, { type: "application/pdf" }));
              sharedPdf = pdf;
              showHome(`${pdf.name} is open. Choose the next tool.`);
            },
          },
          "Continue with this file",
        )
      : null,
  );
}

async function runTool(ctx) {
  const { state, tool } = ctx;
  Object.assign(state, { busy: true, result: null, runError: null });
  ctx.refresh();
  const started = performance.now();
  try {
    const result = await tool.run(ctx);
    const ms = performance.now() - started;
    const took = ms < 1000 ? `${Math.max(1, Math.round(ms))} ms` : `${(ms / 1000).toFixed(1)} s`;
    state.result = { ...result, message: `${result.message} (${took})` };
  } catch (error) {
    state.runError = error.message;
  }
  state.busy = false;
  ctx.refresh();
}

// ── Dropping files anywhere ────────────────────────────────────────────

document.addEventListener("dragover", (event) => {
  event.preventDefault();
  document.body.classList.add("dragging");
});
document.addEventListener("dragleave", (event) => {
  if (!event.relatedTarget) document.body.classList.remove("dragging");
});
document.addEventListener("drop", (event) => {
  event.preventDefault();
  document.body.classList.remove("dragging");
  openFiles([...event.dataTransfer.files]);
});

/** Files dropped on the page or opened with the installed app. */
async function openFiles(files) {
  if (!files.length) return;
  if (!current) {
    if (files.every(isImage)) openTool(TOOLS.find((t) => t.id === "images"));
    else if (files.filter(isPdf).length > 1) openTool(TOOLS.find((t) => t.id === "merge"));
    else {
      const file = files.find(isPdf);
      if (!file) return;
      try {
        sharedPdf = await Pdf.open(file);
        showHome();
      } catch (error) {
        showHome(error.message);
      }
      return;
    }
  }
  current.addFiles(files);
}

// ── Installing as an app ───────────────────────────────────────────────

if ("serviceWorker" in navigator && location.protocol !== "file:") {
  navigator.serviceWorker.register("sw.js").catch((error) => console.warn("No offline support:", error));
}

// Chrome and Edge offer installing; the sidebar shows a button while they do.
const install = document.getElementById("install");
let installPrompt = null;
window.addEventListener("beforeinstallprompt", (event) => {
  event.preventDefault();
  installPrompt = event;
  install.hidden = false;
});
install.addEventListener("click", async () => {
  if (!installPrompt) return;
  installPrompt.prompt();
  await installPrompt.userChoice;
  installPrompt = null;
  install.hidden = true;
});
window.addEventListener("appinstalled", () => (install.hidden = true));

// PDFs and images opened with the installed app (its file handlers).
if ("launchQueue" in window) {
  window.launchQueue.setConsumer(async (params) => {
    if (!params.files?.length) return;
    const files = await Promise.all(params.files.map((handle) => handle.getFile()));
    await ready;
    openFiles(files);
  });
}

document.getElementById("brand").addEventListener("click", () => showHome());

ready.then(
  () => {
    const tool = TOOLS.find((t) => t.id === location.hash.slice(1));
    if (tool) openTool(tool);
    else if (location.hash === "#about") showAbout({ push: false });
    else showHome();
  },
  (error) => main.replaceChildren(h("p", { className: "error" }, `The PDF engine couldn’t start: ${error.message}`)),
);
renderNav();
