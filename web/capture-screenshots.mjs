import { chromium } from "@playwright/test";
import { mkdir, readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { env } from "node:process";
import { fileURLToPath } from "node:url";

const root = resolve(fileURLToPath(new URL("../docs/design-reviews/screenshots/web/", import.meta.url)));
const baseURL = env.PDFTOOLS_WEB_URL ?? "http://127.0.0.1:4173";
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
    await page.getByRole("heading", { name: "Choose a tool" }).waitFor();
    await page.screenshot({ path: resolve(dir, "home.png") });

    await page.goto(new URL("/#about", baseURL).href);
    await page.getByRole("heading", { name: "About PDF Tools" }).waitFor();
    await page.screenshot({ path: resolve(dir, "about.png") });

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
      if (id === "split") await page.getByPlaceholder("1-3, 4-6, 7-").fill("1-7, 8-14");
      if (id === "protect") {
        await page.locator('input[type="password"]').first().fill("review-password");
        await page.locator('input[type="password"]').nth(1).fill("review-password");
      }
      if (id === "unlock") {
        await page.locator('.options input[type="password"]').waitFor();
      } else {
        await page.waitForFunction((tool) => {
          const cards = [...document.querySelectorAll(".pages .card")];
          if (!cards.length) return false;
          if (tool === "images") return cards.every((card) => {
            const image = card.querySelector(".thumb img");
            return image?.complete && image.naturalWidth > 0;
          });
          return cards.every((card) => {
            const canvas = card.querySelector(".thumb canvas");
            return canvas && canvas.width > 0 && canvas.height > 0;
          });
        }, id);
        if (id === "watermark") await page.waitForFunction(() => {
          const action = document.querySelector(".actions .primary");
          return Boolean(action && !action.disabled);
        });
        if (id === "watermark") await page.waitForFunction(() => {
          const stamps = [...document.querySelectorAll(".pages .stamp")];
          return stamps.length > 0 && stamps.every((stamp) => stamp.complete && stamp.naturalWidth > 0);
        });
      }
      await page.screenshot({ path: resolve(dir, "tools", `${id}-configured.png`) });
    }
    await context.close();
  }
} finally {
  await browser.close();
}
