import { useCallback, useEffect, useState } from "react";
import type { LibraryAdmin, RekordboxNewTracks, RekordboxSent, RekordboxWrite } from "../library/types";

const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");
const plural = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;

interface Props {
  admin: LibraryAdmin;
  /** Changes whenever rekordbox was read again, so the list is looked at again. */
  refresh: number;
  /** rekordbox's library was written and read again: the sidebar's playlists changed. */
  onWritten?: () => void;
}

/** The rekordbox screen: tracks openCrate filed that rekordbox does not have yet. */
export function SendToRekordbox({ admin, refresh, onWritten }: Props) {
  const [listed, setListed] = useState<RekordboxNewTracks | null>(null);
  const [unticked, setUnticked] = useState<Set<number>>(new Set());
  const [sent, setSent] = useState<RekordboxSent | null>(null);
  const [written, setWritten] = useState<RekordboxWrite | null>(null);
  /** Writing to rekordbox's library is on and ready: tracks go straight in. */
  const [direct, setDirect] = useState(false);
  const [busy, setBusy] = useState(false);
  const [showHidden, setShowHidden] = useState(false);
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    try {
      const [next, state] = await Promise.all([admin.rekordboxNewTracks(), admin.rekordboxDatabase().catch(() => null)]);
      setListed(next);
      setDirect(Boolean(state?.writeEnabled && !state.writeNeedsConfirmation && state.writeTested));
    } catch (reason) {
      setError(message(reason));
    }
  }, [admin]);

  useEffect(() => { void load(); }, [load, refresh]);

  const outcome = written && <div role="status" className="send-steps">
    <p className="settings-copy">{written.added === 0
      ? "Nothing new went into rekordbox."
      : `Added ${plural(written.added, "track", "tracks")} to rekordbox's ${written.playlist} playlist${written.imported > 0 ? ` (${written.imported} new to rekordbox)` : ""}. A backup of rekordbox's library was taken first; it is listed in Settings.`}</p>
    {written.leftOut.length > 0 && <ul className="skipped-list">{written.leftOut.map((why) => <li key={why}>{why}</li>)}</ul>}
    {written.readError && <p className="inline-error">{`rekordbox was written, but reading it again did not work: ${written.readError}`}</p>}
  </div>;
  if (!listed || listed.tracks.length === 0) return error ? <p role="alert" className="inline-error">{error}</p> : outcome || null;

  const offered = listed.tracks.filter((track) => !track.hidden);
  const hidden = listed.tracks.filter((track) => track.hidden);
  const chosen = offered.filter((track) => !unticked.has(track.fileId)).map((track) => track.fileId);
  const hide = async (fileId: number, keepOut: boolean) => {
    setBusy(true);
    setError("");
    try {
      setListed(await admin.hideFromRekordbox([fileId], keepOut));
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  };
  const toggle = (fileId: number) => setUnticked((current) => {
    const next = new Set(current);
    if (next.has(fileId)) next.delete(fileId);
    else next.add(fileId);
    return next;
  });
  const send = async () => {
    setBusy(true);
    setError("");
    try {
      if (direct) {
        setWritten(await admin.sendNewToRekordbox(chosen));
        onWritten?.();
      } else {
        setSent(await admin.sendToRekordbox(chosen));
      }
      await load();
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  };
  const stale = listed.mirrorReadMs === null
    ? "rekordbox has not been read yet, so openCrate cannot tell which of these it already has. Read rekordbox first so they are left out."
    : listed.newestFiledMs !== null && listed.mirrorReadMs < listed.newestFiledMs
      ? "rekordbox was last read before these were filed. Read it again first so tracks it already has are left out."
      : "";

  return <section className="sync-card" role="region" aria-label="New in your library">
    <h2>New in your library</h2>
    <p className="settings-copy">{direct
      ? `${plural(offered.length, "track", "tracks")} openCrate filed that rekordbox does not have yet. Adding puts them straight into rekordbox, in its Imported playlist. rekordbox must be closed.`
      : `${plural(offered.length, "track", "tracks")} openCrate filed that rekordbox does not have yet. Sending writes them into openCrate's own rekordbox file; rekordbox itself is not changed.`}</p>
    {stale && <p className="inline-error">{stale}</p>}
    <ul className="folder-list">{offered.map((track) => <li key={track.fileId} className="folder-row">
      <label className="send-row">
        <input type="checkbox" checked={!unticked.has(track.fileId)} onChange={() => toggle(track.fileId)} aria-label={`${track.title}, ${track.artist}`} />
        <span className="folder-text">
          <strong>{track.title}</strong>
          <small>{[track.artist, track.album, track.container].filter(Boolean).join(" - ")}{track.offeredMs !== null ? " - Sent, waiting for rekordbox" : ""}</small>
        </span>
      </label>
      <button className="secondary-button" disabled={busy} aria-label={`Keep ${track.title} out of rekordbox`} onClick={() => void hide(track.fileId, true)}>Hide</button>
    </li>)}</ul>
    {hidden.length > 0 && <button className="link-button" aria-expanded={showHidden} onClick={() => setShowHidden(!showHidden)}>{showHidden ? "Hide the list of hidden tracks" : `Show ${plural(hidden.length, "hidden track", "hidden tracks")}`}</button>}
    {showHidden && <ul className="folder-list" aria-label="Kept out of rekordbox">{hidden.map((track) => <li key={track.fileId} className="folder-row">
      <span className="folder-text"><strong>{track.title}</strong><small>{[track.artist, track.album].filter(Boolean).join(" - ")}</small></span>
      <button className="secondary-button" disabled={busy} aria-label={`Bring ${track.title} back`} onClick={() => void hide(track.fileId, false)}>Bring back</button>
    </li>)}</ul>}
    {error && <p role="alert" className="inline-error">{error}</p>}
    <button className="import-button" disabled={busy || chosen.length === 0} onClick={() => void send()}>{direct ? `Add ${chosen.length} to rekordbox` : `Send ${chosen.length} to rekordbox`}</button>
    {outcome}
    {sent && <div role="status" className="send-steps">
      <p className="settings-copy">{`${plural(sent.tracks, "track is", "tracks are")} in openCrate's rekordbox file:`} <code>{sent.xmlPath}</code></p>
      <ol className="settings-copy">
        <li>The first time only: in rekordbox, open Preferences, then Advanced, then rekordbox xml, and choose this file as the Imported Library. Then in Preferences, View, Layout, tick rekordbox xml so it shows in the tree. Restart rekordbox if it does not appear.</li>
        <li>{`In rekordbox's tree, open rekordbox xml, then the playlist "${listed.playlist}". Refresh it if it is already open.`}</li>
        <li>Select its tracks and drag them onto your Imported playlist.</li>
      </ol>
      <p className="settings-copy">These steps are still to be checked on a real rekordbox. The next read of rekordbox shows which tracks arrived.</p>
    </div>}
  </section>;
}
