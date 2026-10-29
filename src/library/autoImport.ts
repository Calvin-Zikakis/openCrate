import { acceptedForRecipe, type Recipe } from "./recipes";
import type { DownloadRelease, ImportOptions, ImportPlan, LibraryAdmin } from "./types";

/** Importing on its own: off until the owner turns it on. Kept in this browser, like recipes. */
export interface AutoImportSettings {
  on: boolean;
  /** The music folder new releases go into; the first one available when unset. */
  rootId: number | null;
  /** A saved recipe applied to the tags as each release goes in. */
  recipe: string | null;
}

const KEY = "opencrate.autoImport";
export const AUTO_OFF: AutoImportSettings = { on: false, rootId: null, recipe: null };

export function loadAutoImport(storage: Pick<Storage, "getItem">): AutoImportSettings {
  try {
    const saved: unknown = JSON.parse(storage.getItem(KEY) ?? "null");
    if (typeof saved !== "object" || saved === null) return AUTO_OFF;
    const { on, rootId, recipe } = saved as Record<string, unknown>;
    return {
      on: on === true,
      rootId: typeof rootId === "number" && Number.isInteger(rootId) ? rootId : null,
      recipe: typeof recipe === "string" && recipe !== "" ? recipe : null,
    };
  } catch {
    return AUTO_OFF;
  }
}

export function saveAutoImport(storage: Pick<Storage, "setItem">, settings: AutoImportSettings) {
  try { storage.setItem(KEY, JSON.stringify(settings)); } catch { /* applied for now, not remembered */ }
}

/** Something a person should see before importing: a file that cannot go, a guessed value, or a warning. */
export function needsLook(plan: ImportPlan): boolean {
  return plan.blocked > 0 || plan.files.some((file) => (file.tags?.warnings.length ?? 0) > 0 || (file.tags?.fields.some((field) => field.guessed) ?? false));
}

/** How long to wait for the scan that brings imported files into the lists. */
const SCAN_WAIT_MS = 10 * 60_000;

/** New files join the library the way any file does: by a scan of the folder (`onScanned` then
 * refreshes the lists). With writing to rekordbox on, they then go straight into rekordbox's
 * Imported playlist. `imported` names the files just filed (from the import's answer): once the
 * scan knows them as tracks, they go on into the crate and the rekordbox playlist chosen on the
 * Import screen. Returns what happened after the scan, or "" when nothing did. */
