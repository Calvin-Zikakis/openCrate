import { useEffect, useRef, useState } from "react";
import { loadRecipes, RULES, saveRecipes, type Recipe } from "../library/recipes";
import type { CustomFields, EditField, LibraryAdmin, TrackArt, TrackCovers, TrackTags } from "../library/types";
import { Artwork } from "./Artwork";
import { Icon } from "./Icon";
import { PLAIN_TEXT } from "./plainText";

const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");
const plural = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;

const FIELDS: { id: EditField; label: string; single?: boolean; numeric?: boolean }[] = [
  { id: "title", label: "Title" },
  { id: "artist", label: "Artist" },
  { id: "album", label: "Album" },
  { id: "albumArtist", label: "Album artist" },
  { id: "genre", label: "Genre" },
  { id: "label", label: "Label" },
  { id: "year", label: "Year", numeric: true },
  { id: "track", label: "Track number", single: true, numeric: true },
];

/** What a recipe keeps and applies: never a track number, which belongs to one track alone. */
const recipeValues = (values: Partial<Record<EditField, string>>) =>
  Object.fromEntries(Object.entries(values).filter(([field]) => field !== "track")) as Partial<Record<EditField, string>>;

/** A picked image's address, changed with each pick so the window does not show the last one. */
const withVersion = (url: string | null | undefined, version: number) => (url ? `${url}${url.includes("?") ? "&" : "?"}v=${version}` : null);

/** A field's value as the editor shows it; a track number or year of 0 is none. */
const valueOf = (tags: TrackTags, field: EditField): string => {
  const value = tags[field];
  return value === null || value === 0 ? "" : String(value);
};

interface Props {
  admin: LibraryAdmin;
  /** Where a track's own picture, or the one picked and not yet saved, can be shown. */
  coverUrl?: (of: number | "picked") => string | null;
  trackIds: number[];
  onClose: () => void;
  /** Files were rewritten: the library should be read again. */
  onSaved: () => void;
}

/** Edit the details of one or more tracks. A field all of them share shows its value; one that
 * differs shows "Several values" and is left alone unless typed in. Cleanup rules change each
 * track's own values, shown before saving. Changes go into the files' tags. */
