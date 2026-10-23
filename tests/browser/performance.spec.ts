import { expect, test } from "@playwright/test";
import { writeFile } from "node:fs/promises";

test("records synthetic browser latency, scrolling and design evidence", async ({ page }, testInfo) => {
  await page.goto("/");
  await expect(page.getByRole("status")).toContainText("100,000 tracks");
  const coldReady = await page.evaluate(() => performance.now());
  await page.screenshot({ path: testInfo.outputPath("collection.png") });
  const measurements = await page.evaluate(async () => {
    const input = document.querySelector<HTMLInputElement>('input[aria-label="Search library"]');
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
    if (!input || !setter) throw new Error("Search input is unavailable.");
    const terms = ["after", "morning", "studio", "night", "zz-no-result", "099999"];
    const samples: number[] = [];
    for (let index = 0; index < 30; index += 1) {
      const term = terms[index % terms.length] ?? "";
      const started = performance.now();
      setter.call(input, term);
      input.dispatchEvent(new Event("input", { bubbles: true }));
      await new Promise<void>((resolve, reject) => {
        const deadline = performance.now() + 5_000;
        let confirmedFrame = false;
        const frame = () => {
          const results = document.querySelector('section[aria-label="tracks results"]');
          const status = document.querySelector('[role="status"]')?.textContent;
          if (results?.getAttribute("aria-busy") === "false" && status?.includes(`matching "${term}"`)) {
            if (confirmedFrame) { resolve(); return; }
            confirmedFrame = true;
          } else confirmedFrame = false;
          if (performance.now() > deadline) { reject(new Error(`Timed out measuring ${term}`)); return; }
          requestAnimationFrame(frame);
        };
        requestAnimationFrame(frame);
      });
      samples.push(performance.now() - started);
    }
    return samples;
  });
  await page.getByRole("button", { name: "Clear search" }).click();
  await expect(page.getByRole("status")).toContainText("100,000 tracks");
  const scrollMeasurements = await page.getByTestId("library-scroll").evaluate(async (element) => {
    const frames: number[] = [];
    let maxRows = 0;
    let previous = performance.now();
    for (let index = 0; index < 120; index += 1) {
      await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
      const now = performance.now();
      frames.push(now - previous);
      previous = now;
      element.scrollTop = index * 560;
      maxRows = Math.max(maxRows, element.querySelectorAll('[data-testid="track-row"]').length);
    }
    return { frames, maxRows };
  });
  const sorted = [...measurements].sort((a, b) => a - b);
  const frames = [...scrollMeasurements.frames].sort((a, b) => a - b);
  const report = {
    dataset: "100,000 deterministic synthetic tracks; 10,000 releases; 240 artists",
    browser: "Playwright Chromium headless; 1440x960; one worker",
    coldReadyObservedMs: coldReady,
    searchSamples: measurements,
    searchP95Ms: sorted[Math.ceil(sorted.length * .95) - 1],
    scrollFrames: scrollMeasurements.frames.length,
    scrollFrameP95Ms: frames[Math.ceil(frames.length * .95) - 1],
    scrollMaxFrameMs: frames.at(-1),
    scrollFramesOver33Ms: frames.filter((value) => value > 33).length,
    maximumRenderedTrackRows: scrollMeasurements.maxRows,
    limits: "Search: input event through correct result state plus two animation frames. Cold readiness includes Playwright polling. Warm search uses cached catalog and sort. No database, Tauri IPC, audio, Windows or import load.",
  };
  await writeFile(testInfo.outputPath("browser-measurements.json"), JSON.stringify(report, null, 2));
  await testInfo.attach("browser-measurements", { body: JSON.stringify(report), contentType: "application/json" });
  expect(scrollMeasurements.maxRows).toBeLessThan(45);
  await page.getByRole("button", { name: "Releases", exact: true }).click();
  await expect(page.getByRole("status")).toContainText("10,000 releases");
  await page.screenshot({ path: testInfo.outputPath("releases.png") });
});
