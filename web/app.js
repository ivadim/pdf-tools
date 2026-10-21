// The page shell: the sidebar and home screen, opening files, the page grid
// and running the chosen tool.

import { ready, Pdf, put } from "./engine.js";
import { TOOLS, fileColor } from "./tools.js";
import { h, icon, pageCard, zoom, pickFiles, download } from "./ui.js";

const main = document.getElementById("main");
const nav = document.getElementById("nav");

/** The PDF the single-file tools work on; it stays open when switching tools. */
let sharedPdf = null;
/** The open tool's ctx, or null on the home screen. */
let current = null;

/** replaceChildren, skipping the nulls left by conditional parts. */
const fill = (el, ...children) => el.replaceChildren(...children.flat().filter((c) => c !== null && c !== undefined && c !== false));

const isPdf = (file) => file.type === "application/pdf" || /\.pdf$/i.test(file.name);
const isImage = (file) => /^image\/(png|jpeg)$/.test(file.type) || /\.(png|jpe?g)$/i.test(file.name);

function renderNav() {
  nav.replaceChildren(
    navButton("home", "Home", !current, showHome),
    ...TOOLS.map((tool) => navButton(tool.icon, tool.name, current?.tool === tool, () => openTool(tool))),
  );
}

function navButton(iconName, label, active, onClick) {
  return h("button", { type: "button", className: active ? "active" : "", onClick }, icon(iconName), label);
}

function showHome(message) {
  current = null;
  history.replaceState(null, "", location.pathname);
  renderNav();
  main.replaceChildren(
    ...[
    h("h1", {}, "PDF Tools"),
    h("p", { className: "subtitle" }, "Everything runs in this tab; nothing is uploaded."),
    sharedPdf ? h("div", { className: "notice" }, `${sharedPdf.name} is open. Choose what to do with it.`) : null,
    typeof message === "string" ? h("div", { className: "notice" }, message) : null,
    h(
      "div",
      { className: "home" },
      TOOLS.map((tool) =>
        h("button", { type: "button", onClick: () => openTool(tool) }, icon(tool.icon), h("strong", {}, tool.name), h("span", { className: "desc" }, tool.desc)),
      ),
    ),
    ].filter(Boolean),
  );
}

// ── Opening a tool ─────────────────────────────────────────────────────

let nextImage = 0;

function openTool(tool) {
  const state = { tool, pdf: null, files: [], items: [], opts: {}, result: null, error: null, busy: false };
  const pagesBox = h("div");
  const panelBox = h("div", { className: "panel" });
  const ctx = {
    tool,
    state,
    refresh: () => draw(ctx, { panel: tool.id === "merge" || tool.id === "rearrange" }),
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
      const files = list ?? (await pickFiles(tool.kind === "images" ? "image/png,image/jpeg" : "application/pdf,.pdf", tool.kind !== undefined));
      await addFiles(ctx, files);
    },
    pagesBox,
    panelBox,
  };
  if (tool.kind) state.opts = tool.defaults(state);
  current = ctx;
  history.replaceState(null, "", `#${tool.id}`);
  renderNav();
  main.replaceChildren(
    h("h1", {}, tool.name),
    h("p", { className: "subtitle" }, tool.desc),
    h("div", { className: "tool" }, pagesBox, panelBox),
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
  state.items = [];
  ctx.resetItems();
  if (!pdf.locked || tool.allowLocked) state.opts = tool.defaults(state);
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
    const options = hasInput && (!state.pdf?.locked || tool.allowLocked) ? tool.panel(ctx) : [];
    ctx.panelBox.replaceChildren(...options.flat().filter(Boolean), (ctx.footer = h("div", { className: "footer" })));
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
    h("div", { className: "drop", onClick: () => ctx.addFiles() }, h("strong", {}, `Choose ${what}`), "or drop them anywhere on this page"),
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
  const scale = Math.min(170 / layout.page[0], 200 / layout.page[1]);
  const [pw, ph] = [layout.page[0] * scale, layout.page[1] * scale];
  const quarter = item.rotation % 180 === 90;
  const [w, hgt] = [layout.width * scale, layout.height * scale];
  const img = h("img", {
    src: item.url,
    style: {
      position: "absolute",
      width: `${quarter ? hgt : w}px`,
      height: `${quarter ? w : hgt}px`,
      left: `${(pw - (quarter ? hgt : w)) / 2}px`,
      top: `${(ph - (quarter ? w : hgt)) / 2}px`,
      transform: `rotate(${item.rotation}deg)`,
    },
  });
  return h(
    "div",
    { className: "card" },
    h("div", { className: "thumb" }, h("div", { className: "sheet", style: { width: `${pw}px`, height: `${ph}px`, cursor: "default" } }, img)),
    h("div", { className: "label", title: item.name }, item.name.length > 22 ? `${item.name.slice(0, 20)}…` : item.name),
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
  h("button", { type: "button", className: "icon-button", title, disabled, onClick }, icon(name));

function drawFooter(ctx, hasInput) {
  const { state, tool } = ctx;
  if (!ctx.footer) return;
  if (!hasInput) {
    fill(ctx.footer, h("p", { className: "status" }, tool.kind === "images" ? "Add images to begin." : "Choose a file to begin."));
    return;
  }
  if (state.pdf?.locked && !tool.allowLocked) {
    fill(ctx.footer, );
    return;
  }
  const problem = state.busy ? null : tool.problem(ctx);
  const button = h("button", { type: "button", className: "primary", disabled: Boolean(problem) || state.busy, onClick: () => runTool(ctx) }, state.busy ? "Working…" : `Run ${tool.name}`);
  fill(ctx.footer, 
    h("div", { className: "field" }, problem ? h("p", { className: "problem" }, problem) : null, button),
    state.result ? results(ctx) : null,
    state.runError ? h("p", { className: "error" }, state.runError) : null,
  );
}

function results(ctx) {
  const { files, message } = ctx.state.result;
  const links = files.map((f) => download(f.name, f.data));
  return h(
    "div",
    { className: "results" },
    h("span", { className: "done" }, message),
    links,
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
document.addEventListener("drop", async (event) => {
  event.preventDefault();
  document.body.classList.remove("dragging");
  const files = [...event.dataTransfer.files];
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
});

document.getElementById("brand").addEventListener("click", () => showHome());

ready.then(
  () => {
    const tool = TOOLS.find((t) => t.id === location.hash.slice(1));
    if (tool) openTool(tool);
    else showHome();
  },
  (error) => main.replaceChildren(h("p", { className: "error" }, `The PDF engine couldn’t start: ${error.message}`)),
);
renderNav();
