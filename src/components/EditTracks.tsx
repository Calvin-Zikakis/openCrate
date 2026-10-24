import { useEffect, useRef, useState } from "react";
import { fixCasing, removeJunk, removeNumberPrefix, tidySeparators } from "../library/cleanup";
import { loadRecipes, saveRecipes, type Recipe } from "../library/recipes";
import type { CustomFields, EditField, LibraryAdmin, TrackTags } from "../library/types";
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

/** Cleanup rules, each for the fields it suits. */
const RULES: { id: string; label: string; fields: EditField[]; apply: (value: string) => string }[] = [
  { id: "numbers", label: "Remove number prefixes", fields: ["title"], apply: removeNumberPrefix },
  { id: "casing", label: "Fix casing", fields: ["title", "artist", "album"], apply: fixCasing },
  { id: "junk", label: "Remove links and junk", fields: ["title", "artist", "album"], apply: removeJunk },
  { id: "separators", label: "Tidy separators", fields: ["artist", "albumArtist"], apply: tidySeparators },
];

const valueOf = (tags: TrackTags, field: EditField): string => {
  const value = tags[field];
  return value === null ? "" : String(value);
};

interface Props {
  admin: LibraryAdmin;
  trackIds: number[];
  onClose: () => void;
  /** Files were rewritten: the library should be read again. */
  onSaved: () => void;
}

/** Edit the details of one or more tracks. A field all of them share shows its value; one that
 * differs shows "Several values" and is left alone unless typed in. Cleanup rules change each
 * track's own values, shown before saving. Changes go into the files' tags. */
