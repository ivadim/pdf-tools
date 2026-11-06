import { chromium } from "@playwright/test";
import { mkdir } from "node:fs/promises";
import { resolve } from "node:path";
import { env } from "node:process";
import { fileURLToPath } from "node:url";

const root = resolve(fileURLToPath(new URL("../docs/design-reviews/screenshots/web/", import.meta.url)));
const baseURL = env.PDFTOOLS_WEB_URL ?? "http://127.0.0.1:4173";
const selectedTool = env.PDFTOOLS_SCREENSHOT_TOOL;
const fixture = (path) => fileURLToPath(new URL(`../${path}`, import.meta.url));
const tools = [
  ["merge", "Merge"], ["split", "Split"], ["compress", "Compress"], ["protect", "Protect"],
  ["unlock", "Unlock"], ["rotate", "Rotate"], ["rearrange", "Rearrange"], ["numbers", "Page numbers"],
  ["watermark", "Watermark"], ["metadata", "Metadata"], ["signature", "Signature"],
  ["redact", "Redact"], ["grayscale", "Grayscale"], ["images", "Images to PDF"],
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
    if (!selectedTool) await page.screenshot({ path: resolve(dir, "home.png") });

    await page.goto(new URL("/#about", baseURL).href);
    await page.getByRole("heading", { name: "PDF Tools", exact: true }).waitFor();
    if (!selectedTool) await page.screenshot({ path: resolve(dir, "about.png") });

    for (const [id, title] of tools.filter(([toolId]) => !selectedTool || selectedTool === toolId)) {
      // A hash-only navigation keeps the previously opened PDF in memory.
      // Reload so each empty capture really starts with no input.
      await page.goto(baseURL);
      await page.reload();
      await page.goto(new URL(`/#${id}`, baseURL).href);
      await page.getByRole("heading", { name: title }).waitFor();
      await page.locator(".drop").waitFor();
      await page.screenshot({ path: resolve(dir, "tools", `${id}-empty.png`) });

      const files = id === "merge"
        ? [fixture("tests/fixtures/pdfjs/tracemonkey.pdf"), fixture("tests/fixtures/pdfjs/rotation.pdf")]
        : id === "images"
          ? [fixture("web/icon-192.png"), fixture("web/icon-512.png")]
          : id === "unlock"
            ? [fixture("tests/fixtures/encrypted/aes256-password-test.pdf")]
            : [fixture("tests/fixtures/pdfjs/tracemonkey.pdf")];
      const chooser = page.waitForEvent("filechooser");
      await page.locator(".drop").click();
      await (await chooser).setFiles(files);
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
      if (id === "redact") {
        await page.locator(".pages .sheet").first().click();
        const dialog = page.getByRole("dialog", { name: "Mark areas on page 1" });
        await dialog.locator("canvas").waitFor();
        const box = await dialog.locator("canvas").boundingBox();
        await page.mouse.move(box.x + box.width * 0.2, box.y + box.height * 0.2);
        await page.mouse.down();
        await page.mouse.move(box.x + box.width * 0.7, box.y + box.height * 0.3);
        await page.mouse.up();
        await dialog.getByRole("button", { name: "Done" }).click();
      }
      await page.screenshot({ path: resolve(dir, "tools", `${id}-configured.png`) });
    }
    await context.close();
  }
} finally {
  await browser.close();
}
