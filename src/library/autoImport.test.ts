import { describe, expect, it } from "vitest";
import { autoImport, loadAutoImport, oneAtATime } from "./autoImport";
import type { DownloadRelease, ImportPlan, LibraryAdmin } from "./types";

const release = (name: string, extra: Partial<DownloadRelease> = {}): DownloadRelease => ({
  folderId: 9, folderLabel: "Downloads", name, loose: false, state: "ready", quietSeconds: null, audioFiles: 1, archives: 0,
  alreadyThere: false, imported: false, newestModifiedMs: 1, ...extra,
});

const plan = (name: string, guessed: boolean): ImportPlan => ({
  release: name, rootLabel: "Music", ready: 1, blocked: 0,
  files: [{ source: `${name}/01.aif`, destination: `A/${name}.aif`, shortened: false, problem: null, becomes: null,
    tags: { fields: [{ name: "Title", value: "02 - NIGHT", guessed, tidiedFrom: null }], audio: null, length: null, readError: null, warnings: [] } }],
});

function fakeAdmin(imported: { name: string; accepted: unknown }[], byHand: string[] = []): LibraryAdmin {
  return {
    // What the download folders hold when the import's turn comes: one imported by hand meanwhile shows as imported.
    downloadReleases: async () => ({ folders: [], releases: ["Clean", "Guessed", "Fresh"].map((name) => release(name, { imported: byHand.includes(name) })) }),
    planRelease: async (_folder: number, name: string) => plan(name, name === "Guessed"),
    importRelease: async (_folder: number, name: string, rootId: number, options?: { accepted?: unknown }) => {
      imported.push({ name, accepted: options?.accepted });
      return { rootId, rootLabel: "Music", filed: 1, skipped: [] };
    },
  } as unknown as LibraryAdmin;
}

describe("importing on its own", () => {
  it("imports only new finished releases whose tags need no look, with the recipe, once each", async () => {
    const imported: { name: string; accepted: unknown }[] = [];
    const brought: number[] = [];
    const seen = new Set<string>();
    const releases = [
      release("Clean"), release("Guessed"), release("Downloading", { state: "downloading" }),
      release("Older", { alreadyThere: true }), release("Done", { imported: true }),
    ];
    const options = { convert: { lossless: null, lossy: null }, tidy: true };
    const recipe = { name: "Tidy", values: { genre: "Techno" }, rules: ["numbers", "casing"] };
    const outcome = await autoImport(fakeAdmin(imported), releases, 7, options, recipe, seen, async (root) => { brought.push(root); });
    expect(outcome).toEqual({ imported: ["Clean"], waiting: [{ name: "Guessed", why: "its tags need a look" }] });
    expect(imported).toEqual([{ name: "Clean", accepted: [{ source: "Clean/01.aif", title: "Night", genre: "Techno" }] }]);
    expect(brought).toEqual([7]);
    // Looked at again, nothing is tried twice.
    expect(await autoImport(fakeAdmin(imported), releases, 7, options, recipe, seen, async () => undefined)).toEqual({ imported: [], waiting: [] });
    // A release imported by hand while this one waited its turn is not imported again.
    const twice: { name: string; accepted: unknown }[] = [];
    expect(await autoImport(fakeAdmin(twice, ["Fresh"]), [release("Fresh")], 7, options, null, new Set(), async () => undefined)).toEqual({ imported: [], waiting: [] });
    expect(twice).toEqual([]);
    // Replaced settings stop a run before its next release.
    const fresh: { name: string; accepted: unknown }[] = [];
    await autoImport(fakeAdmin(fresh), [release("Clean")], 7, options, null, new Set(), async () => undefined, () => false);
    expect(fresh).toEqual([]);
  });
  it("runs imports one at a time and reads its settings safely", async () => {
    const order: string[] = [];
    const slow = oneAtATime(async () => { await new Promise((resolve) => setTimeout(resolve, 20)); order.push("first"); });
    const quick = oneAtATime(async () => { order.push("second"); });
    await Promise.all([slow, quick]);
    expect(order).toEqual(["first", "second"]);
    expect(loadAutoImport({ getItem: () => "{" })).toEqual({ on: false, rootId: null, recipe: null });
    expect(loadAutoImport({ getItem: () => JSON.stringify({ on: true, rootId: 3, recipe: "Tidy" }) })).toEqual({ on: true, rootId: 3, recipe: "Tidy" });
  });
});