export function EditTracks({ admin, trackIds, onClose, onSaved }: Props) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [tracks, setTracks] = useState<TrackTags[] | null>(null);
  /** Fields typed in, applied to every track. */
  const [typed, setTyped] = useState<Partial<Record<EditField, string>>>({});
  /** Values a cleanup rule changed, per track. */
  const [cleaned, setCleaned] = useState<Map<number, Partial<Record<EditField, string>>>>(new Map());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [left, setLeft] = useState<string[]>([]);
  /** Saved recipes: typed values and cleanup rules to apply together. */
  const [recipes, setRecipes] = useState<Recipe[]>(() => loadRecipes(localStorage));
  /** The rules pressed in this editor, for saving as a recipe. */
  const [applied, setApplied] = useState<string[]>([]);
  const [naming, setNaming] = useState(false);
  const [recipeName, setRecipeName] = useState("");
  /** The owner's own fields on these tracks, kept in openCrate rather than in the files. */
  const [custom, setCustom] = useState<CustomFields | null>(null);
  const [newField, setNewField] = useState({ name: "", value: "" });
  const [customError, setCustomError] = useState("");

  useEffect(() => {
    dialog.current?.showModal();
    admin.trackTags(trackIds).then(setTracks, (reason: unknown) => setError(message(reason)));
    admin.customFields(trackIds).then(setCustom, () => setCustom({ fields: [], names: [] }));
  }, [admin, trackIds]);

  const single = trackIds.length === 1;
  const shared = (field: EditField): string | null => {
    if (!tracks || tracks.length === 0) return null;
    const first = valueOf(tracks[0]!, field);
    return tracks.every((track) => valueOf(track, field) === first) ? first : null;
  };
  /** What a track holds for a field now, counting what was typed and cleaned. */
  const current = (track: TrackTags, field: EditField) => typed[field] ?? cleaned.get(track.trackId)?.[field] ?? valueOf(track, field);

  const applyRule = (rule: (typeof RULES)[number]) => {
    if (!tracks) return;
    setApplied((before) => (before.includes(rule.id) ? before : [...before, rule.id]));
    setCleaned((before) => {
      const next = new Map(before);
      for (const track of tracks) {
        for (const field of rule.fields) {
          if (typed[field] !== undefined) continue;
          const now = current(track, field);
          const changed = rule.apply(now);
          if (changed !== now) next.set(track.trackId, { ...next.get(track.trackId), [field]: changed });
        }
      }
      return next;
    });
  };

  const typedChanges = Object.entries(typed)
    .filter(([field, value]) => value !== (shared(field as EditField) ?? undefined))
    .map(([field, value]) => ({ field: field as EditField, value: value ?? "" }));
  const cleanedChanges = [...cleaned.entries()].flatMap(([trackId, fields]) => {
    const track = tracks?.find((found) => found.trackId === trackId);
    return Object.entries(fields)
      .filter(([field, value]) => track && value !== valueOf(track, field as EditField))
      .map(([field, value]) => ({ trackId, title: track?.title ?? "", field: field as EditField, from: track ? valueOf(track, field as EditField) : "", to: value ?? "" }));
  });
  const nothing = typedChanges.length === 0 && cleanedChanges.length === 0;

  const save = async () => {
    setBusy(true);
    setError("");
    try {
      const notes: string[] = [];
      if (typedChanges.length > 0) notes.push(...(await admin.editTracks(trackIds, typedChanges)).left);
      if (cleanedChanges.length > 0) {
        const edits = [...new Set(cleanedChanges.map((change) => change.trackId))].map((trackId) => ({
          trackId,
          changes: cleanedChanges.filter((change) => change.trackId === trackId).map(({ field, to }) => ({ field, value: to })),
        }));
        notes.push(...(await admin.editEach(edits)).left);
      }
      onSaved();
      if (notes.length > 0) setLeft(notes);
      else onClose();
    } catch (reason) {
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

  return <dialog ref={dialog} className="settings-dialog editor-dialog" aria-labelledby="editor-title" onCancel={onClose}>
    <div className="dialog-heading">
      <div><h2 id="editor-title">{single ? "Edit details" : `Edit ${trackIds.length} tracks`}</h2></div>
      <button className="icon-button" aria-label="Close the editor" onClick={onClose}><Icon name="close" /></button>
    </div>
    <div className="dialog-body">
      {!tracks && !error && <p className="settings-copy" role="status">Reading the tags...</p>}
      {tracks && <>
        <div className="editor-fields">{FIELDS.filter((field) => single || !field.single).map((field) => {
          const common = shared(field.id);
          return <label key={field.id} className="editor-field">
            <span>{field.label}</span>
            <input {...PLAIN_TEXT} value={typed[field.id] ?? common ?? ""} inputMode={field.numeric ? "numeric" : undefined}
              placeholder={common === null ? "Several values" : ""}
              onChange={(event) => setTyped((before) => ({ ...before, [field.id]: event.target.value }))} />
          </label>;
        })}</div>
        {(recipes.length > 0 || naming || Object.keys(typed).length > 0 || applied.length > 0) && <div className="recipes">
          {recipes.length > 0 && <label className="crate-pick">Recipe
            <select aria-label="Apply a recipe" value="" onChange={(event) => {
              const recipe = recipes.find((found) => found.name === event.target.value);
              if (!recipe) return;
              setTyped((before) => ({ ...before, ...recipe.values }));
              for (const rule of RULES) if (recipe.rules.includes(rule.id)) applyRule(rule);
            }}>
              <option value="">Apply a recipe...</option>
              {recipes.map((recipe) => <option key={recipe.name} value={recipe.name}>{recipe.name}</option>)}
            </select>
          </label>}
          {!naming && (Object.keys(typed).length > 0 || applied.length > 0) && <button className="link-button" onClick={() => setNaming(true)}>Save as recipe...</button>}
          {naming && <span className="recipe-name">
            <input {...PLAIN_TEXT} aria-label="Recipe name" value={recipeName} placeholder="Name" onChange={(event) => setRecipeName(event.target.value)} />
            <button className="secondary-button" disabled={!recipeName.trim()} onClick={() => {
              const recipe: Recipe = { name: recipeName.trim(), values: { ...typed }, rules: applied };
              const next = [...recipes.filter((found) => found.name !== recipe.name), recipe];
              setRecipes(next);
              saveRecipes(localStorage, next);
              setNaming(false);
              setRecipeName("");
            }}>Save</button>
            <button className="link-button" onClick={() => setNaming(false)}>Cancel</button>
          </span>}
        </div>}
        <h3>Your fields</h3>
        <p className="settings-copy">Fields of your own, such as an energy or a mood. They are kept in openCrate, not in the files, and are saved as you type them.</p>
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
        <div className="cleanup-buttons">{RULES.map((rule) => <button key={rule.id} className="secondary-button" disabled={busy} onClick={() => applyRule(rule)}>{rule.label}</button>)}</div>
        {cleanedChanges.length > 0 && <ul className="cleanup-changes" aria-label="Cleanup changes">{cleanedChanges.map((change) => <li key={`${change.trackId}:${change.field}`}>
          <strong>{change.title || "Untitled"}</strong><small>{`${label(change.field)}: ${change.from || "(empty)"} -> ${change.to || "(empty)"}`}</small>
        </li>)}</ul>}
        <p className="settings-copy">Saving writes into the files' tags. If rekordbox has these tracks, the next Sync with rekordbox offers the change to it.</p>
        {left.length > 0 && <div role="alert" className="inline-error"><p>Some files were left as they were:</p><ul>{left.map((why) => <li key={why}>{why}</li>)}</ul></div>}
        {error && <p role="alert" className="inline-error">{error}</p>}
        <div className="warning-actions">
          <button className="import-button" disabled={busy || nothing} onClick={() => void save()}>{busy ? "Saving..." : nothing ? "Save" : `Save ${plural(typedChanges.length + cleanedChanges.length, "change", "changes")}`}</button>
          <button className="secondary-button" disabled={busy} onClick={onClose}>{left.length > 0 ? "Close" : "Cancel"}</button>
        </div>
      </>}
      {error && !tracks && <p role="alert" className="inline-error">{error}</p>}
    </div>
  </dialog>;
}
