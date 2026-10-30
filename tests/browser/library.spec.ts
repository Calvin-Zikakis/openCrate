import { expect, test } from "@playwright/test";

test("renders bounded rows, reaches a deep window and selects tracks", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/");
  await expect(page.getByRole("status")).toContainText("100,000 tracks");
  const rows = page.getByTestId("track-row");
  expect(await rows.count()).toBeLessThan(45);
  await rows.first().click();
  await expect(rows.first()).toHaveAttribute("aria-pressed", "true");
  await page.getByTestId("library-scroll").evaluate((element) => { element.scrollTop = element.scrollHeight; });
  await expect(rows.last()).toContainText("After Hours 000001");
  expect(await rows.count()).toBeLessThan(45);
  expect(errors).toEqual([]);
});

test("search, formats, sorting and rapid changes show only current results", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("status")).toContainText("100,000 tracks");
  const search = page.getByRole("textbox", { name: "Search library" });
  await search.fill("after");
  await search.fill("not-in-the-catalog");
  await search.fill("Morning Light 099999");
  await expect(page.getByRole("status")).toContainText(/\b1 track matching/);
  await expect(page.getByTestId("track-row")).toHaveCount(1);
  await expect(page.getByTestId("track-row")).toContainText("Morning Light 099999");
  await page.getByRole("button", { name: "Clear search" }).click();
  await page.getByRole("button", { name: "MP3", exact: true }).click();
  await expect(page.getByRole("status")).toContainText("33,333 tracks");
  await page.getByLabel("Sort library").selectOption("bpm");
  await expect(page.getByTestId("track-row").first().locator(".track-bpm")).toHaveText("110");
  await search.fill("no such record");
  await expect(page.getByRole("heading", { name: "No matches this time" })).toBeVisible();
});

test("searching artists for a track's title finds its artist, which opens on that track", async ({ page }) => {
  await page.goto("/");
  // The search carries from Tracks to Artists: the box looks in all three.
  await page.getByRole("textbox", { name: "Search library" }).fill("Morning Light 099999");
  await page.getByRole("button", { name: "Artists", exact: true }).click();
  await expect(page.getByRole("textbox", { name: "Search library" })).toHaveValue("Morning Light 099999");
  await page.getByRole("textbox", { name: "Search library" }).fill("");
  await page.getByRole("button", { name: "Artists", exact: true }).click();
  await page.getByRole("textbox", { name: "Search library" }).fill("Morning Light 099999");
  await expect(page.getByRole("status")).toContainText(/\b1 artist matching/);
  await page.getByTestId("library-card").first().click();
  await expect(page.getByRole("status")).toContainText(/\b1 track matching/);
  await expect(page.getByTestId("track-row")).toContainText("Morning Light 099999");
  await page.getByRole("button", { name: "Clear search" }).click();
  await expect(page.getByRole("status")).toContainText(/\b\d{2,} tracks/);
});

test("release and artist cards are virtualized and open the correct tracks", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Releases", exact: true }).click();
  await expect(page.getByRole("status")).toContainText("10,000 releases");
  await expect(page.locator(".page-note")).toHaveText("All your releases live here, one card per album or EP.");
  expect(await page.getByTestId("library-card").count()).toBeLessThan(60);
  await page.getByTestId("library-card").first().click();
  await expect(page.getByRole("status")).toContainText("10 tracks");
  await expect(page.locator(".page-note")).toHaveCount(0);
  await page.getByRole("button", { name: "Back to releases" }).click();
  await expect(page.getByRole("status")).toContainText("10,000 releases");
  await page.getByRole("button", { name: "Artists", exact: true }).click();
  await expect(page.getByRole("status")).toContainText("240 artists");
  await expect(page.locator(".page-note")).toContainText("All your artists live here");
  await page.getByTestId("library-card").first().click();
  await expect(page.getByRole("status")).toContainText("410 tracks");
});

test("keyboard navigation, settings persistence and modal focus work", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("status")).toContainText("100,000 tracks");
  await page.keyboard.press("/");
  await expect(page.getByRole("textbox", { name: "Search library" })).toBeFocused();
  await page.getByTestId("track-row").first().focus();
  await page.keyboard.press("ArrowDown");
  await expect(page.getByTestId("track-row").nth(1)).toBeFocused();
  await page.keyboard.press("End");
  await expect(page.getByTestId("track-row").last()).toContainText("After Hours 000001");
  await expect(page.getByTestId("track-row").last()).toBeFocused();
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.getByRole("checkbox", { name: "Compact track rows" }).check();
  await page.getByRole("checkbox", { name: "Reduce motion" }).check();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).not.toBeVisible();
  await expect(page.getByRole("button", { name: "Settings", exact: true })).toBeFocused();
  await page.reload();
  await expect(page.getByRole("button", { name: "Compact", exact: true })).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("html")).toHaveAttribute("data-reduce-motion", "true");
});

