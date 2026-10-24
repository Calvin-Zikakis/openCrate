import { useCallback, useEffect, useRef, useState } from "react";
import { Artwork } from "./Artwork";
import { Icon } from "./Icon";
import { Scope, Waveform } from "./Waveform";
import { carriesTracks, draggedItems } from "../library/drag";
import type { LibraryApi, LibraryItem, LibraryQuery, PlayerState, TrackCue, WaveBands, WaveDetail } from "../library/types";

/** Ask the player where it is this often while a track is loaded. */
const POLL_MS = 250;
/** Previous restarts the track instead when it has played longer than this. */
const RESTART_MS = 3000;
/** At the end of a track, unplayable ones after it are skipped, but not forever. */
const MAX_SKIPS = 10;
const VOLUME_KEY = "opencrate.volume";
const message = (error: unknown) => (error instanceof Error ? error.message : "Something went wrong.");

export function clock(ms: number): string {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

function loadVolume(): number {
  try {
    const saved = Number(localStorage.getItem(VOLUME_KEY));
    if (localStorage.getItem(VOLUME_KEY) !== null && Number.isFinite(saved)) return Math.min(1, Math.max(0, saved));
  } catch { /* storage unavailable: full volume */ }
  return 1;
}

/** A track to play, with the list it came from so next and previous can follow it. */
export interface PlayRequest {
  item: LibraryItem;
  index: number | null;
  query: LibraryQuery;
  nonce: number;
}

interface Playing {
  item: LibraryItem;
  index: number | null;
  query: LibraryQuery;
}

interface Props {
  api: LibraryApi;
  selected: LibraryItem | null;
  /** Where the selected track sits in the list on screen. */
  selectedIndex: number | null;
  query: LibraryQuery;
  request: PlayRequest | null;
  /** Tracks to play next, before the list the track came from carries on. */
  queue: LibraryItem[];
  onQueueChange: (queue: LibraryItem[]) => void;
  analysis?: React.ReactNode;
}

const typing = (target: EventTarget | null) =>
  target instanceof HTMLElement && (target.isContentEditable || target.tagName === "TEXTAREA" || target.tagName === "SELECT" ||
    (target.tagName === "INPUT" && !["range", "checkbox", "radio", "button"].includes((target as HTMLInputElement).type)));

/** The bar at the bottom: what is playing, the transport, a seek bar and the volume. */
export function PlayerBar({ api, selected, selectedIndex, query, request, queue, onQueueChange, analysis }: Props) {
  const admin = api.admin;
  const [playing, setPlaying] = useState<Playing | null>(null);
  const [state, setState] = useState<PlayerState | null>(null);
  const [error, setError] = useState("");
  const [dragMs, setDragMs] = useState<number | null>(null);
  const [volume, setVolume] = useState(loadVolume);
  const advancing = useRef(false);
  /** Counts seeks, so a status asked for before the latest one is not shown. */
  const seeks = useRef(0);
  const [bands, setBands] = useState<WaveBands | null>(null);
  const [detail, setDetail] = useState<WaveDetail | null>(null);
  const [cues, setCues] = useState<TrackCue[]>([]);
  const [queueOpen, setQueueOpen] = useState(false);
  const [dropping, setDropping] = useState(false);

  /** Load and play a track. A track that cannot be played stops the player, so the previous
   * one does not carry on under the new title. Resolves to whether it plays. */
  const start = useCallback(async (item: LibraryItem, index: number | null, from: LibraryQuery): Promise<boolean> => {
    if (!admin) return false;
    setError("");
    setPlaying({ item, index, query: from });
    setState(null);
    try {
      await admin.playTrack(item.id);
      await admin.player("volume", volume);
      return true;
    } catch (reason) {
      setError(message(reason));
      setState(await admin.player("stop").catch(() => null));
      return false;
    }
  }, [admin, volume]);

  /** The track `step` places away in the list the track was played from, or null past either end. */
  const neighbour = useCallback(async (from: Playing, step: number): Promise<{ item: LibraryItem; index: number } | null> => {
    if (from.index === null) return null;
    const index = from.index + step;
    if (index < 0) return null;
    const controller = new AbortController();
    const page = await api.read(from.query, index, 1, controller.signal);
    const item = page.items[0];
    return item && index < page.total ? { item, index } : null;
  }, [api]);

  /** Play the next track: the queue's first, else the next in the list. At the end of a track
   * (`skipping`), unplayable ones are passed over. A queued track keeps the list's place, so the
   * list carries on after the queue from where it was left. */
  const next = useCallback(async (skipping = false) => {
    let rest = queue;
    for (let tried = 0; rest.length > 0 && tried < (skipping ? MAX_SKIPS : 1); tried += 1) {
      const [head, ...after] = rest;
      rest = after;
      onQueueChange(after);
      if (head && await start(head, playing?.index ?? null, playing?.query ?? query)) return;
    }
    if (queue.length > 0 && !skipping) return;
    if (!playing) return;
    for (let step = 1; step <= (skipping ? MAX_SKIPS : 1); step += 1) {
      const found = await neighbour(playing, step).catch(() => null);
      if (!found) return;
      if (await start(found.item, found.index, playing.query)) return;
    }
  }, [neighbour, onQueueChange, playing, query, queue, start]);

  const previous = useCallback(async () => {
    if (!admin || !playing) return;
    if ((state?.positionMs ?? 0) > RESTART_MS) {
      setState(await admin.player("seek", 0).catch(() => state));
      return;
    }
    const found = await neighbour(playing, -1).catch(() => null);
    if (found) await start(found.item, found.index, playing.query);
    else setState(await admin.player("seek", 0).catch(() => state));
  }, [admin, neighbour, playing, start, state]);

  const toggle = useCallback(async () => {
    if (!admin) return;
    if (!playing || (selected && selected.kind === "tracks" && selected.id !== playing.item.id && !state?.playing)) {
      if (selected?.kind === "tracks") await start(selected, selectedIndex, query);
      else if (!playing && queue.length > 0) await next();
      return;
    }
    setState(await admin.player(state?.playing ? "pause" : "play").catch((reason: unknown) => { setError(message(reason)); return state; }));
  }, [admin, next, playing, query, queue.length, selected, selectedIndex, start, state]);

  // Each request plays once, however often this effect runs (StrictMode, a volume change).
  const handled = useRef<number | null>(null);
  useEffect(() => {
    if (!request || handled.current === request.nonce) return;
    handled.current = request.nonce;
    void start(request.item, request.index, request.query);
  }, [request, start]);

  // The waveforms and rekordbox's cues for the track that is loaded.
  const loadedId = playing?.item.id ?? null;
  useEffect(() => {
    setBands(null);
    setDetail(null);
    setCues([]);
    if (!admin || loadedId === null) return;
    let stopped = false;
    admin.waveform(loadedId).then((found) => { if (!stopped) setBands(found); }, () => undefined);
    admin.waveformDetail(loadedId).then((found) => { if (!stopped) setDetail(found); }, () => undefined);
    admin.trackAnalysis(loadedId).then((found) => { if (!stopped) setCues(found?.cues ?? []); }, () => undefined);
    return () => { stopped = true; };
  }, [admin, loadedId]);

  // Follow the player while a track is loaded; at the end, move on to the next one.
  useEffect(() => {
    if (!admin || !playing) return;
    let stopped = false;
    const tick = async () => {
      const asked = seeks.current;
      const now = await admin.player("status").catch(() => null);
      // A status for another track is from before the last load; it says nothing about this one.
      if (stopped || !now || now.trackId !== playing.item.id) return;
      // One asked for before a seek landed still has the old position.
      if (asked !== seeks.current) return;
      setState(now);
      if (now.error) setError(now.error);
      if (now.ended && !advancing.current) {
        advancing.current = true;
        await next(true);
        advancing.current = false;
      }
    };
    void tick();
    const timer = window.setInterval(() => void tick(), POLL_MS);
    return () => { stopped = true; window.clearInterval(timer); };
  }, [admin, playing, next]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.code !== "Space" || event.repeat || typing(event.target) || event.metaKey || event.ctrlKey || event.altKey) return;
      if (document.querySelector("[role=dialog]")) return;
      event.preventDefault();
      void toggle();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [toggle]);

  /** Go to `ms`. The bar shows the new spot until the player confirms it. */
  const seekTo = (ms: number) => {
    if (!admin) return;
    seeks.current += 1;
    const mine = seeks.current;
    setDragMs(ms);
    const settle = (next: PlayerState | null) => {
      if (seeks.current !== mine) return;
      if (next) setState(next);
      setDragMs(null);
      // A status asked for while the seek was on its way is old; the next one is not.
      seeks.current += 1;
    };
    void admin.player("seek", ms).then(settle, () => settle(null));
  };

  const changeVolume = (value: number) => {
    setVolume(value);
    try { localStorage.setItem(VOLUME_KEY, String(value)); } catch { /* applied, not remembered */ }
    if (playing) void admin?.player("volume", value).catch(() => undefined);
  };

  const shown = playing?.item ?? selected;
  const duration = state?.durationMs ?? (playing ? playing.item.duration * 1000 : 0);
  const position = dragMs ?? state?.positionMs ?? 0;
  const isPlaying = Boolean(state?.playing);
  const canPlay = Boolean(admin) && (Boolean(playing) || selected?.kind === "tracks" || queue.length > 0);

  return <footer className={`player-bar${dropping ? " is-drop-target" : ""}`} aria-label="Player"
    onDragOver={(event) => {
      if (!admin || !carriesTracks(event.dataTransfer)) return;
      event.preventDefault();
      event.dataTransfer.dropEffect = "copy";
      setDropping(true);
    }}
    onDragLeave={(event) => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDropping(false); }}
    onDrop={(event) => {
      setDropping(false);
      const items = draggedItems(event.dataTransfer);
      if (!admin || !items) return;
      event.preventDefault();
      // The first plays now; any others go to the front of the queue, in order.
      const [first, ...others] = items;
      if (others.length > 0) onQueueChange([...others, ...queue]);
      if (first) void start(first, null, query);
    }}>
    {admin && playing && detail && <Scope detail={detail} cues={cues} positionMs={position} playing={isPlaying && dragMs === null} />}
    <div className="now-playing">
      <Artwork color={shown?.color ?? 0} src={shown && shown.kind !== "artists" ? api.artworkUrl?.(shown.releaseId) ?? null : null} />
      <div><strong>{shown?.title ?? "Nothing selected"}</strong><span>{shown?.artist ?? "Select a track"}</span></div>
    </div>
    <div className="player-center">
      <div className="transport">
        <button className="icon-button previous" disabled={!playing} aria-label="Previous track" onClick={() => void previous()}><Icon name="next" size={17} /></button>
        <button className="play-button" disabled={!canPlay} aria-label={!admin ? "Playback not available in demo" : isPlaying ? "Pause" : "Play"} onClick={() => void toggle()}><Icon name={isPlaying ? "pause" : "play"} size={18} /></button>
        <button className="icon-button" disabled={!playing || (playing.index === null && queue.length === 0)} aria-label="Next track" onClick={() => void next(false)}><Icon name="next" size={17} /></button>
      </div>
      {admin && playing
        ? <div className="seek">
          <span>{clock(position)}</span>
          <Waveform bands={bands} cues={cues} positionMs={position} durationMs={duration}
            onSeek={(fraction, done) => { const ms = Math.round(fraction * duration); if (done) seekTo(ms); else setDragMs(ms); }}>
            <input type="range" aria-label="Position" min={0} max={Math.max(1, duration)} step={250} value={Math.min(position, Math.max(1, duration))}
              onChange={(event) => setDragMs(Number(event.target.value))}
              onKeyUp={() => { if (dragMs !== null) seekTo(dragMs); }} />
          </Waveform>
          <span>{clock(duration)}</span>
        </div>
        : <div className="waveform-preview" aria-hidden="true">{Array.from({ length: 64 }, (_, index) => <i key={index} style={{ height: 3 + ((index * 17) % 19) }} />)}</div>}
      {error && <p className="player-error" role="alert">{error}</p>}
    </div>
    {queueOpen && <section className="queue-panel" aria-label="Queue">
      <div className="queue-heading"><h2>Up next</h2>{queue.length > 0 && <button className="secondary-button" onClick={() => onQueueChange([])}>Clear</button>}</div>
      {queue.length === 0
        ? <p className="settings-copy">Nothing queued. Right-click a track and choose Add to queue, or drag tracks onto the player.</p>
        : <ol>{queue.map((item, index) => <li key={`${item.id}-${index}`}>
          <span className="truncate"><strong>{item.title}</strong><small>{item.artist}</small></span>
          <button className="icon-button" aria-label={`Remove ${item.title} from the queue`} onClick={() => onQueueChange(queue.filter((_, at) => at !== index))}><Icon name="close" size={14} /></button>
        </li>)}</ol>}
    </section>}
    <div className="player-meta">
      {analysis}
      {admin && <button className={`icon-button queue-button${queueOpen ? " is-open" : ""}`} aria-expanded={queueOpen}
        aria-label={`Queue, ${queue.length} ${queue.length === 1 ? "track" : "tracks"}`} onClick={() => setQueueOpen((open) => !open)}>
        <Icon name="queue" size={18} />{queue.length > 0 && <span className="queue-count" aria-hidden="true">{queue.length}</span>}
      </button>}
      {!admin && <span>Visual preview only</span>}
      <Icon name="volume" size={18} />
      {admin
        ? <input className="volume" type="range" aria-label="Volume" min={0} max={1} step={0.05} value={volume} onChange={(event) => changeVolume(Number(event.target.value))} />
        : <div className="volume-preview" />}
    </div>
  </footer>;
}
