// A minimal WASI (preview 1) host with an in-memory file system: exactly the
// calls Rust's standard library makes for pdftools-cli. One `MemoryFs` lives
// as long as the page; each command runs in a fresh instance against it.

const E = { SUCCESS: 0, BADF: 8, EXIST: 20, INVAL: 28, ISDIR: 31, NOSYS: 52, NOENT: 44, NOTDIR: 54 };
const FILETYPE = { CHAR: 2, DIR: 3, FILE: 4 };
const OFLAGS = { CREAT: 1, DIRECTORY: 2, EXCL: 4, TRUNC: 8 };
const FDFLAGS_APPEND = 1;
const PREOPEN_FD = 3;

/** Normalises `path` (relative to `base`) to an absolute path without `.`/`..`. */
export function resolve(base, path) {
  const parts = [];
  for (const part of (path.startsWith("/") ? path : `${base}/${path}`).split("/")) {
    if (part === "" || part === ".") continue;
    if (part === "..") parts.pop();
    else parts.push(part);
  }
  return "/" + parts.join("/");
}

const parent = (path) => (path === "/" ? "/" : path.slice(0, path.lastIndexOf("/")) || "/");

export class MemoryFs {
  constructor() {
    /** @type {Map<string, {data: Uint8Array, size: number}>} */
    this.files = new Map();
    this.dirs = new Set(["/"]);
  }

  mkdirs(path) {
    for (let dir = path; !this.dirs.has(dir); dir = parent(dir)) this.dirs.add(dir);
  }

  write(path, bytes) {
    path = resolve("/", path);
    this.mkdirs(parent(path));
    this.files.set(path, { data: bytes, size: bytes.length });
  }

  read(path) {
    const file = this.files.get(resolve("/", path));
    return file && file.data.subarray(0, file.size);
  }

  /** Files under `dir`, as [path, bytes] pairs. */
  list(dir) {
    const prefix = resolve("/", dir).replace(/\/?$/, "/");
    return [...this.files.keys()].filter((p) => p.startsWith(prefix)).sort().map((p) => [p, this.read(p)]);
  }

  /** Removes `path` and, for a folder, everything in it. */
  remove(path) {
    path = resolve("/", path);
    const prefix = path.replace(/\/?$/, "/");
    for (const p of [...this.files.keys()]) if (p === path || p.startsWith(prefix)) this.files.delete(p);
    for (const d of [...this.dirs]) if (d !== "/" && (d === path || d.startsWith(prefix))) this.dirs.delete(d);
  }
}

class Exit extends Error {
  constructor(code) {
    super(`exit ${code}`);
    this.code = code;
  }
}

/**
 * Runs a WASI command module to completion.
 * @returns {{code: number, stdout: string, stderr: string}}
 */
