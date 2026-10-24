import { useCallback, useEffect, useState } from "react";
import type { LibraryAdmin, RekordboxBackup, RekordboxDatabase as Status } from "../library/types";

const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");
const REASONS: Record<string, string> = {
  read: "Before reading", "before-write": "Before writing", "before-restore": "Before a restore", "before-restore-damaged": "Before a restore (the library was damaged)",
};
const when = (ms: number) => new Date(ms).toLocaleString();

/** Settings: the opt-in switch for reading rekordbox's own library, and its backups. */
export function RekordboxDatabase({ admin, open }: { admin: LibraryAdmin; open: boolean }) {
  const [status, setStatus] = useState<Status | null>(null);
  const [backups, setBackups] = useState<RekordboxBackup[]>([]);
  const [warning, setWarning] = useState(false);
  const [writeWarning, setWriteWarning] = useState(false);
  const [restoring, setRestoring] = useState<RekordboxBackup | null>(null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    try {
      const [next, saved] = await Promise.all([admin.rekordboxDatabase(), admin.rekordboxBackups()]);
      setStatus(next);
      setBackups(saved);
    } catch (reason) {
      setError(message(reason));
    }
  }, [admin]);

  useEffect(() => {
    if (open) void load();
  }, [open, load]);

  const act = async (work: () => Promise<void>) => {
    setBusy(true);
    setError("");
    setNote("");
    try {
      await work();
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  };
  const enable = (enabled: boolean) => act(async () => {
    setStatus(await admin.setRekordboxDatabase(enabled));
    setWarning(false);
  });
  const toggle = () => {
    if (status?.enabled) void enable(false);
    else setWarning(true);
  };
  const enableWrite = (enabled: boolean) => act(async () => {
    setStatus(await admin.setRekordboxWrite(enabled));
    setWriteWarning(false);
  });
  const toggleWrite = () => {
    if (status?.writeEnabled) void enableWrite(false);
    else setWriteWarning(true);
  };
  const readingOn = Boolean(status?.enabled && !status.needsConfirmation);
  const pin = (backup: RekordboxBackup) => act(async () => {
    setBackups(await admin.pinRekordboxBackup(backup.name, !backup.pinned));
  });
  const restore = (backup: RekordboxBackup) => act(async () => {
    await admin.restoreRekordboxBackup(backup.name);
    setRestoring(null);
    setNote(`Restored the library from ${when(backup.takenMs)}. The library it replaced is kept as a pinned backup.`);
    setBackups(await admin.rekordboxBackups());
  });

  const version = status?.appVersion ?? "this version";
  return <>
    <h3>rekordbox</h3>
    <p className="settings-copy">An XML export is read on the rekordbox screen. openCrate can also read rekordbox's own library directly, which brings in My Tags and needs no export.</p>
    {status && !status.installed && <p className="settings-copy">rekordbox's library was not found on this computer.</p>}
    <label className="setting-row">
      <span><strong>Let openCrate read rekordbox's library directly</strong><small>Off by default. This only reads: each read works on a backup copy taken while rekordbox is closed. Writing is a separate switch below.</small></span>
      <input type="checkbox" checked={status?.enabled ?? false} disabled={!status?.installed || busy} onChange={toggle} />
    </label>
    {warning && <div className="warning-panel" role="region" aria-label="Before you turn this on">
      <h4>Before you turn this on</h4>
      <p className="settings-copy">rekordbox's library is not a published format. A rekordbox update can change it, and a version openCrate has not seen may be read wrongly.</p>
      <p className="settings-copy">To protect your library, openCrate only reads a backup copy it takes first, rekordbox must be closed while it does, and any backup can be put back from this page. If rekordbox changes version, this switch asks you again.</p>
      <div className="warning-actions">
        <button className="import-button" disabled={busy} onClick={() => void enable(true)}>I understand, turn it on</button>
        <button className="secondary-button" disabled={busy} onClick={() => setWarning(false)}>Cancel</button>
      </div>
    </div>}
    {status?.enabled && !status.needsConfirmation && <p className="settings-copy">{`On for rekordbox ${status.confirmedVersion ?? ""}.`}</p>}
    {status?.enabled && status.needsConfirmation && <div className="warning-panel">
      <p className="settings-copy">{`rekordbox is now ${version} (it was ${status.confirmedVersion ?? "unknown"} when you turned this on). Reading stays off until you confirm again.`}</p>
      <button className="secondary-button" disabled={busy} onClick={() => void enable(true)}>{`Confirm for ${version}`}</button>
    </div>}
    <label className="setting-row">
      <span><strong>Let openCrate write to rekordbox's library</strong><small>{!readingOn
        ? "Needs reading turned on first."
        : status?.writeTested === false
          ? `openCrate has not been tested writing to rekordbox ${version}, so this stays off. Reading still works.`
          : "Off by default. When on, tracks you drop on a rekordbox playlist go into it, and new imports go into rekordbox's Imported playlist, with no xml steps."}</small></span>
      <input type="checkbox" checked={status?.writeEnabled ?? false} disabled={busy || (!status?.writeEnabled && (!readingOn || !status?.writeTested))} onChange={toggleWrite} />
    </label>
    {writeWarning && <div className="warning-panel" role="region" aria-label="Before openCrate writes to rekordbox">
      <h4>Before openCrate writes to rekordbox</h4>
      <p className="settings-copy">This changes rekordbox's own library. A rekordbox update can change how that library is stored, and a write in a form rekordbox does not expect could lose playlists, cues or analysis.</p>
      <p className="settings-copy">To protect your library: openCrate writes only while rekordbox and its agent are closed, only to rekordbox versions it has been tested with, takes a backup just before every write (listed below, and any one can be put back), and reads the library back afterwards to check the change is there. It only adds: it never deletes a track or a playlist. If rekordbox changes version, this switch asks you again.</p>
      <div className="warning-actions">
        <button className="import-button" disabled={busy} onClick={() => void enableWrite(true)}>I understand, let openCrate write</button>
        <button className="secondary-button" disabled={busy} onClick={() => setWriteWarning(false)}>Cancel</button>
      </div>
    </div>}
    {status?.writeEnabled && !status.writeNeedsConfirmation && <p className="settings-copy">{`Writing is on for rekordbox ${status.writeConfirmedVersion ?? ""}.`}</p>}
    {status?.writeEnabled && status.writeNeedsConfirmation && <div className="warning-panel">
      <p className="settings-copy">{`rekordbox is now ${version} (it was ${status.writeConfirmedVersion ?? "unknown"} when you turned writing on). Writing stays off until you confirm again.`}</p>
      <button className="secondary-button" disabled={busy || !status.writeTested} onClick={() => void enableWrite(true)}>{`Confirm writing for ${version}`}</button>
    </div>}
    {error && <p role="alert" className="inline-error">{error}</p>}
    {note && <p role="status" className="settings-copy">{note}</p>}
    {backups.length > 0 && <>
      <h4>Backups of rekordbox's library</h4>
      <p className="settings-copy">The last 20 are kept, and every pinned one until you unpin it.</p>
      <ul className="folder-list" aria-label="rekordbox backups">{backups.map((backup) => <li key={backup.name} className="folder-row">
        <div className="folder-text">
          <strong>{`${REASONS[backup.reason] ?? backup.reason}, ${when(backup.takenMs)}`}{backup.pinned ? " (pinned)" : ""}</strong>
          <small>{`${backup.tracks} tracks, ${backup.playlists} playlists${backup.appVersion ? `, rekordbox ${backup.appVersion}` : ""}`}</small>
        </div>
        <button className="secondary-button" disabled={busy} aria-label={`${backup.pinned ? "Unpin" : "Pin"} backup from ${when(backup.takenMs)}`} onClick={() => void pin(backup)}>{backup.pinned ? "Unpin" : "Pin"}</button>
        <button className="secondary-button" disabled={busy} aria-label={`Restore backup from ${when(backup.takenMs)}`} onClick={() => setRestoring(backup)}>Restore...</button>
      </li>)}</ul>
      {restoring && <div className="warning-panel" role="region" aria-label="Restore this backup?">
        <h4>Restore this backup?</h4>
        <p className="settings-copy">{`rekordbox's library goes back to how it was on ${when(restoring.takenMs)}. The library you have now is saved as a backup first, so this can be undone. rekordbox must be closed.`}</p>
        <div className="warning-actions">
          <button className="import-button" disabled={busy} onClick={() => void restore(restoring)}>Restore</button>
          <button className="secondary-button" disabled={busy} onClick={() => setRestoring(null)}>Cancel</button>
        </div>
      </div>}
    </>}
  </>;
}