test("the page header folds to one line for more rows, on every view, and stays folded", async ({ page }) => {
  await page.goto("/");
  const hero = page.locator(".library-hero");
  await expect(hero.locator(".page-note")).toBeVisible();
  const before = (await page.locator(".library-results").boundingBox())!.height;
  await page.getByRole("button", { name: "Hide header" }).click();
  await expect(hero.locator(".page-note")).toBeHidden();
  await expect(hero.locator(".hero-art")).toBeHidden();
  await expect(hero.getByRole("heading", { name: "Tracks" })).toBeVisible();
  expect((await page.locator(".library-results").boundingBox())!.height).toBeGreaterThan(before + 100);
  await page.getByRole("button", { name: "Releases", exact: true }).click();
  await expect(hero.locator(".page-note")).toBeHidden();
  await page.reload();
  await expect(page.getByRole("button", { name: "Show header" })).toHaveAttribute("aria-expanded", "false");
  await page.getByRole("button", { name: "Show header" }).click();
  await expect(hero.locator(".page-note")).toBeVisible();
});

test("storage failures are visible and unbuilt features are explicit", async ({ page }) => {
  await page.addInitScript(() => {
    Storage.prototype.setItem = () => { throw new DOMException("Storage blocked", "SecurityError"); };
  });
  await page.goto("/");
  await page.getByRole("button", { name: "Compact", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("could not be saved");
  await page.getByRole("button", { name: "Import", exact: true }).click();
  await expect(page.getByText("Watch folders need a library on disk.")).toBeVisible();
  await page.getByRole("button", { name: "rekordbox", exact: true }).click();
  await expect(page.getByText("Reading a rekordbox export needs a library on disk.")).toBeVisible();
  await expect(page.getByRole("button", { name: "Playback not available in demo" })).toBeDisabled();
});

test("fits the minimum desktop width without horizontal overflow", async ({ page }) => {
  await page.setViewportSize({ width: 720, height: 600 });
  await page.goto("/");
  await expect(page.getByRole("status")).toContainText("100,000 tracks");
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await expect(page.getByTestId("track-row").first()).toBeVisible();
  await page.getByRole("button", { name: "Releases", exact: true }).click();
  await expect(page.getByRole("status")).toContainText("10,000 releases");
});

test("blocked worker startup produces a visible error rather than a blank shell", async ({ page }) => {
  await page.addInitScript(() => {
    Object.defineProperty(window, "Worker", {
      value: class {
        constructor() { throw new Error("Worker construction is blocked"); }
      },
    });
  });
  await page.goto("/");
  await expect(page.getByRole("alert")).toContainText("Could not start the demo library");
  await expect(page.getByRole("alert")).toContainText("Worker construction is blocked");
});

test("body text tokens meet contrast and system reduced motion is respected", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/");
  await expect(page.getByRole("status")).toContainText("100,000 tracks");
  const ratios = await page.evaluate(() => {
    const styles = getComputedStyle(document.documentElement);
    const luminance = (value: string) => {
      // The bundler may shorten a token to three hex digits (#fff); expand it before reading channels.
      const hex = value.trim().replace("#", "");
      const parts = (hex.length === 3 ? hex.replace(/./g, "$&$&") : hex).match(/.{2}/g);
      if (!parts || parts.length !== 3) throw new Error(`Expected RGB token, got ${value}`);
      const channels = parts.map((part) => parseInt(part, 16) / 255)
        .map((channel) => channel <= .04045 ? channel / 12.92 : ((channel + .055) / 1.055) ** 2.4);
      return (channels[0] ?? 0) * .2126 + (channels[1] ?? 0) * .7152 + (channels[2] ?? 0) * .0722;
    };
    const results: number[] = [];
    for (const foreground of ["--text", "--secondary", "--faint"]) {
      for (const background of ["--base", "--panel", "--raised", "--hover"]) {
        const light = luminance(styles.getPropertyValue(foreground));
        const dark = luminance(styles.getPropertyValue(background));
        results.push((light + .05) / (dark + .05));
      }
    }
    return results;
  });
  expect(Math.min(...ratios)).toBeGreaterThanOrEqual(4.5);
  await expect(page.getByRole("button", { name: "Settings", exact: true })).toHaveCSS("transition-duration", "0s");
});

test("light theme text and state colours meet contrast on every surface", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("group", { name: "Theme" }).getByRole("radio", { name: "Light" }).check();
  await page.keyboard.press("Escape");
  const ratios = await page.evaluate(() => {
    const styles = getComputedStyle(document.documentElement);
    const luminance = (value: string) => {
      // The bundler may shorten a token to three hex digits (#fff); expand it before reading channels.
      const hex = value.trim().replace("#", "");
      const parts = (hex.length === 3 ? hex.replace(/./g, "$&$&") : hex).match(/.{2}/g);
      if (!parts || parts.length !== 3) throw new Error(`Expected RGB token, got ${value}`);
      const channels = parts.map((part) => parseInt(part, 16) / 255)
        .map((channel) => channel <= .04045 ? channel / 12.92 : ((channel + .055) / 1.055) ** 2.4);
      return (channels[0] ?? 0) * .2126 + (channels[1] ?? 0) * .7152 + (channels[2] ?? 0) * .0722;
    };
    const results: { pair: string; ratio: number }[] = [];
    for (const foreground of ["--text", "--secondary", "--faint", "--accent-text", "--ok", "--danger"]) {
      for (const background of ["--base", "--panel", "--raised"]) {
        const a = luminance(styles.getPropertyValue(foreground));
        const b = luminance(styles.getPropertyValue(background));
        results.push({ pair: `${foreground} on ${background}`, ratio: (Math.max(a, b) + .05) / (Math.min(a, b) + .05) });
      }
    }
    return results;
  });
  expect(ratios.filter((item) => item.ratio < 4.5)).toEqual([]);
});