export function runCommand(module, args, fs) {
  const fds = new Map([
    [0, { kind: "stdio" }],
    [1, { kind: "stdio", out: [] }],
    [2, { kind: "stdio", out: [] }],
    [PREOPEN_FD, { kind: "dir", path: "/" }],
  ]);
  let nextFd = PREOPEN_FD + 1;
  const view = () => new DataView(memory.buffer);
  const bytes = () => new Uint8Array(memory.buffer);
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();
  const str = (ptr, len) => decoder.decode(bytes().slice(ptr, ptr + len));
  const pathAt = (fd, ptr, len) => {
    const dir = fds.get(fd);
    return dir && dir.kind === "dir" ? resolve(dir.path, str(ptr, len)) : null;
  };

  const writeStrings = (list, ptrsPtr, bufPtr) => {
    for (const s of list) {
      const data = encoder.encode(s + "\0");
      view().setUint32(ptrsPtr, bufPtr, true);
      bytes().set(data, bufPtr);
      ptrsPtr += 4;
      bufPtr += data.length;
    }
    return E.SUCCESS;
  };
  const writeSizes = (list, countPtr, sizePtr) => {
    view().setUint32(countPtr, list.length, true);
    view().setUint32(sizePtr, list.reduce((n, s) => n + encoder.encode(s).length + 1, 0), true);
    return E.SUCCESS;
  };

  const writeFilestat = (ptr, type, size) => {
    const v = view();
    for (let i = 0; i < 64; i += 8) v.setBigUint64(ptr + i, 0n, true);
    v.setUint8(ptr + 16, type);
    v.setBigUint64(ptr + 24, 1n, true);
    v.setBigUint64(ptr + 32, BigInt(size), true);
    return E.SUCCESS;
  };
  const stat = (path, ptr) => {
    if (fs.dirs.has(path)) return writeFilestat(ptr, FILETYPE.DIR, 0);
    const file = fs.files.get(path);
    return file ? writeFilestat(ptr, FILETYPE.FILE, file.size) : E.NOENT;
  };

  const iovecs = (ptr, count) =>
    Array.from({ length: count }, (_, i) => [view().getUint32(ptr + i * 8, true), view().getUint32(ptr + i * 8 + 4, true)]);

  const wasi = {
    args_get: (argv, buf) => writeStrings(args, argv, buf),
    args_sizes_get: (count, size) => writeSizes(args, count, size),
    environ_get: (env, buf) => writeStrings([], env, buf),
    environ_sizes_get: (count, size) => writeSizes([], count, size),
    clock_time_get(id, _precision, out) {
      const ms = id === 0 ? Date.now() : performance.now();
      view().setBigUint64(out, BigInt(Math.round(ms * 1e6)), true);
      return E.SUCCESS;
    },
    random_get(ptr, len) {
      for (let i = 0; i < len; i += 65536) crypto.getRandomValues(bytes().subarray(ptr + i, ptr + Math.min(len, i + 65536)));
      return E.SUCCESS;
    },
    sched_yield: () => E.SUCCESS,
    proc_exit(code) {
      throw new Exit(code);
    },

    fd_prestat_get(fd, out) {
      if (fd !== PREOPEN_FD) return E.BADF;
      view().setUint8(out, 0);
      view().setUint32(out + 4, 1, true);
      return E.SUCCESS;
    },
    fd_prestat_dir_name(fd, ptr, len) {
      if (fd !== PREOPEN_FD) return E.BADF;
      bytes().set(encoder.encode("/").subarray(0, len), ptr);
      return E.SUCCESS;
    },
    fd_fdstat_get(fd, out) {
      const entry = fds.get(fd);
      if (!entry) return E.BADF;
      const type = entry.kind === "dir" ? FILETYPE.DIR : entry.kind === "file" ? FILETYPE.FILE : FILETYPE.CHAR;
      view().setUint8(out, type);
      view().setUint16(out + 2, entry.append ? FDFLAGS_APPEND : 0, true);
      view().setBigUint64(out + 8, 0xffffffffffffffffn, true);
      view().setBigUint64(out + 16, 0xffffffffffffffffn, true);
      return E.SUCCESS;
    },
    fd_filestat_get(fd, out) {
      const entry = fds.get(fd);
      if (!entry) return E.BADF;
      if (entry.kind === "stdio") return writeFilestat(out, FILETYPE.CHAR, 0);
      return stat(entry.path, out);
    },
    fd_close(fd) {
      return fds.delete(fd) ? E.SUCCESS : E.BADF;
    },
    fd_read(fd, iovs, count, nread) {
      const entry = fds.get(fd);
      if (!entry) return E.BADF;
      let total = 0;
      if (entry.kind === "file") {
        const file = fs.files.get(entry.path);
        if (!file) return E.BADF;
        for (const [ptr, len] of iovecs(iovs, count)) {
          const chunk = file.data.subarray(entry.pos, Math.min(file.size, entry.pos + len));
          bytes().set(chunk, ptr);
          entry.pos += chunk.length;
          total += chunk.length;
          if (chunk.length < len) break;
        }
      } else if (entry.kind === "dir") {
        return E.ISDIR;
      }
      view().setUint32(nread, total, true);
      return E.SUCCESS;
    },
    fd_write(fd, iovs, count, nwritten) {
      const entry = fds.get(fd);
      if (!entry) return E.BADF;
      let total = 0;
      for (const [ptr, len] of iovecs(iovs, count)) {
        const chunk = bytes().slice(ptr, ptr + len);
        if (entry.kind === "stdio") {
          entry.out?.push(chunk);
        } else if (entry.kind === "file") {
          const file = fs.files.get(entry.path);
          if (!file) return E.BADF;
          if (entry.append) entry.pos = file.size;
          const end = entry.pos + len;
          if (end > file.data.length) {
            const grown = new Uint8Array(Math.max(end, file.data.length * 2, 4096));
            grown.set(file.data.subarray(0, file.size));
            file.data = grown;
          }
          file.data.set(chunk, entry.pos);
          entry.pos = end;
          file.size = Math.max(file.size, end);
        } else {
          return E.ISDIR;
        }
        total += len;
      }
      view().setUint32(nwritten, total, true);
      return E.SUCCESS;
    },
    fd_seek(fd, offset, whence, out) {
      const entry = fds.get(fd);
      if (!entry || entry.kind !== "file") return E.BADF;
      const size = fs.files.get(entry.path)?.size ?? 0;
      const base = whence === 0 ? 0 : whence === 1 ? entry.pos : size;
      const pos = base + Number(offset);
      if (pos < 0) return E.INVAL;
      entry.pos = pos;
      view().setBigUint64(out, BigInt(pos), true);
      return E.SUCCESS;
    },

    path_open(dirFd, _dirflags, ptr, len, oflags, _rightsBase, _rightsInheriting, fdflags, out) {
      const path = pathAt(dirFd, ptr, len);
      if (!path) return E.BADF;
      let entry;
      if (fs.dirs.has(path)) {
        if (oflags & (OFLAGS.CREAT | OFLAGS.TRUNC) && !(oflags & OFLAGS.DIRECTORY)) return oflags & OFLAGS.EXCL ? E.EXIST : E.ISDIR;
        entry = { kind: "dir", path };
      } else {
        if (oflags & OFLAGS.DIRECTORY) return fs.files.has(path) ? E.NOTDIR : E.NOENT;
        const exists = fs.files.has(path);
        if (exists && oflags & OFLAGS.EXCL) return E.EXIST;
        if (!exists && !(oflags & OFLAGS.CREAT)) return E.NOENT;
        if (!fs.dirs.has(parent(path))) return E.NOENT;
        if (!exists || oflags & OFLAGS.TRUNC) fs.files.set(path, { data: new Uint8Array(0), size: 0 });
        entry = { kind: "file", path, pos: 0, append: Boolean(fdflags & FDFLAGS_APPEND) };
      }
      const fd = nextFd++;
      fds.set(fd, entry);
      view().setUint32(out, fd, true);
      return E.SUCCESS;
    },
    path_filestat_get(fd, _flags, ptr, len, out) {
      const path = pathAt(fd, ptr, len);
      return path ? stat(path, out) : E.BADF;
    },
    path_create_directory(fd, ptr, len) {
      const path = pathAt(fd, ptr, len);
      if (!path) return E.BADF;
      if (fs.dirs.has(path) || fs.files.has(path)) return E.EXIST;
      if (!fs.dirs.has(parent(path))) return E.NOENT;
      fs.dirs.add(path);
      return E.SUCCESS;
    },
    path_rename(fd, ptr, len, newFd, newPtr, newLen) {
      const from = pathAt(fd, ptr, len);
      const to = pathAt(newFd, newPtr, newLen);
      if (!from || !to) return E.BADF;
      const file = fs.files.get(from);
      if (!file) return fs.dirs.has(from) ? E.NOSYS : E.NOENT;
      if (fs.dirs.has(to)) return E.ISDIR;
      if (!fs.dirs.has(parent(to))) return E.NOENT;
      fs.files.delete(from);
      fs.files.set(to, file);
      return E.SUCCESS;
    },
    path_unlink_file(fd, ptr, len) {
      const path = pathAt(fd, ptr, len);
      if (!path) return E.BADF;
      if (fs.dirs.has(path)) return E.ISDIR;
      return fs.files.delete(path) ? E.SUCCESS : E.NOENT;
    },
  };

  // Anything else the module might import fails cleanly instead of linking errors.
  const imports = new Proxy(wasi, { get: (target, name) => target[name] ?? (() => E.NOSYS) });
  const instance = new WebAssembly.Instance(module, { wasi_snapshot_preview1: imports });
  const memory = instance.exports.memory;

  let code = 0;
  try {
    instance.exports._start();
  } catch (error) {
    if (error instanceof Exit) code = error.code;
    else throw error;
  }
  const text = (fd) => {
    const chunks = fds.get(fd)?.out ?? [];
    return decoder.decode(chunks.length === 1 ? chunks[0] : new Uint8Array(chunks.flatMap((c) => [...c])));
  };
  // fds 1 and 2 are never closed by the module, so their output is still here.
  return { code, stdout: text(1), stderr: text(2) };
}
