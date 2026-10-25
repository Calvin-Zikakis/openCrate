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
 * Imported playlist. Returns what happened in rekordbox, or "" when nothing did. */
export async function bringIn(admin: LibraryAdmin, rootId: number, onScanned?: () => void): Promise<string> {
  await admin.scan(rootId).catch(() => undefined);
  for (let waited = 0; waited < SCAN_WAIT_MS; waited += 500) {
    if ((await admin.scanStatus().catch(() => null))?.state !== "running") break;
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  onScanned?.();
  const rekordbox = await admin.rekordboxDatabase().catch(() => null);
  if (!rekordbox?.writeEnabled || rekordbox.writeNeedsConfirmation || !rekordbox.writeTested) return "";
  try {
    const done = await admin.sendNewToRekordbox();
    onScanned?.();
    return done.added > 0 ? `Added ${done.added} ${done.added === 1 ? "track" : "tracks"} to rekordbox's Imported playlist.` : "";
  } catch (reason) {
    return `Not added to rekordbox yet: ${reason instanceof Error ? reason.message : "something went wrong."} They wait on the rekordbox screen until then.`;
  }
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
  afterImport: (rootId: number) => Promise<void>,
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
        const filed = await admin.importRelease(release.folderId, release.name, rootId, { ...options, accepted });
        if (filed.filed > 0) await afterImport(filed.rootId);
        return filed;
      });
      if (done.filed > 0) outcome.imported.push(release.name);
      else outcome.waiting.push({ name: release.name, why: done.skipped[0]?.problem ?? "nothing was imported" });
    } catch (reason) {
      outcome.waiting.push({ name: release.name, why: reason instanceof Error ? reason.message : "something went wrong" });
    }
  }
  return outcome;
}
