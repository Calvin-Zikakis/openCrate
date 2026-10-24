import { useEffect, useState } from "react";
import type { LibraryAdmin, LookupKey, LookupKeys as Keys } from "../library/types";

const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");

type Service = "discogs" | "acoustid";

const SERVICES: { id: Service; name: string; about: string; where: string }[] = [
  {
    id: "discogs", name: "Discogs personal token",
    about: "Lets Look up ask Discogs when MusicBrainz has nothing, and adds Discogs's style (such as Deep House) as the genre.",
    where: "On discogs.com: Settings, Developers, Generate new token.",
  },
  {
    id: "acoustid", name: "AcoustID application key",
    about: "For identifying a track by its sound when its tags say nothing. Saved now; fingerprint lookups come in a later version.",
    where: "On acoustid.org: sign in, then Applications, Register a new application.",
  },
];

/** Settings: the owner's own keys for lookup services. A saved key is shown only by its last
 * four characters. MusicBrainz needs no key; Bandcamp has no public API to hold one. */
export function LookupKeys({ admin, open }: { admin: LibraryAdmin; open: boolean }) {
  const [keys, setKeys] = useState<Keys | null>(null);
  const [drafts, setDrafts] = useState<Record<Service, string>>({ discogs: "", acoustid: "" });
  const [busy, setBusy] = useState<Service | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    if (open) admin.lookupKeys().then(setKeys, (reason: unknown) => setError(message(reason)));
  }, [admin, open]);

  const save = async (service: Service, value: string | null) => {
    setBusy(service);
    setError("");
    try {
      setKeys(await admin.setLookupKey(service, value));
      setDrafts((current) => ({ ...current, [service]: "" }));
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(null);
    }
  };
  const state = (key: LookupKey | undefined) => (key?.saved ? `Saved, ending in ${key.ending ?? ""}` : "Not set");

  return <>
    <h3>Online lookups</h3>
    <p className="settings-copy">Look up asks MusicBrainz, which needs no key. Add your own keys to use more services; they stay on this computer and are only sent to the service they belong to. Bandcamp has no public service to hold a key.</p>
    {SERVICES.map((service) => {
      const key = keys?.[service.id];
      return <div key={service.id} className="key-row">
        <label htmlFor={`key-${service.id}`}><strong>{service.name}</strong><small>{`${service.about} ${service.where}`}</small></label>
        <div className="key-controls">
          <input id={`key-${service.id}`} type="password" autoComplete="off" spellCheck={false} placeholder={state(key)}
            value={drafts[service.id]} onChange={(event) => setDrafts((current) => ({ ...current, [service.id]: event.target.value }))} />
          <button className="secondary-button" disabled={busy !== null || drafts[service.id].trim() === ""} onClick={() => void save(service.id, drafts[service.id])}>{busy === service.id ? "Checking..." : "Save"}</button>
          {key?.saved && <button className="link-button" disabled={busy !== null} aria-label={`Remove the ${service.name}`} onClick={() => void save(service.id, null)}>Remove</button>}
        </div>
        <small className="key-state">{state(key)}</small>
      </div>;
    })}
    {keys?.note && <p role="status" className="settings-copy">{keys.note}</p>}
    {error && <p role="alert" className="inline-error">{error}</p>}
  </>;
}
