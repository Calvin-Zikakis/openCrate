# Interface

The interface follows Spotify's layout and feel: dark, artwork first, calm. It does not use Spotify's colors, logo or fonts, and it avoids rekordbox's dense multi-panel look. Speed is part of the design: every rule here exists so nothing feels laggy ([decision 0004](../decisions/0004-interface-and-speed-budgets.md)).

## Layout

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
- Panels sit on a near-black background with small gaps and rounded corners, like Spotify's. The sidebar is collapsible to icons.

## Screens

| Screen | Shows | Main actions |
| --- | --- | --- |
| Tracks | A virtualized list: artwork, title and artist, release, version badge, BPM, key, time, format badge | Play, show in folder, sort, filter |
| Releases | A grid of covers with title and artist, sorted by recently added or name | Open a release, play it |
| Artists | A grid of artists, then their releases | Open, play |
| Release or artist page | A large cover and header, then the tracks | Play, play all |
| Playlists | rekordbox's folders and playlists in the sidebar, then a read-only track list | Play, show in folder, filter the library by playlist |
| Import | Three tabs: Needs review, Ready to file, Filed. Needs review is grouped by release | Accept, edit, always apply this mapping, reject |
| Sync | rekordbox status: the XML export file in use, when it was last read, counts, and entries whose files are gone | Choose the export file, re-read, send new tracks |
| Settings | Library roots, watch folders, the rekordbox XML export file, an experimental option to read rekordbox's database, appearance, tokens for providers | Folder and file pickers everywhere a path is needed |
| Player bar | Cover, title, artist, transport, a waveform with cue markers read from rekordbox, time, volume | Play, pause, seek by clicking the waveform |

Every place that asks for a folder or file opens the operating system's folder picker. Paths are never typed unless the owner chooses to.

## Visual language

| Token | Starting value | Notes |
| --- | --- | --- |
| Background | `#0a0a0c` base, `#121216` panel, `#1a1a20` raised, `#22222a` hover | Four steps, no pure black panels |
| Text | `#f4f4f6` main, `#a1a1ad` secondary, `#80808c` faint | Check contrast when built: body text at least 4.5:1 |
| Accent | `#ff7a3d` warm orange, used for the play button, the current row and focus | One accent only |
| States | success `#3ddc97`, warning `#ffc857`, danger `#ff5d5d` | Used for the review queue and sync results |
| Radius | 6, 10 and 14 px | Cards 10, buttons 6, panels 14 |
| Spacing | 4 px steps | |
| Type | `"Segoe UI Variable Text", "Segoe UI", system-ui, -apple-system, "SF Pro Text", sans-serif` | The owner is on Windows. Tabular numbers for BPM and time |
| Motion | 120 to 180 ms, ease-out, transform and opacity only | Reduced-motion setting turns it off |

Artwork: embedded art is cut into two sizes once (a small one for rows, a larger one for grids and the player) and stored in a cache folder. A track with no art gets a quiet gradient chosen from a hash of its release, so a list never shows empty boxes.

Density: a comfortable row (56 px) and a compact row (40 px). The choice is remembered.

## Behavior rules

1. **Windowed data.** A list asks the backend for rows `start` to `start + n` of a sorted view, with the total known up front, so the scrollbar is exact. Only the visible rows plus a little overscan are rendered.
2. **Feedback within 100 ms.** A click shows an effect at once (a pressed state, a spinner after 100 ms, optimistic updates). Slow work continues in the background and reports progress.
3. **No layout thrash.** Row heights are fixed. Images have reserved sizes. Cover images decode off the main thread.
4. **Keyboard first.** Arrow keys move through lists, Enter plays, Space pauses, `/` focuses search, `Ctrl` or `Cmd` plus `K` opens a command palette, `Esc` backs out.
5. **Empty, offline and error states are designed.** An unplugged library drive shows a calm banner with the drive letter and keeps the library browsable. An empty review queue says so. Errors say what happened and what to try, never a stack trace.
6. **Accessible.** Visible focus rings, labels on icon buttons, contrast checked, motion reducible.
7. **One source of truth for data access.** The UI talks to a `LibraryApi` interface with two implementations: the real one over Tauri commands and a mock one with deterministic synthetic tracks, so the interface can be built and measured in a browser.

## Measuring it

- A script scrolls a 100,000-row list in a browser and records frame times. Target: 60 fps, no frame over 33 ms.
- A script types into search against the mock API and records time to first result. The backend side is measured by the database benchmark.
- On the real app: cold start to a usable list, time from keystroke to results, and time from pressing play to first audio, recorded on the owner's Windows desktop with the real library, as numbers only.
