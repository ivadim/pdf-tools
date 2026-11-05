import { chromium } from "@playwright/test";
import { mkdir, readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(fileURLToPath(new URL("../docs/design-reviews/screenshots/web/", import.meta.url)));
const baseURL = process.env.PDFTOOLS_WEB_URL ?? "http://127.0.0.1:4173";
const fixture = (path) => fileURLToPath(new URL(`../${path}`, import.meta.url));
const asDropFile = async (path) => ({
  name: path.split("/").at(-1),
  type: path.endsWith(".pdf") ? "application/pdf" : "image/png",
  base64: (await readFile(path)).toString("base64"),
});
const tools = [
  ["merge", "Merge"], ["split", "Split"], ["compress", "Compress"], ["protect", "Protect"],
  ["unlock", "Unlock"], ["rotate", "Rotate"], ["rearrange", "Rearrange"], ["numbers", "Page numbers"],
  ["watermark", "Watermark"], ["metadata", "Metadata"], ["signature", "Signature"],
  ["grayscale", "Grayscale"], ["images", "Images to PDF"],
];

const browser = await chromium.launch({ headless: true });
try {
  for (const theme of ["light", "dark"]) {
    const context = await browser.newContext({ viewport: { width: 1440, height: 1000 }, colorScheme: theme });
    const page = await context.newPage();
    const dir = resolve(root, theme);
    await mkdir(resolve(dir, "tools"), { recursive: true });
    await page.goto(baseURL);
    await page.getByRole("heading", { name: "PDF Tools" }).waitFor();
    await page.screenshot({ path: resolve(dir, "home.png") });

    for (const [id, title] of tools) {
      await page.goto(new URL(`/#${id}`, baseURL).href);
      await page.getByRole("heading", { name: title }).waitFor();
      await page.screenshot({ path: resolve(dir, "tools", `${id}-empty.png`) });

      const files = id === "merge"
        ? [fixture("tests/fixtures/pdfjs/tracemonkey.pdf"), fixture("tests/fixtures/pdfjs/rotation.pdf")]
        : id === "images"
          ? [fixture("web/icon-192.png"), fixture("web/icon-512.png")]
          : id === "unlock"
            ? [fixture("tests/fixtures/encrypted/aes256-password-test.pdf")]
            : [fixture("tests/fixtures/pdfjs/tracemonkey.pdf")];
      const dropped = await Promise.all(files.map(asDropFile));
      await page.evaluate((items) => {
        const transfer = new DataTransfer();
        for (const item of items) {
          const bytes = Uint8Array.from(atob(item.base64), (character) => character.charCodeAt(0));
          transfer.items.add(new File([bytes], item.name, { type: item.type }));
        }
        document.dispatchEvent(new DragEvent("drop", { bubbles: true, cancelable: true, dataTransfer: transfer }));
      }, dropped);
      await page.locator(".pages").waitFor();
      await page.screenshot({ path: resolve(dir, "tools", `${id}-configured.png`) });
    }
    await context.close();
  }
} finally {
  await browser.close();
}