export function EditTracks({ admin, coverUrl, trackIds, onClose, onSaved }: Props) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [tracks, setTracks] = useState<TrackTags[] | null>(null);
  /** Fields typed in, applied to every track. */
  const [typed, setTyped] = useState<Partial<Record<EditField, string>>>({});
  /** Values a cleanup rule changed, per track. */
  const [cleaned, setCleaned] = useState<Map<number, Partial<Record<EditField, string>>>>(new Map());
  /** What the last rule or recipe did, when the fields alone do not show it. */
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [left, setLeft] = useState<string[]>([]);
  /** Saved recipes: typed values and cleanup rules to apply together. */
  const [recipes, setRecipes] = useState<Recipe[]>(() => loadRecipes(localStorage));
  /** The rules pressed in this editor, for saving as a recipe. */
  const [applied, setApplied] = useState<string[]>([]);
  const [naming, setNaming] = useState(false);
  /** The saved recipes listed, to delete one. */
  const [managing, setManaging] = useState(false);
  const [recipeName, setRecipeName] = useState("");
  /** The owner's own fields on these tracks, kept in openCrate rather than in the files. */
  const [custom, setCustom] = useState<CustomFields | null>(null);
  const [newField, setNewField] = useState({ name: "", value: "" });
  const [customError, setCustomError] = useState("");
  /** The chosen tracks' pictures, and the cover change waiting for Save. */
  const [covers, setCovers] = useState<TrackCovers | null>(null);
  const [art, setArt] = useState<TrackArt | null>(null);
  /** Bumped on each pick, so the preview loads the new image. */
  const [picks, setPicks] = useState(0);
  /** Closing with changes not saved waits for a yes. */
  const [discarding, setDiscarding] = useState(false);

  useEffect(() => {
    dialog.current?.showModal();
    admin.trackTags(trackIds).then(setTracks, (reason: unknown) => setError(message(reason)));
    admin.customFields(trackIds).then(setCustom, () => setCustom({ fields: [], names: [] }));
    admin.trackCovers(trackIds).then(setCovers, () => setCovers(null));
  }, [admin, trackIds]);

  const single = trackIds.length === 1;
  const shared = (field: EditField): string | null => {
    if (!tracks || tracks.length === 0) return null;
    const first = valueOf(tracks[0]!, field);
    return tracks.every((track) => valueOf(track, field) === first) ? first : null;
  };

  /** Run rules in order, each on what the one before left. A field typed in (`typing`, which a
   * recipe passes before its values reach state) is the typed value's, so rules leave it.
   * Returns a note naming the rules that found nothing to change. */
  const applyRules = (rules: (typeof RULES)[number][], typing: Partial<Record<EditField, string>> = typed): string => {
    if (!tracks) return "";
    setApplied((before) => [...before, ...rules.map((rule) => rule.id).filter((id) => !before.includes(id))]);
    const next = new Map(cleaned);
    const idle: string[] = [];
    for (const rule of rules) {
      let changedAny = false;
      for (const track of tracks) {
        for (const field of rule.fields) {
          if (typing[field] !== undefined) continue;
          const now = next.get(track.trackId)?.[field] ?? valueOf(track, field);
          const changed = rule.apply(now);
          if (changed === now) continue;
          next.set(track.trackId, { ...next.get(track.trackId), [field]: changed });
          changedAny = true;
        }
      }
      if (!changedAny) idle.push(rule.label);
    }
    setCleaned(next);
    return idle.length > 0 ? `${idle.join(", ")}: nothing to change.` : "";
  };

  const saveRecipe = () => {
    const name = recipeName.trim();
    if (!name) return;
    // A field typed in and emptied back to how it was is not an edit: kept, it would empty that
    // field on every track the recipe is applied to.
    const meant = Object.fromEntries(Object.entries(typed).filter(([field, value]) => value?.trim() || value !== (shared(field as EditField) ?? undefined)));
    const recipe: Recipe = { name, values: recipeValues(meant), rules: applied };
    const next = [...recipes.filter((found) => found.name !== name), recipe];
    setRecipes(next);
    saveRecipes(localStorage, next);
    setNaming(false);
    setRecipeName("");
    setNote(recipes.some((found) => found.name === name) ? `Replaced the recipe "${name}".` : `Saved the recipe "${name}".`);
  };

  const typedChanges = Object.entries(typed)
    .filter(([field, value]) => value !== (shared(field as EditField) ?? undefined))
    .map(([field, value]) => ({ field: field as EditField, value: value ?? "" }));
  // A field typed in after a rule ran saves what was typed, not what the rule made.
  const cleanedChanges = [...cleaned.entries()].flatMap(([trackId, fields]) => {
    const track = tracks?.find((found) => found.trackId === trackId);
    return Object.entries(fields)
      .filter(([field, value]) => track && typed[field as EditField] === undefined && value !== valueOf(track, field as EditField))
      .map(([field, value]) => ({ trackId, title: track?.title ?? "", field: field as EditField, from: track ? valueOf(track, field as EditField) : "", to: value ?? "" }));
  });
  const nothing = typedChanges.length === 0 && cleanedChanges.length === 0 && art === null;
  const changeCount = typedChanges.length + cleanedChanges.length + (art === null ? 0 : 1);
  const coverNow = covers?.state === "none" ? "no picture" : covers?.state === "mixed" ? "several pictures" : "the current picture";

  const pick = async () => {
    setError("");
    try {
      if (await admin.pickCover()) {
        setArt("picked");
        setPicks((count) => count + 1);
      }
    } catch (reason) {
      setError(message(reason));
    }
  };

  const save = async () => {
    setBusy(true);
    setError("");
    try {
      // One call, so the save is one entry in Recent changes and one Undo puts all of it back.
      const edits = trackIds
        .map((trackId) => ({
          trackId,
          changes: [...typedChanges, ...cleanedChanges.filter((change) => change.trackId === trackId).map(({ field, to }) => ({ field, value: to }))],
        }))
        .filter((edit) => edit.changes.length > 0);
      const done = edits.length > 0 ? await admin.editEach(edits) : { left: [] as string[] };
      const covered = art === null ? { left: [] as string[] } : await admin.setTracksArt(trackIds, art);
      onSaved();
      const leftOver = [...done.left, ...covered.left];
      if (leftOver.length === 0) {
        onClose();
        return;
      }
      // Some files changed and some did not: show what the files hold now, so a second Save
      // starts from there rather than from the values the editor opened with.
      setLeft(leftOver);
      setTyped({});
      setCleaned(new Map());
      setArt(null);
      setNote("");
      setTracks(await admin.trackTags(trackIds));
      setCovers(await admin.trackCovers(trackIds));
    } catch (reason) {
      // Files may have changed before the failure; the list should show what they hold.
      onSaved();
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  };

  const label = (field: EditField) => FIELDS.find((found) => found.id === field)?.label ?? field;

  /** A custom field's value when every chosen track has the same one; "" when none has it. */
  const customShared = (name: string): string | null => {
    const values = trackIds.map((id) => custom?.fields.find((field) => field.trackId === id && field.name === name)?.value ?? "");
    return values.every((value) => value === values[0]) ? values[0]! : null;
  };
  const customNames = [...new Set((custom?.fields ?? []).map((field) => field.name))];
  const setField = async (name: string, value: string) => {
    setCustomError("");
    try {
      await admin.setCustomField(trackIds, name, value);
      setCustom(await admin.customFields(trackIds));
      onSaved();
    } catch (reason) {
      setCustomError(message(reason));
    }
  };

  /** Close, unless there are changes not saved yet: those are asked about first. */
  const requestClose = () => {
    if (busy) return;
    if (changeCount > 0) setDiscarding(true);
    else onClose();
  };

  return <dialog ref={dialog} className="settings-dialog editor-dialog" aria-labelledby="editor-title" onCancel={(event) => { event.preventDefault(); if (discarding) setDiscarding(false); else requestClose(); }} onClose={onClose}>
    <div className="dialog-heading">
      <div><h2 id="editor-title">{single ? "Edit details" : `Edit ${trackIds.length} tracks`}</h2></div>
      <button className="icon-button" aria-label="Close the editor" onClick={requestClose}><Icon name="close" /></button>
    </div>
    <div className="dialog-body">
      {discarding && <div className="crate-confirm editor-discard" role="alertdialog" aria-label="Discard the changes?" onKeyDown={(event) => { if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); setDiscarding(false); } }}>
        <span>{`${plural(changeCount, "change is", "changes are")} not saved yet.`}</span>
        <button className="link-button" onClick={onClose}>Discard changes</button>
        <button className="link-button" autoFocus onClick={() => setDiscarding(false)}>Keep editing</button>
      </div>}
      {!tracks && !error && <p className="settings-copy" role="status">Reading the tags...</p>}
      {tracks && <>
        <div className="editor-cover" role="group" aria-label="Cover">
          <Artwork color={1} src={art === "picked" ? withVersion(coverUrl?.("picked"), picks) : art === null && covers?.state === "same" ? coverUrl?.(trackIds[0]!) ?? null : null} />
          <div>
            <span>Cover</span>
            <small>{art === "picked" ? "The image you chose, written when you save." : art === "folder" ? "Each track's own image beside its files, written when you save." : art === "remove" ? "Removed when you save." : covers === null ? "Reading..." : covers.state === "none" ? (single ? "This track has no picture." : "These tracks have no picture.") : covers.state === "mixed" ? "These tracks have different pictures." : single ? "The picture in this track's file." : "The picture these tracks share."}</small>
            <div className="cover-actions">
              <button className="link-button" disabled={busy} onClick={() => void pick()}>Choose image...</button>
              {covers?.folderImage && art !== "folder" && <button className="link-button" disabled={busy} onClick={() => setArt("folder")}>Use folder image</button>}
              {(covers?.state ?? "none") !== "none" && art !== "remove" && <button className="link-button" disabled={busy} onClick={() => setArt("remove")}>Remove cover</button>}
              {art !== null && <button className="link-button" disabled={busy} onClick={() => setArt(null)}>Keep the current cover</button>}
            </div>
          </div>
        </div>
        <div className="editor-fields">{FIELDS.filter((field) => single || !field.single).map((field) => {
          const common = shared(field.id);
          return <label key={field.id} className="editor-field">
            <span>{field.label}</span>
            <input {...PLAIN_TEXT} value={typed[field.id] ?? common ?? ""} inputMode={field.numeric ? "numeric" : undefined}
              placeholder={common === null ? "Several values" : ""}
              onChange={(event) => {
                const value = event.target.value;
                setTyped((before) => {
                  // Emptied again on a field the tracks differ in: back to "Several values",
                  // not a blank written over every one of them.
                  if (value.trim() === "" && common === null) {
                    const rest = { ...before };
                    delete rest[field.id];
                    return rest;
                  }
                  return { ...before, [field.id]: value };
                });
              }} />
          </label>;
        })}</div>
        {(recipes.length > 0 || naming || Object.keys(typed).length > 0 || applied.length > 0) && <div className="recipes">
          {recipes.length > 0 && <label className="crate-pick">Recipe
            <select aria-label="Apply a recipe" value="" onChange={(event) => {
              const recipe = recipes.find((found) => found.name === event.target.value);
              if (!recipe) return;
              const typing = { ...typed, ...recipeValues(recipe.values) };
              setTyped(typing);
              const idle = applyRules(recipe.rules.flatMap((id) => RULES.filter((rule) => rule.id === id)), typing);
              setNote(`Applied "${recipe.name}".${idle ? ` ${idle}` : ""}`);
            }}>
              <option value="">Apply a recipe</option>
              {recipes.map((recipe) => <option key={recipe.name} value={recipe.name}>{recipe.name}</option>)}
            </select>
          </label>}
          {!naming && (Object.keys(typed).length > 0 || applied.length > 0) && <button className="link-button" onClick={() => setNaming(true)}>Save as recipe...</button>}
          {recipes.length > 0 && <button className="link-button" aria-expanded={managing} onClick={() => setManaging(!managing)}>{managing ? "Done with recipes" : "Manage recipes"}</button>}
          {naming && <span className="recipe-name">
            <input {...PLAIN_TEXT} autoFocus aria-label="Recipe name" value={recipeName} placeholder="Name" onChange={(event) => setRecipeName(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") saveRecipe();
                // Escape gives up on the name, not on the whole editor.
                if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); setNaming(false); }
              }} />
            <button className="secondary-button" disabled={!recipeName.trim()} onClick={saveRecipe}>{recipes.some((found) => found.name === recipeName.trim()) ? "Replace" : "Save"}</button>
            <button className="link-button" onClick={() => setNaming(false)}>Cancel</button>
          </span>}
          {naming && recipes.some((found) => found.name === recipeName.trim()) && <small className="recipe-replace">{`A recipe named "${recipeName.trim()}" exists; saving replaces it.`}</small>}
          {managing && <ul className="cleanup-changes recipe-list" aria-label="Saved recipes">{recipes.map((recipe) => <li key={recipe.name}>
            <strong>{recipe.name}</strong>
            <small>{[...Object.entries(recipe.values).map(([field, value]) => `${label(field as EditField)}: ${value || "(empty)"}`), ...recipe.rules.map((id) => RULES.find((rule) => rule.id === id)?.label ?? id)].join("; ") || "Nothing in it"}</small>
            <button className="link-button keep-button" aria-label={`Delete the recipe ${recipe.name}`} onClick={() => {
              const next = recipes.filter((found) => found.name !== recipe.name);
              setRecipes(next);
              saveRecipes(localStorage, next);
              setNote(`Deleted the recipe "${recipe.name}".`);
              if (next.length === 0) setManaging(false);
            }}>Delete</button>
          </li>)}</ul>}
        </div>}
        <h3>Your fields</h3>
        <p className="settings-copy">Fields of your own, such as an energy or a mood. They are kept in openCrate, not in the files, and are saved when you leave the field, so Cancel does not undo them.</p>
        <div className="custom-fields" role="group" aria-label="Your fields">
          {customNames.map((name) => {
            const shared = customShared(name);
            return <label key={name} className="editor-field"><span>{name}</span>
              <input {...PLAIN_TEXT} defaultValue={shared ?? ""} placeholder={shared === null ? "Several values" : ""} aria-label={`${name} value`}
                onBlur={(event) => { if (event.target.value !== (shared ?? "")) void setField(name, event.target.value); }} />
            </label>;
          })}
          <div className="custom-add">
            <input {...PLAIN_TEXT} list="custom-field-names" aria-label="New field name" placeholder="Field" value={newField.name} onChange={(event) => setNewField({ ...newField, name: event.target.value })} />
            <datalist id="custom-field-names">{(custom?.names ?? []).filter((name) => !customNames.includes(name)).map((name) => <option key={name} value={name} />)}</datalist>
            <input {...PLAIN_TEXT} aria-label="New field value" placeholder="Value" value={newField.value} onChange={(event) => setNewField({ ...newField, value: event.target.value })} />
            <button className="secondary-button" disabled={!newField.name.trim() || !newField.value.trim()} onClick={() => { void setField(newField.name, newField.value); setNewField({ name: "", value: "" }); }}>Add</button>
          </div>
          {customError && <p role="alert" className="inline-error">{customError}</p>}
        </div>
        <h3>Clean up</h3>
        <p className="settings-copy">{single ? "Each rule changes this track's own details." : "Each rule changes each track's own details."} What it would change is listed before you save.</p>
        <div className="cleanup-buttons">{RULES.map((rule) => <button key={rule.id} className="secondary-button" disabled={busy} onClick={() => setNote(applyRules([rule]))}>{rule.label}</button>)}</div>
        {note && <p className="settings-copy" role="status">{note}</p>}
        {(typedChanges.length > 0 || art !== null) && <ul className="cleanup-changes" aria-label="Changes to save">{art !== null && <li className={art === "remove" ? "emptied" : undefined}>
          <strong>{single ? tracks[0]?.title || "Untitled" : `All ${trackIds.length} tracks`}</strong>
          <small>{`Cover: ${coverNow} -> `}{art === "remove" ? <em>no picture, the picture is removed</em> : art === "picked" ? "the image you chose" : "each track's image beside its files"}</small>
        </li>}{typedChanges.map((change) => {
          const from = shared(change.field);
          const emptied = change.value.trim() === "";
          return <li key={change.field} className={emptied ? "emptied" : undefined}>
            <strong>{single ? tracks[0]?.title || "Untitled" : `All ${trackIds.length} tracks`}</strong>
            <small>{`${label(change.field)}: ${from === null ? "several values" : from || "(empty)"} -> `}{emptied ? <em>(empty), the tag is removed</em> : change.value}</small>
            <button className="link-button keep-button" title={`Keep the ${label(change.field).toLowerCase()} as it is`} onClick={() => setTyped((before) => { const rest = { ...before }; delete rest[change.field]; return rest; })}>Keep as it is</button>
          </li>;
        })}</ul>}
        {cleanedChanges.length > 0 && <ul className="cleanup-changes" aria-label="Cleanup changes">{cleanedChanges.map((change) => <li key={`${change.trackId}:${change.field}`}>
          <strong>{change.title || "Untitled"}</strong><small>{`${label(change.field)}: ${change.from || "(empty)"} -> ${change.to || "(empty)"}`}</small>
          <button className="link-button keep-button" title={`Keep the ${label(change.field).toLowerCase()} of ${change.title || "Untitled"} as it is`} onClick={() => setCleaned((before) => {
            const next = new Map(before);
            const fields = { ...next.get(change.trackId) };
            delete fields[change.field];
            next.set(change.trackId, fields);
            return next;
          })}>Keep as it is</button>
        </li>)}</ul>}
        <p className="settings-copy">Saving writes into the files' tags. If rekordbox has these tracks, the next Sync with rekordbox offers the change to it.{art !== null ? " A cover change is not kept in Recent changes, so Undo cannot bring the old picture back, and rekordbox keeps its own copy of the artwork until it reads the file again." : ""}</p>
        {left.length > 0 && <div role="alert" className="inline-error"><p>Some files were left as they were:</p><ul>{left.map((why) => <li key={why}>{why}</li>)}</ul></div>}
        {error && <p role="alert" className="inline-error">{error}</p>}
        <div className="warning-actions">
          <button className="import-button" disabled={busy || nothing} onClick={() => void save()}>{busy ? "Saving..." : nothing ? "Save" : `Save ${plural(changeCount, "change", "changes")}`}</button>
          <button className="secondary-button" disabled={busy} onClick={requestClose}>{left.length > 0 ? "Close" : "Cancel"}</button>
        </div>
      </>}
      {error && !tracks && <p role="alert" className="inline-error">{error}</p>}
    </div>
  </dialog>;
}
