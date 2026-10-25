import { useEffect, useState } from "react";
import type { FolderLayout as Layout, LibraryAdmin } from "../library/types";
import { PLAIN_TEXT } from "./plainText";

const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");

/** Settings: how imported music is arranged in the music folder. Saved in the library. */
export function FolderLayout({ admin, open }: { admin: LibraryAdmin; open: boolean }) {
  const [layout, setLayout] = useState<Layout | null>(null);
  const [custom, setCustom] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    if (!open) return;
    let stopped = false;
    admin.folderLayout().then((found) => {
      if (stopped) return;
      setLayout(found);
      setCustom(found.layout);
    }, (reason: unknown) => { if (!stopped) setError(message(reason)); });
    return () => { stopped = true; };
  }, [admin, open]);

  const choose = async (next: string) => {
    setError("");
    try {
      const saved = await admin.setFolderLayout(next);
      setLayout(saved);
      setCustom(saved.layout);
    } catch (reason) {
      setError(message(reason));
    }
  };

  if (!layout) return <><h3>Folder layout</h3>{error && <p role="alert" className="inline-error">{error}</p>}</>;
  const preset = layout.presets.find((item) => item.layout === layout.layout);
  return <>
    <h3>Folder layout</h3>
    <p className="settings-copy">How imported music is arranged in your music folder. Music already there stays where it is unless you move it with Folder layout on the Clean up screen.</p>
    <div className="layout-options" role="radiogroup" aria-label="Folder layout">
      {layout.presets.map((item) => <label key={item.layout} className="layout-option">
        <input type="radio" name="folder-layout" checked={preset?.layout === item.layout} onChange={() => void choose(item.layout)} />
        {item.name}
      </label>)}
      <label className="layout-option">
        <input type="radio" name="folder-layout" checked={!preset} onChange={() => document.getElementById("custom-folder-layout")?.focus()} />
        Custom
      </label>
    </div>
    <form className="layout-custom" onSubmit={(event) => { event.preventDefault(); void choose(custom); }}>
      <input {...PLAIN_TEXT} id="custom-folder-layout" aria-label="Custom folder layout" value={custom} onChange={(event) => setCustom(event.target.value)} />
      <button className="secondary-button" type="submit" disabled={custom.trim() === layout.layout}>Use this</button>
    </form>
    <p className="settings-copy layout-help">{"Fields: {folder_artist}, {artist}, {album_artist}, {release}, {title}, {year}, {track:02}, {disc}, {remixer}. Use / for a folder."}</p>
    <p className="settings-copy">Example: <code>{layout.example}</code></p>
    {error && <p role="alert" className="inline-error">{error}</p>}
  </>;
}
