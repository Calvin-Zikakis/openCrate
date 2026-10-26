import { useEffect, useRef, useState } from "react";
import { autoImport, bringIn, loadAutoImport, saveAutoImport, type AutoImportSettings, type AutoOutcome } from "../library/autoImport";
import { loadRecipes } from "../library/recipes";
import type { DownloadRelease, FolderInfo, LibraryAdmin } from "../library/types";
import { loadOptions } from "./ImportChoices";

/** How often the watch folders are looked at while importing on its own is on. */
const AUTO_MS = 10_000;
const NONE: AutoOutcome = { imported: [], waiting: [] };

/** Importing on its own, run by the app whatever screen shows: its settings, what it did, and a
 * way to change them. `onImported` refreshes the lists after files go in. */
export function useAutoImport(admin: LibraryAdmin | undefined, onImported: () => void) {
  const [settings, setSettings] = useState<AutoImportSettings>(() => loadAutoImport(localStorage));
  const [outcome, setOutcome] = useState<AutoOutcome>(NONE);
  /** Why the last look could not import anything, shown on the card; "" when it could. */
  const [problem, setProblem] = useState("");
  /** When the watch folders were last looked at, so the card shows it is alive. */
  const [lastLookMs, setLastLookMs] = useState<number | null>(null);
  const refresh = useRef(onImported);
  refresh.current = onImported;
  const change = (next: AutoImportSettings) => {
    setSettings(next);
    saveAutoImport(localStorage, next);
  };

  useEffect(() => {
    if (!admin || !settings.on) { setProblem(""); return; }
    let running = false;
    let live = true;
    // New settings give every release another look.
    const seen = new Set<string>();
    const tick = async () => {
      if (running) return;
      running = true;
      try {
        const roots = (await admin.folders()).filter((folder) => folder.availability === "available");
        const root = roots.find((folder) => folder.id === settings.rootId) ?? roots[0];
        if (!root) {
          setProblem("Paused: no music folder can be reached right now. It carries on when one is back.");
          return;
        }
        setProblem("");
        const snapshot = await admin.downloadReleases();
        setLastLookMs(Date.now());
        const layout = await admin.folderLayout().catch(() => null);
        const chosen = loadOptions();
        const options = layout?.canConvert ? chosen : { ...chosen, convert: { lossless: null, lossy: null } };
        const recipe = settings.recipe ? loadRecipes(localStorage).find((found) => found.name === settings.recipe) ?? null : null;
        const done = await autoImport(admin, snapshot.releases, root.id, options, recipe, seen, (rootId) => bringIn(admin, rootId, () => refresh.current()).then(() => undefined), () => live);
        // What a run did is kept even when the settings changed during it: it happened.
        if (done.imported.length === 0 && done.waiting.length === 0) return;
        setOutcome((before) => ({
          imported: [...before.imported, ...done.imported],
          waiting: [...before.waiting.filter((item) => !done.imported.includes(item.name) && !done.waiting.some((other) => other.name === item.name)), ...done.waiting],
        }));
      } catch (reason) {
        // Tried again at the next look.
        setProblem(`The last look did not work: ${reason instanceof Error ? reason.message : "something went wrong"}. It tries again shortly.`);
      } finally {
        running = false;
      }
    };
    void tick();
    const timer = setInterval(() => void tick(), AUTO_MS);
    return () => { live = false; clearInterval(timer); };
  }, [admin, settings]);

  return { settings, change, outcome, problem, lastLookMs, clear: () => setOutcome(NONE) };
}

export type AutoImport = ReturnType<typeof useAutoImport>;

interface Props {
  settings: AutoImportSettings;
  onChange: (next: AutoImportSettings) => void;
  outcome: AutoOutcome;
  problem?: string;
  /** When the folders were last looked at; null before the first look. */
  lastLookMs?: number | null;
  onClear?: () => void;
  roots: FolderInfo[];
  /** What the watch folders hold now, so a release imported by hand stops waiting. */
  releases: DownloadRelease[];
}

/** The Import screen's switch for importing on its own, and what it has done this session. */
export function AutoImportCard({ settings, onChange, outcome, problem = "", lastLookMs = null, onClear, roots, releases }: Props) {
  const recipes = loadRecipes(localStorage);
  const available = roots.filter((folder) => folder.availability === "available");
  const stillWaiting = outcome.waiting.filter((item) => releases.some((release) => release.name === item.name && !release.imported));
  return <div className="sync-card" role="region" aria-label="Import on its own">
    <h2>Import on its own</h2>
    <label className="send-row"><input type="checkbox" checked={settings.on} onChange={(event) => onChange({ ...settings, on: event.target.checked })} />Import new releases on their own once they finish downloading</label>
    <p className="settings-copy">Only releases whose tags need no look go in on their own, with the choices above. The rest wait below for you. With writing to rekordbox on, they go on to rekordbox's Imported playlist too. This works while openCrate is open.</p>
    <div className="auto-choices">
      {available.length > 1 && <label className="crate-pick">Music folder<select value={settings.rootId ?? available[0]?.id ?? ""} onChange={(event) => onChange({ ...settings, rootId: Number(event.target.value) })}>{available.map((folder) => <option key={folder.id} value={folder.id}>{folder.label}</option>)}</select></label>}
      <label className="crate-pick">Recipe<select value={settings.recipe ?? ""} onChange={(event) => onChange({ ...settings, recipe: event.target.value || null })}>
        <option value="">None</option>
        {recipes.map((recipe) => <option key={recipe.name} value={recipe.name}>{recipe.name}</option>)}
      </select></label>
      {recipes.length === 0 && <small>Save a recipe in the track editor to use one here.</small>}
    </div>
    {settings.on && problem && <p className="inline-error" role="alert">{problem}</p>}
    {settings.on && !problem && <p className="settings-copy">{`On. The watch folders are looked at every ${AUTO_MS / 1000} seconds${lastLookMs ? `; last look ${new Date(lastLookMs).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}` : ""}.`}</p>}
    {outcome.imported.length > 0 && <p className="settings-copy" role="status">{`Imported on their own: ${outcome.imported.join(", ")}.`} {onClear && <button className="link-button" onClick={onClear}>Clear</button>}</p>}
    {stillWaiting.length > 0 && <p className="settings-copy">{`Waiting for you: ${stillWaiting.map((item) => `${item.name} (${item.why})`).join("; ")}.`}</p>}
  </div>;
}