export async function bringIn(admin: LibraryAdmin, rootId: number, onScanned?: () => void, imported: number[] = []): Promise<string> {
  await admin.scan(rootId).catch(() => undefined);
  for (let waited = 0; waited < SCAN_WAIT_MS; waited += 500) {
    if ((await admin.scanStatus().catch(() => null))?.state !== "running") break;
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  onScanned?.();
  const notes: string[] = [];
  const rekordbox = await admin.rekordboxDatabase().catch(() => null);
  const writing = Boolean(rekordbox?.writeEnabled && !rekordbox.writeNeedsConfirmation && rekordbox.writeTested);
  if (writing) {
    try {
      const done = await admin.sendNewToRekordbox();
      onScanned?.();
      if (done.added > 0) notes.push(`Added ${plural(done.added, "track", "tracks")} to rekordbox's Imported playlist.`);
    } catch (reason) {
      notes.push(`Not added to rekordbox yet: ${reason instanceof Error ? reason.message : "something went wrong."} They wait on the rekordbox screen until then.`);
    }
  }
  if (imported.length > 0) notes.push(...await followUp(admin, imported, writing, onScanned));
  return notes.join(" ");
}

const plural = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;

/** The imported tracks go on into the chosen crate and rekordbox playlist. Each step says what
 * it did, or why it could not. */
async function followUp(admin: LibraryAdmin, imported: number[], writing: boolean, onScanned?: () => void): Promise<string[]> {
  const target = await admin.importTarget().catch(() => null);
  if (!target || (target.crateId === null && target.playlistId === null)) return [];
  const notes: string[] = [];
  let tracks: number[];
  try {
    tracks = await admin.importedTracks(imported);
  } catch (reason) {
    return [`The imported tracks could not be found in the library yet: ${reason instanceof Error ? reason.message : "something went wrong."}`];
  }
  if (tracks.length === 0) return ["The imported files are not in the library yet, so they were not added to the crate or playlist."];
  if (target.crateId !== null) {
    const name = (await admin.crates().catch(() => [])).find((crate) => crate.id === target.crateId)?.name ?? "the crate";
    try {
      const added = await admin.addToCrate(target.crateId, tracks);
      notes.push(added > 0 ? `Added ${plural(added, "track", "tracks")} to the crate ${name}.` : `The crate ${name} already had them.`);
      onScanned?.();
    } catch (reason) {
      notes.push(`Could not add them to the crate ${name}: ${reason instanceof Error ? reason.message : "something went wrong."}`);
    }
  }
  if (target.playlistId !== null) {
    const name = (await admin.playlists().catch(() => [])).find((playlist) => playlist.id === target.playlistId)?.name ?? "the rekordbox playlist";
    if (!writing) {
      notes.push(`Not added to the rekordbox playlist ${name}: writing to rekordbox is off in Settings.`);
    } else {
      try {
        const done = await admin.addToRekordboxPlaylist(target.playlistId, tracks);
        notes.push(`Added ${plural(done.added, "track", "tracks")} to the rekordbox playlist ${done.playlist || name}.${done.leftOut.length > 0 ? ` Left out: ${done.leftOut.join("; ")}` : ""}`);
        onScanned?.();
      } catch (reason) {
        notes.push(`Could not add them to the rekordbox playlist ${name}: ${reason instanceof Error ? reason.message : "something went wrong."}`);
      }
    }
  }
  return notes;
}

let chain: Promise<unknown> = Promise.resolve();
/** Imports run one at a time, whoever starts them: two at once could aim at the same place. */
export function oneAtATime<T>(run: () => Promise<T>): Promise<T> {
  const next = chain.then(run, run);
  chain = next.catch(() => undefined);
  return next;
}

export interface AutoOutcome {
  /** Releases imported, by name. */
  imported: string[];
  /** Releases left for the owner, and why. */
  waiting: { name: string; why: string }[];
}

/** A release as it was when last looked at; one unchanged since is not looked at again. */
export const stampOf = (release: DownloadRelease) => `${release.folderId}:${release.name}:${release.audioFiles}:${release.newestModifiedMs}`;

/** Import, one at a time, each new release that has finished downloading and whose tags need no
 * look, with the recipe's values; `seen` holds the stamps already handled so none is tried twice.
 * `afterImport` brings the files into the library (a scan) and on to rekordbox. Once `current`
 * says no (the settings changed), no further release is started. */
export async function autoImport(
  admin: LibraryAdmin,
  releases: DownloadRelease[],
  rootId: number,
  options: ImportOptions,
  recipe: Recipe | null,
  seen: Set<string>,
  afterImport: (rootId: number, imported: number[]) => Promise<void>,
  current: () => boolean = () => true,
): Promise<AutoOutcome> {
  const outcome: AutoOutcome = { imported: [], waiting: [] };
  for (const release of releases) {
    if (!current()) break;
    if (release.imported || release.alreadyThere || release.state !== "ready") continue;
    const stamp = stampOf(release);
    if (seen.has(stamp)) continue;
    seen.add(stamp);
    try {
      const plan = await admin.planRelease(release.folderId, release.name, rootId, options);
      if (plan.ready === 0 || needsLook(plan)) {
        outcome.waiting.push({ name: release.name, why: plan.ready === 0 ? "nothing in it can go in yet" : "its tags need a look" });
        continue;
      }
      const accepted = recipe ? acceptedForRecipe(plan.files, recipe) : [];
      const done = await oneAtATime(async () => {
        // An import by hand may have gone first while this waited its turn: look again, so the
        // release is never filed twice (into another folder, or under a recipe's names).
        if (!current()) return null;
        const now = (await admin.downloadReleases()).releases.find((other) => other.folderId === release.folderId && other.name === release.name);
        if (!now || now.imported || now.alreadyThere) return null;
        const filed = await admin.importRelease(release.folderId, release.name, rootId, { ...options, accepted });
        if (filed.filed > 0) await afterImport(filed.rootId, filed.imported);
        return filed;
      });
      if (!done) continue;
      if (done.filed > 0) outcome.imported.push(release.name);
      else outcome.waiting.push({ name: release.name, why: done.skipped[0]?.problem ?? "nothing was imported" });
    } catch (reason) {
      outcome.waiting.push({ name: release.name, why: reason instanceof Error ? reason.message : "something went wrong" });
    }
  }
  return outcome;
}
