// Building blocks for the tools: elements, form fields, the page grid and
// the full-size page view.

export function h(tag, props = {}, ...children) {
  const el = document.createElement(tag);
  for (const [key, value] of Object.entries(props ?? {})) {
    if (value === undefined || value === null || value === false) continue;
    if (key.startsWith("on")) el.addEventListener(key.slice(2).toLowerCase(), value);
    else if (key === "style" && typeof value === "object") {
      for (const [name, v] of Object.entries(value)) {
        if (name.startsWith("--")) el.style.setProperty(name, v);
        else el.style[name] = v;
      }
    }
    else if (key in el && key !== "list") el[key] = value;
    else el.setAttribute(key, value === true ? "" : value);
  }
  for (const child of children.flat(Infinity)) {
    if (child !== null && child !== undefined && child !== false) el.append(child instanceof Node ? child : String(child));
  }
  return el;
}

export const icon = (name) => h("span", { className: "icon", style: { "--icon": `url(icons/${name}.svg)` } });

export function iconButton(name, title, onClick, disabled = false) {
  return h("button", { type: "button", className: "icon-button", title, ariaLabel: title, disabled, onClick }, icon(name));
}

export function field(label, control) {
  return h("label", { className: "field" }, h("span", {}, label), control);
}

export function textField(label, value, onInput, { type = "text", placeholder = "", list } = {}) {
  const input = h("input", { type, value, placeholder, onInput: () => onInput(input.value) });
  if (list) input.setAttribute("list", list);
  return field(label, input);
}

export function numberField(label, value, onInput, { min, max, step = 1 } = {}) {
  const input = h("input", { type: "number", value, min, max, step, onInput: () => onInput(Number(input.value)) });
  return field(label, input);
}

export function rangeField(label, value, onInput, { min, max, step, format }) {
  const shown = h("span", {}, format(value));
  const input = h("input", {
    type: "range",
    min,
    max,
    step,
    value,
    onInput: () => {
      shown.textContent = format(Number(input.value));
      onInput(Number(input.value));
    },
  });
  return h("label", { className: "field" }, h("span", {}, `${label} `, shown), input);
}

export function selectField(label, value, options, onChange) {
  const select = h(
    "select",
    { onChange: () => onChange(select.value) },
    options.map(([v, text]) => h("option", { value: v, selected: v === value }, text)),
  );
  return field(label, select);
}

export function checkField(label, checked, onChange) {
  const input = h("input", { type: "checkbox", checked, onChange: () => onChange(input.checked) });
  return h("label", { className: "check" }, input, label);
}

export function segmented(options, value, onChange) {
  return h(
    "div",
    { className: "segmented" },
    options.map(([v, text]) => h("button", { type: "button", className: v === value ? "active" : "", onClick: () => onChange(v) }, text)),
  );
}

const VERTICAL = ["top", "middle", "bottom"];
const HORIZONTAL = ["left", "center", "right"];

/** A 3×3 (or 2×3 without the middle row) picker of "top-left"… positions. */
export function anchorPicker(value, onChange, { middle = true } = {}) {
  const rows = middle ? VERTICAL : ["top", "bottom"];
  return h(
    "div",
    { className: "anchors" },
    rows.flatMap((v) =>
      HORIZONTAL.map((hz) => {
        const name = `${v}-${hz}`;
        return h("button", { type: "button", title: name.replace("-", " "), className: name === value ? "active" : "", onClick: () => onChange(name) });
      }),
    ),
  );
}

/** Where a w×h box goes on a page at `anchor`, `margin` from the edges; its centre. */
export function place(anchor, pageW, pageH, w, h, margin) {
  const [v, hz] = anchor.split("-");
  const x = hz === "left" ? margin + w / 2 : hz === "right" ? pageW - margin - w / 2 : pageW / 2;
  const y = v === "top" ? margin + h / 2 : v === "bottom" ? pageH - margin - h / 2 : pageH / 2;
  return [x, y];
}

export function pickFiles(accept, multiple) {
  return new Promise((resolve) => {
    const input = h("input", { type: "file", accept, multiple });
    input.addEventListener("change", () => resolve([...input.files]));
    input.click();
  });
}

const THUMB_W = 170;
const THUMB_H = 200;

/** Draws `picture` turned clockwise by `rotation` into a new canvas. */
function drawRotated(picture, rotation) {
  const quarter = ((rotation % 360) + 360) % 180 === 90;
  const canvas = h("canvas", { width: quarter ? picture.height : picture.width, height: quarter ? picture.width : picture.height });
  const ctx = canvas.getContext("2d");
  ctx.translate(canvas.width / 2, canvas.height / 2);
  ctx.rotate((rotation * Math.PI) / 180);
  ctx.drawImage(picture.bitmap, -picture.width / 2, -picture.height / 2);
  return canvas;
}

/** Grids are redrawn on every change; each card's page canvas is reused. */
function cachedCanvas(cache, picture, rotation) {
  if (cache.canvas && cache.picture === picture && cache.rotation === rotation) return cache.canvas;
  Object.assign(cache, { canvas: drawRotated(picture, rotation), picture, rotation });
  return cache.canvas;
}

/**
 * One page in a grid.
 * @param {object} o
 * @param {Promise} o.picture the rendered page (from Pdf.thumbs)
 * @param {number} [o.rotation] extra clockwise rotation
 * @param {(frame: {width: number, height: number, points: number[]}) => Node[]} [o.overlay]
 *   what to draw over the page; `points` is the page size as displayed
 */
