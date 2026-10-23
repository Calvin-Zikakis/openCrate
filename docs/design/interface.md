# Interface

The interface follows Spotify's layout and feel: dark, artwork first, calm. It does not use Spotify's colors, logo or fonts, and it avoids rekordbox's dense multi-panel look. Speed is part of the design: every rule here exists so nothing feels laggy ([decision 0004](../decisions/0004-interface-and-speed-budgets.md)).

## Built synthetic shell

The visual sub-slice is built locally under
[decision 0010](../decisions/0010-synthetic-interface-shell.md). It uses React 19,
Vite 7, TypeScript 5.9, TanStack Virtual 3 and plain CSS in
[`src/styles.css`](../../src/styles.css). It is not full milestone 0.

| Area | Current behavior |
| --- | --- |
| Navigation | Tracks, Releases, Artists, Import and rekordbox screens; sidebar becomes icons below 850 px and keeps explicit accessible names |
| Library | Virtualized track rows and release/artist cards; search, sort, track-format filters and selection; either card type opens its filtered tracks, with a back action |
| Keyboard | `/` focuses search outside editing/modal contexts; Up/Down move row focus, grids also support Left/Right, Home/End reach the ends; Enter selects a track or opens a card, not playback |
| Settings | Native HTML modal dialog, initial close-button focus, Escape dismissal and focus return; compact rows and reduced motion only |
| Persistence | Only appearance in `localStorage` under `opencrate.appearance.v1`; load/write failures are visible, and failed writes leave the change session-only |
| Player | Selected synthetic details and decorative waveform; transport disabled and labelled as a visual preview |
| Import/sync | Honest empty screens: filesystem pickers, imports and XML are not connected |
| Accessibility checks | Focus styles, skip link, named icon buttons, collapsed navigation names, modal behavior, body-token contrast and system reduced motion; not a full accessibility audit |

The app labels itself as a synthetic demo and says no files are connected.
Ctrl/Cmd+K, a command palette and actual playback shortcuts are not implemented.
There is no persisted navigation/selection, real folder setting, artwork cache,
audio decoding, XML connection or provider setting.

### Current data and native boundary

[`LibraryApi`](../../src/library/types.ts) exposes
`read(query, start, count, signal): Promise<LibraryPage>` and `dispose()`.
A page contains `items`, `total` and `start`. The only implemented adapter is
[`WorkerLibraryApi`](../../src/library/worker-api.ts). A dedicated Web Worker
owns the deterministic in-memory catalog, query filtering and sorting, including
cached sort indexes and the last filtered view. Replies contain at most 200
items; the whole catalog does not cross the worker boundary.

Requests have IDs. Abort rejects the pending promise, removes it from reply
tracking, and UI cleanup guards against outdated results. It does not send an
interrupt to the worker or stop already-running synchronous computation.
Worker startup/runtime failures are surfaced rather than silently falling back.
Fixed-height rows and overscan bound rendered content separately from the
200-item data window.

The minimal [Tauri host](../../src-tauri/README.md) has no filesystem, network
or shell plugins and an empty capability permissions list. Its configured CSP
permits local assets/workers, data images and Tauri's local IPC transport.
Inline styles are allowed for virtual positions and artwork. There is no custom
command, real `LibraryApi` adapter, windowed SQLite API or access to `oc-core-db`.
These restrictions do not establish a general OS sandbox.

## Product layout target

The diagram and full screen table below describe the planned product, not
features already connected in the demo. The built shell keeps the overall
sidebar/search/content/player arrangement.

```
+---------------------------------------------------------------------------------+
| [<] [>]   ( Search tracks, releases, artists                  )      [Settings]  |
+------------------+--------------------------------------------------------------+
| Library          |  Tracks                                  [Density] [Sort v]  |
|   Tracks         |  #   Title                  Release       BPM  Key  Time     |
|   Releases       |  1   [art] Title            Release       124  8A   6:12     |
|   Artists        |      Artist                                                  |
| Playlists        |  2   [art] Title (Mix)      Release       126  9A   7:03     |
|   Group          |      Artist                                                  |
|     Name         |  ...  only the visible rows exist in the page                |
| Import           |                                                              |
|   Needs review 3 |                                                              |
| Sync             |                                                              |
|   rekordbox      |                                                              |
+------------------+--------------------------------------------------------------+
| [art] Title - Artist   |<  >  >|  0:42 [ waveform with cue markers ] 6:12  vol  |
+---------------------------------------------------------------------------------+
```

- A fixed sidebar on the left, a main area that scrolls, a search field on top that works from any screen, and a player bar fixed at the bottom.
- Panels sit on a near-black background with small gaps and rounded corners. The current sidebar collapses responsively to icons; there is no manual collapse preference.

## Planned product screens