test("appearance: dark by default, light when chosen and remembered, or following the computer", async ({ page }) => {
  const background = () => page.evaluate(() => getComputedStyle(document.body).backgroundColor === "rgba(0, 0, 0, 0)"
    ? getComputedStyle(document.documentElement).backgroundColor : getComputedStyle(document.body).backgroundColor);
  await page.emulateMedia({ colorScheme: "light" });
  await page.goto("/");
  await expect(page.getByRole("status")).toContainText("100,000 tracks");
  expect(await background()).toBe("rgb(10, 10, 12)");
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  const themes = page.getByRole("group", { name: "Theme" });
  await expect(themes.getByRole("radio", { name: "Dark" })).toBeChecked();
  await themes.getByRole("radio", { name: "Light" }).check();
  await expect.poll(background).toBe("rgb(244, 244, 247)");
  await page.screenshot({ path: "test-results/theme-light-settings.png" });
  await page.keyboard.press("Escape");
  await page.screenshot({ path: "test-results/theme-light.png" });
  await page.reload();
  await expect.poll(background).toBe("rgb(244, 244, 247)");
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("group", { name: "Theme" }).getByRole("radio", { name: "Match this computer" }).check();
  await expect.poll(background).toBe("rgb(244, 244, 247)");
  await page.emulateMedia({ colorScheme: "dark" });
  await expect.poll(background).toBe("rgb(10, 10, 12)");
});

test("the navigation folds to a row of icons so the lists below get the height, and stays folded", async ({ page }) => {
  await page.goto("/");
  const sidebar = page.locator(".sidebar");
  const navigation = page.getByRole("navigation", { name: "Main navigation" });
  const tall = (await navigation.boundingBox())!.height;
  await expect(navigation.getByRole("button", { name: "Releases" })).toContainText("Releases");
  await page.getByRole("button", { name: "Hide the navigation" }).click();
  await expect(sidebar).toHaveClass(/nav-folded/);
  expect((await navigation.boundingBox())!.height).toBeLessThan(tall / 3);
  // Every screen is still a click away.
  await navigation.getByRole("button", { name: "Releases" }).click();
  await expect(page.getByRole("status")).toContainText("10,000 releases");
  await page.reload();
  await expect(page.getByRole("button", { name: "Show the navigation" })).toHaveAttribute("aria-expanded", "false");
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("checkbox", { name: "Hide the navigation" }).uncheck();
  await expect(sidebar).not.toHaveClass(/nav-folded/);
});

test("a sort can be reversed from its heading or the Reverse button, and Select all chooses every track", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("status")).toContainText("100,000 tracks");
  const rows = page.getByTestId("track-row");
  await expect(rows.first()).toContainText("Parallel Lines 100000");
  const headings = page.getByRole("group", { name: "Sort by column" });
  // The heading that already sorts the list turns it round: oldest first.
  await headings.getByRole("button", { name: "#" }).click();
  await expect(rows.first()).toContainText("After Hours 000001");
  await expect(page.getByRole("button", { name: "Reverse the order" })).toHaveAttribute("aria-pressed", "true");
  await page.getByRole("button", { name: "Reverse the order" }).click();
  await expect(rows.first()).toContainText("Parallel Lines 100000");
  // BPM high to low keeps tracks without a tempo last.
  await page.getByLabel("Sort library").selectOption("bpm");
  await page.getByRole("button", { name: "Reverse the order" }).click();
  await expect(rows.first().locator(".track-bpm")).toHaveText("140");
  await page.getByTestId("library-scroll").evaluate((element) => { element.scrollTop = element.scrollHeight; });
  await expect(rows.last().locator(".track-bpm")).toHaveText("--");
  // Cmd+A chooses the whole list, loaded or not.
  await rows.first().click();
  await page.keyboard.press("ControlOrMeta+KeyA");
  await expect(page.getByRole("status")).toContainText("100,000 chosen");
  await expect(rows.last()).toHaveAttribute("aria-pressed", "true");
});
