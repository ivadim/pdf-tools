import { expect, test } from "@playwright/test";
import { fileURLToPath } from "node:url";

const fixture = (name) => fileURLToPath(new URL(`../../tests/fixtures/pdfjs/${name}`, import.meta.url));

test("loads the WebAssembly app and follows a tool route", async ({ page }) => {
  await page.goto("/#compress");

  await expect(page.getByRole("heading", { name: "Compress" })).toBeVisible();
  await expect(page.getByText("Choose a PDF file", { exact: true })).toBeVisible();

  await page.getByRole("button", { name: "Home", exact: true }).click();
  await expect(page.getByRole("heading", { name: "PDF Tools" })).toBeVisible();
});

test("merges fixture PDFs and produces a download", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "PDF Tools" })).toBeVisible();

  await page.getByRole("button", { name: "Merge", exact: true }).first().click();
  const chooser = page.waitForEvent("filechooser");
  await page.locator(".drop").click();
  await (await chooser).setFiles([fixture("tracemonkey.pdf"), fixture("rotation.pdf")]);

  await expect(page.getByRole("button", { name: "Run Merge" })).toBeEnabled();
  await page.getByRole("button", { name: "Run Merge" }).click();
  await expect(page.getByText(/Merged 2 files into 16 pages\./)).toBeVisible();

  const download = page.waitForEvent("download");
  await page.getByRole("link", { name: "Download merged.pdf" }).click();
  expect((await download).suggestedFilename()).toBe("merged.pdf");
});
