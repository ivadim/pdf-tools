// The page's side of the PDF engine: commands sent to the worker, open PDFs
// with their page thumbnails, and the app's page-range syntax.

const worker = new Worker(new URL("./worker.js", import.meta.url), { type: "module" });
const pending = new Map();
let nextId = 0;

worker.onmessage = ({ data }) => {
  const request = pending.get(data.id);
  pending.delete(data.id);
  if (data.ok) request.resolve(data);
  else request.reject(new Error(data.error));
};
worker.onerror = (event) => {
  for (const request of pending.values()) request.reject(new Error(event.message || "the PDF engine stopped"));
  pending.clear();
};

function call(op, payload = {}, transfer = []) {
  const id = ++nextId;
  return new Promise((resolve, reject) => {
    pending.set(id, { resolve, reject });
    worker.postMessage({ id, op, ...payload }, transfer);
  });
}

export const ready = call("ready");

/** Copies a file into the engine's file system. */
export async function put(path, data) {
  const buffer = data instanceof ArrayBuffer ? data : data.slice().buffer;
  await call("put", { path, data: buffer }, [buffer]);
}

export const remove = (path) => call("remove", { path });

let nextOutput = 0;

/**
 * Runs a pdftools-cli command. Paths under the `{out}` folder it is given
 * are returned as files.
 * @returns {Promise<{json: any, files: {path: string, name: string, data: ArrayBuffer}[]}>}
 */
export async function run(args, { outputs = false } = {}) {
  const folder = `/out/${++nextOutput}`;
  const filled = args.map((a) => a.replaceAll("{out}", folder));
  const { json, files } = await call("run", { args: filled, outputs: outputs ? folder : null });
  return { json, files: files.map((f) => ({ ...f, name: f.path.slice(f.path.lastIndexOf("/") + 1) })) };
}

let nextFile = 0;
const THUMB_BATCH = 6;

export class Pdf {
  /** Reads `file` into the engine and inspects it. */
  static async open(file) {
    const pdf = new Pdf(file.name, `/in/${++nextFile}.pdf`, file.size);
    await put(pdf.path, await file.arrayBuffer());
    pdf.info = (await run(["info", pdf.path])).json;
    pdf.queueThumbnails();
    return pdf;
  }

  constructor(name, path, size) {
    this.name = name;
    this.path = path;
    this.size = size;
    this.info = null;
    /** page → Promise<{bitmap, width, height, points}> */
    this.thumbs = new Map();
    // Cards grow to about 260 px on wide screens; render sharp enough for that.
    this.thumbWidth = Math.round(240 * Math.min(2, window.devicePixelRatio || 1));
    this.closed = false;
  }

  get pageCount() {
    return this.info?.pages ?? 0;
  }

  get locked() {
    return Boolean(this.info?.needs_password);
  }

  get baseName() {
    return this.name.replace(/\.pdf$/i, "");
  }

  /** Renders every page's thumbnail in the background, a few at a time. */
  queueThumbnails() {
    if (this.locked) return;
    const resolvers = new Map();
    for (let n = 1; n <= this.pageCount; n++) {
      this.thumbs.set(n, new Promise((resolve, reject) => resolvers.set(n, { resolve, reject })));
      this.thumbs.get(n).catch(() => {});
    }
    (async () => {
      for (let first = 1; first <= this.pageCount && !this.closed; first += THUMB_BATCH) {
        const pages = range(first, Math.min(this.pageCount, first + THUMB_BATCH - 1));
        try {
          for (const [n, picture] of await this.render(pages, this.thumbWidth)) resolvers.get(n).resolve(picture);
        } catch (error) {
          for (const n of pages) resolvers.get(n).reject(error);
        }
      }
    })();
  }

  /** Renders pages at `width` pixels; returns page → picture. */
  async render(pages, width) {
    const { json, files } = await run(["render", this.path, "{out}", `--width=${width}`, `--pages=${formatList(pages)}`], {
      outputs: true,
    });
    const pixels = new Map(files.map((f) => [parseInt(f.name, 10), f.data]));
    const out = new Map();
    for (const page of json) {
      const image = new ImageData(new Uint8ClampedArray(pixels.get(page.page)), page.width, page.height);
      out.set(page.page, { bitmap: await createImageBitmap(image), width: page.width, height: page.height, points: page.points });
    }
    return out;
  }

  close() {
    this.closed = true;
    remove(this.path);
  }
}

export const range = (from, to) => Array.from({ length: Math.max(0, to - from + 1) }, (_, i) => from + i);

/** "1-3, 5, 8-" → [[1,2,3],[5],[8,…,total]], like pdftools-render's ranges. */
export function parseGroups(text, total) {
  const items = text
    .split(/[,;]/)
    .map((s) => s.trim())
    .filter(Boolean);
  if (!items.length) throw new Error("Enter page ranges, e.g. 1-3, 4-6");
  return items.map((item) => {
    const [from, to] = item.split(/[-–]/, 2).map((s) => s.trim());
    const dash = /[-–]/.test(item);
    const number = (s, fallback) => {
      if (s === "" || s === undefined) return fallback;
      if (!/^\d+$/.test(s)) throw new Error(`“${item}” isn’t a page or range`);
      return parseInt(s, 10);
    };
    if (from === "" && (!dash || to === "")) throw new Error(`“${item}” isn’t a page or range`);
    const start = number(from, 1);
    const end = dash ? number(to, total) : start;
    if (start < 1 || end < 1 || start > total || end > total) throw new Error(`“${item}” is outside 1–${total}`);
    if (start > end) throw new Error(`“${item}” runs backwards`);
    return range(start, end);
  });
}

export function parseList(text, total) {
  return [...new Set(parseGroups(text, total).flat())].sort((a, b) => a - b);
}

/** [1,2,3,5] → "1-3, 5". */
export function formatList(pages) {
  const sorted = [...new Set(pages)].sort((a, b) => a - b);
  const parts = [];
  for (let i = 0; i < sorted.length; i++) {
    const start = sorted[i];
    while (i + 1 < sorted.length && sorted[i + 1] === sorted[i] + 1) i++;
    parts.push(start === sorted[i] ? `${start}` : `${start}-${sorted[i]}`);
  }
  return parts.join(", ");
}

export function formatBytes(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