| Screen | Shows | Main actions |
| --- | --- | --- |
| Tracks | A virtualized list: artwork, title and artist, release, version badge, BPM, key, time, format badge | Play, show in folder, sort, filter |
| Releases | A grid of covers with title and artist, sorted by recently added or name | Open a release, play it |
| Artists | A grid of artists, then their releases | Open, play |
| Release or artist page | A large cover and header, then the tracks | Play, play all |
| Playlists | rekordbox's folders and playlists in the sidebar, then a read-only track list | Play, show in folder, filter the library by playlist |
| Import | Three tabs: Needs review, Ready to file, Filed. Needs review is grouped by release. With no download folder set, an empty state offers "Choose a download folder" | Accept, edit, always apply this mapping, reject |
| Sync | rekordbox status: the XML export file in use, when it was last read, counts, and entries whose files are gone | Choose the export file, re-read, send new tracks |
| Settings | Library roots, download folders (one or more, each with an on/off switch), the rekordbox XML export file, an experimental option to read rekordbox's database, appearance, tokens for providers | Folder and file pickers everywhere a path is needed. Adding, removing or switching off a download folder takes effect at once |
| Player bar | Cover, title, artist, transport, a waveform with cue markers read from rekordbox, time, volume | Play, pause, seek by clicking the waveform |

When path features are implemented, every place that asks for a folder or file
will use the operating system's picker. The demo has no connected picker.
Paths will not require typing unless the owner chooses to.

## Visual language

| Token | Value or target | Notes |
| --- | --- | --- |
| Background | `#0a0a0c` base, `#121216` panel, `#1a1a20` raised, `#22222a` hover | Four steps, no pure black panels |
| Text | `#f4f4f6` main, `#a1a1ad` secondary, `#9494a1` faint | Implemented; all twelve foreground/background combinations across the four surfaces pass at least 4.5:1 in Chromium tests. The old faint value `#80808c` is superseded |
| Accent | `#ff7a3d` warm orange, used for the play button, the current row and focus | One accent only |
| States | success `#3ddc97`, warning `#ffc857`, danger `#ff5d5d` | Product targets; the demo uses warning styling for errors, not a working review queue/sync result |
| Radius | Target family: 6, 10 and 14 px | Current component CSS varies; these are not shared implemented radius tokens |
| Spacing | 4 px steps | Design target, not a strict implemented token scale |
| Type | `"Segoe UI Variable Text", "Segoe UI", system-ui, -apple-system, "SF Pro Text", sans-serif` | The owner is on Windows. Tabular numbers for BPM and time |
| Motion | Current button/link transitions: 140 ms ease-out, opacity and transform | Either system reduced motion or the stored appearance preference disables motion |

Current artwork is generated CSS geometry and gradients, with synthetic color
indices; no embedded image is read or cached. Later, embedded art will be cut
into row and grid/player sizes once and cached locally. Hash-based fallback
artwork is a product target, not a current file-processing feature.

Density is implemented: comfortable rows are 56 px and compact rows are 40 px.
Only appearance choices are remembered.

## Product behavior rules and remaining work

1. **Windowed data.** The worker supplies totals and bounded windows now. The
   future backend must do the same without sending whole-library results.
   Only visible rows/cards plus overscan are rendered.
2. **Feedback within 100 ms.** This remains a product budget, not a blanket
   measured guarantee. Current pending queries show a loading status; timed
   spinners, progress reporting and optimistic mutations are not implemented.
3. **No layout thrash.** Current rows have fixed heights and artwork has reserved
   space. Real cover decoding/caching is later work.
4. **Keyboard first.** The current navigation/selection keys are listed above.
   Enter-to-play, Space-to-pause and Ctrl/Cmd+K command palette remain planned.
   Escape currently dismisses settings; it is not a general back command.
5. **Explicit states.** Empty results and worker/storage errors are implemented.
   Offline-root banners and review-queue states need real indexing/import;
   the demo does not implement availability semantics.
6. **Accessible.** Preserve labels, focus and reduced-motion behavior. The
   twelve token checks do not certify every composited color or all assistive
   technology behavior.
7. **One data interface.** `LibraryApi` has one worker adapter today. A real
   Tauri-command implementation is planned, after path/state and database
   contracts. Do not connect the synchronous lifecycle owner on the UI thread.

## Measuring it

Current evidence is in [research 004](../research/004-synthetic-interface-validation.md).
`npx -y -p node@24 -p npm@11 npm run test:browser` runs the production preview
against 100,000 deterministic tracks, 10,000 releases and 240 artists. The
latest arm64 Mac/headless Chromium 156.0.8078.4 run at 1440x960 recorded:

- 30 warm search samples: 34.1 ms p95, from the input event through completion
  of the current result plus two animation frames, not merely worker time.
- 120 scroll frames: 17.5 ms p95, 17.8 ms maximum, none over 33 ms; at most
  21 track DOM rows. These are animation-frame intervals, not a guaranteed
  presentation rate.
- Observed cold readiness: 83.2 ms including Playwright polling, not precise
  startup-to-paint.

The test records timings; it does not enforce a general latency guarantee.
The 60 fps target and full application budgets remain open. Native readiness,
database/IPC, Windows, import-load and audio measurements are absent. Later
owner-run real-library checks must report aggregate numbers only; no private
data belongs in fixtures, reports or screenshots.