export function pageCard({ picture, rotation = 0, cache = {}, label, controls = [], overlay, removed, gray, part, onSheetClick, onZoom }) {
  const thumb = h("div", { className: "thumb" }, h("div", { className: "placeholder" }));
  const card = h(
    "div",
    { className: `card${removed ? " removed" : ""}${gray ? " gray" : ""}` },
    thumb,
    h("div", { className: "label" }, label),
    controls.length ? h("div", { className: "controls" }, controls) : null,
  );
  picture
    ?.then((pic) => {
      const quarter = ((rotation % 360) + 360) % 180 === 90;
      const [pw, ph] = quarter ? [pic.points[1], pic.points[0]] : pic.points;
      const scale = Math.min(THUMB_W / pw, THUMB_H / ph);
      const [width, height] = [Math.round(pw * scale), Math.round(ph * scale)];
      const layer = h("div", { className: "overlay" }, overlay ? overlay({ width, height, points: [pw, ph] }) : []);
      const sheet = h(
        "div",
        {
          className: "sheet",
          style: { width: `${width}px`, height: `${height}px`, ...(part ? { "--part": part } : {}) },
          onClick: (event) => {
            if (onSheetClick) {
              const box = sheet.getBoundingClientRect();
              onSheetClick((event.clientX - box.left) / box.width, (event.clientY - box.top) / box.height, event);
            } else onZoom?.();
          },
        },
        cachedCanvas(cache, pic, rotation),
        layer,
      );
      thumb.replaceChildren(sheet);
    })
    .catch((error) => thumb.replaceChildren(h("div", { className: "problem" }, error.message)));
  return card;
}

/** Shows one page large, rendered at the screen's resolution. */
export async function zoom(pdf, page, rotation = 0) {
  const dialog = document.getElementById("zoom");
  const old = dialog.querySelector("canvas");
  const placeholder = h("canvas", { width: 10, height: 14 });
  old.replaceWith(placeholder);
  dialog.showModal();
  const width = Math.round(Math.min(window.innerWidth * 0.9, 1100) * Math.min(2, window.devicePixelRatio || 1));
  try {
    const picture = (await pdf.render([page], width)).get(page);
    placeholder.replaceWith(drawRotated(picture, rotation));
  } catch (error) {
    dialog.close();
    alert(error.message);
  }
}

/** Text drawn with any font the browser has, as a PNG and its size. */
export async function textPicture(text, { font = "sans-serif", bold = false, color = "#000000", size = 160 } = {}) {
  const canvas = h("canvas");
  const ctx = canvas.getContext("2d");
  const fontSpec = `${bold ? "bold " : ""}${size}px ${/[\s,"']/.test(font) && !font.includes(",") ? `"${font}"` : font}`;
  ctx.font = fontSpec;
  const lines = text.split("\n");
  const metrics = lines.map((line) => ctx.measureText(line));
  const pad = Math.ceil(size * 0.15);
  const lineHeight = Math.ceil(size * 1.2);
  canvas.width = Math.ceil(Math.max(1, ...metrics.map((m) => m.width))) + pad * 2;
  canvas.height = lineHeight * lines.length + pad * 2;
  ctx.font = fontSpec;
  ctx.fillStyle = color;
  ctx.textBaseline = "middle";
  lines.forEach((line, i) => ctx.fillText(line, pad, pad + lineHeight * (i + 0.5)));
  return canvasPicture(trim(canvas, pad / 2));
}

/** Crops a canvas to what is drawn on it, keeping `pad` pixels around it. */
export function trim(canvas, pad = 4) {
  const { width, height } = canvas;
  const data = canvas.getContext("2d").getImageData(0, 0, width, height).data;
  let [left, top, right, bottom] = [width, height, -1, -1];
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      if (data[(y * width + x) * 4 + 3] > 8) {
        left = Math.min(left, x);
        right = Math.max(right, x);
        top = Math.min(top, y);
        bottom = Math.max(bottom, y);
      }
    }
  }
  if (right < 0) return null;
  [left, top] = [Math.max(0, left - pad), Math.max(0, top - pad)];
  [right, bottom] = [Math.min(width - 1, right + pad), Math.min(height - 1, bottom + pad)];
  const out = h("canvas", { width: right - left + 1, height: bottom - top + 1 });
  out.getContext("2d").drawImage(canvas, left, top, out.width, out.height, 0, 0, out.width, out.height);
  return out;
}

/** A canvas as {png, url, width, height}. */
export async function canvasPicture(canvas) {
  if (!canvas) return null;
  const blob = await new Promise((resolve) => canvas.toBlob(resolve, "image/png"));
  return { png: new Uint8Array(await blob.arrayBuffer()), url: URL.createObjectURL(blob), width: canvas.width, height: canvas.height };
}

/** An uploaded PNG or JPEG as {png, url, width, height} (bytes kept as they are). */
export async function filePicture(file) {
  const url = URL.createObjectURL(file);
  const img = new Image();
  img.src = url;
  await img.decode();
  return { png: new Uint8Array(await file.arrayBuffer()), url, width: img.naturalWidth, height: img.naturalHeight };
}

export function download(name, data, type = "application/pdf") {
  const url = URL.createObjectURL(new Blob([data], { type }));
  return h("a", { href: url, download: name }, `Download ${name}`);
}
