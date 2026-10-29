// Takes the README screenshots from the browser demo (a synthetic library held in memory; no music files).
// Run `npm run build` and `npm run preview` first, then `node scripts/screenshots/take.mjs`.
import { chromium } from "@playwright/test";
import { mkdir } from "node:fs/promises";

const base = process.env.PREVIEW_URL ?? "http://127.0.0.1:1421";
const out = new URL("../../assets/", import.meta.url);
await mkdir(out, { recursive: true });

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 2, colorScheme: "dark" });
await page.goto(base);
const rows = page.getByTestId("track-row");
await rows.nth(3).waitFor();

// A track selected and playing, so the player bar and waveform show.
await rows.nth(3).dblclick();
await page.waitForTimeout(1500);
await page.screenshot({ path: new URL("library.png", out).pathname, type: "png" });

await browser.close();
console.log("wrote assets/library.png");
