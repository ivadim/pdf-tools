// Runs pdftools-cli.wasm off the main thread. Files live in one in-memory
// file system for the life of the page, so inputs are copied in only once.

import { MemoryFs, runCommand } from "./wasi.js";

const fs = new MemoryFs();
const module = fetch(new URL("pdftools-cli.wasm", import.meta.url))
  .then((response) => {
    if (!response.ok) throw new Error(`couldn’t load pdftools-cli.wasm (${response.status})`);
    return response.arrayBuffer();
  })
  .then((bytes) => WebAssembly.compile(bytes));

self.onmessage = async ({ data: { id, op, ...request } }) => {
  try {
    self.postMessage({ id, ok: true, ...(await handle(op, request)) }, { transfer: transfers });
  } catch (error) {
    self.postMessage({ id, ok: false, error: String(error?.message ?? error) });
  } finally {
    transfers = [];
  }
};

let transfers = [];

async function handle(op, request) {
  switch (op) {
    case "ready":
      await module;
      return {};
    case "put":
      fs.write(request.path, new Uint8Array(request.data));
      return {};
    case "remove":
      fs.remove(request.path);
      return {};
    case "run": {
      // Outputs go to a fresh folder; its files are sent back and dropped.
      const { args, outputs } = request;
      if (outputs) {
        fs.remove(outputs);
        fs.mkdirs(outputs);
      }
      const result = runCommand(await module, ["pdftools-cli", ...args], fs);
      if (result.code !== 0) throw new Error(result.stderr.trim() || `exit status ${result.code}`);
      const files = outputs ? fs.list(outputs).map(([path, bytes]) => ({ path, data: bytes.slice().buffer })) : [];
      if (outputs) fs.remove(outputs);
      transfers = files.map((f) => f.data);
      return { json: result.stdout.trim() ? JSON.parse(result.stdout) : null, files };
    }
    default:
      throw new Error(`unknown operation ${op}`);
  }
}
