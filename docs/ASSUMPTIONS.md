# Assumptions

Calls made on 2026-10-22 that the game plan did not settle. A1 to A30 were made while the owner was away; A31 to A47 build on the owner's answers and files. A28 to A32 and A43 are withdrawn and stay in the table so the numbering holds.
Every item is open until the owner confirms or changes it. "If wrong" says what to touch.
Add a new item here whenever a call is made that the plan does not cover.

## Naming and licensing

| ID | Assumption | Why | If wrong |
| --- | --- | --- | --- |
| A1 | The product is called openCrate, and crates use the prefix `oc-` (`oc-core-db`, `oc-organize`, and so on) | The folder is named openCrate. The prefix follows `rbl-*` and `ob-*` and avoids clashes with crates.io names such as `tag` and `player` | Rename in the `Cargo.toml` files, `package.json` and `src-tauri/tauri.conf.json` |
| A2 | License is SPDX `GPL-3.0-or-later` | The plan says "GPL-3.0". rbxport uses "or later", and the `LICENSE` text is the same either way | Change `license` in the workspace `Cargo.toml` and `package.json` to `GPL-3.0-only` |
| A3 | Bundle identifier is `com.opencrate.desktop` (a placeholder) | Tauri needs one, and it warns about identifiers that end in `.app` | Settle before the first release: it fixes the macOS data directory and the updater identity |

## Tooling

| ID | Assumption | Why | If wrong |
| --- | --- | --- | --- |
| A4 | npm, not pnpm | pnpm is not installed on this Mac and npm ships with Node. rbxport uses pnpm through `npx --yes pnpm@10.17.1` | Delete `package-lock.json`, run `pnpm import`, and set `packageManager` |
| A5 | Frontend unit tests run in the `node` environment with no DOM | Node 26 is installed, and rbxport has 15 DOM tests that fail under Node 26 (`localStorage`, React `act`) | Add jsdom tests once Node 24 is pinned |
| A6 | Rust is pinned to 1.99.0 with clippy and rustfmt, and `rust-version` is 1.89 | Both match rbxport, and 1.99.0 is already installed with those components | Edit `rust-toolchain.toml` and the workspace `rust-version` |
| A7 | Lints follow rbxport, trimmed: `unsafe_code` denied; `unwrap`, `expect`, `panic`, `dbg!` and `print_stdout` denied outside tests; clippy `pedantic` as warnings | They are the owner's own standard, and "nothing may panic across the IPC boundary" applies here too | Edit `[workspace.lints]` |
| A8 | Frontend majors mirror rbxport: React 19, Vite 7, TypeScript 5.9, TanStack Virtual 3 | They are known to work together on this machine | Bump deliberately, in their own change |
| A9 | Commits and pushes happen only when the owner asks or has just said to do them. The owner has now said to commit and push (A47) | rbxport's handoff says to ask before committing work the owner has not reviewed | Tell the agent to stop committing |
| A10 | The app icon will be a generated placeholder (planned; `src-tauri` does not exist yet) | Tauri will not build without icons | Replace `src-tauri/icons/` (run `npx tauri icon <1024px png>`) |

## Product

| ID | Assumption | Why | If wrong |
| --- | --- | --- | --- |
| A11 | The first milestone is a foundation, not a feature: workspace, shell, database, pure naming and identification logic, and a mock-data UI. The order of later milestones is in [BUILD-PLAN](BUILD-PLAN.md) | The plan lists no milestones | Reorder BUILD-PLAN |
| A12 | Default path template is `{folder_artist}/{release}/{track:02} - {artist} - {title}`, where `title` includes the version and `folder_artist` is the remixer, else the album artist, else the artist | The plan fixes Artist/Release/Track and the remixer rule but not file names. The owner's tidier files already use `NN - Artist - Title` ([research 002](research/002-owner-data-shapes.md)). Putting the artist in the file name keeps a remix identifiable when it sits in the remixer's folder | Change `DEFAULT_TEMPLATE` in `oc-organize` |
| A13 | When comparing claims, no version means "Original Mix". "Original Mix" and "Extended Mix" stay different | The plan says version is part of identity. Bandcamp titles carry no version while Beatport titles say "Original Mix" | Edit `oc-identify`'s `normalize_version` |
| A14 | Auto-file needs artist, title and version each agreed by at least two different signal sources, with no source disagreeing | The plan says "at least two independent signals" and "start strict" | Edit `Policy` in `oc-identify` |
| A15 | Names are sanitized to the strictest rules of both OSes on every host, with each path component capped at 100 characters and 240 bytes, and the whole path kept under 260 characters on Windows unless long paths are available | The plan requires identical behavior on macOS and Windows, and the owner is on Windows | Edit the limits in `oc-organize` |
| A16 | Dark theme only, a warm orange accent, the system font stack. The layout and feel follow Spotify, not its colors, logo or fonts | "Think Spotify UI", without borrowing a brand | Edit the tokens in `src/styles/tokens.css` |
| A17 | The player bar, waveform and cue markers in the UI are a visual shell with mock data. No audio plays | The player is milestone work (decode, peaks, cue read-back) | Not applicable until the player milestone |

## Data and storage

| ID | Assumption | Why | If wrong |
| --- | --- | --- | --- |
| A18 | `oc-core-db` uses plain `bundled` SQLite for now, and moves to the SQLCipher build when `oc-rekordbox` links `rbl-db` | Avoids compiling OpenSSL from source before it is needed. Cargo builds one SQLite per binary | Change the `rusqlite` feature in the workspace `Cargo.toml` |
| A19 | The app database always lives in the local app data directory. Only library roots may be remote | SQLite's WAL mode does not work over a network filesystem | Not recommended |
| A20 | Development uses synthetic data and temp directories only. `OPENCRATE_DEMO=<count>` opens a separate demo database seeded with fake tracks | The owner's NAS, Navidrome and rekordbox libraries must never be touched by tests or experiments | Remove the demo mode |
| A21 | `bpm` and `musical_key` columns exist but stay empty until the `rekordbox` read-back fills them | The plan says rekordbox analyzes and the app reads it back | Drop the columns in a migration |
| A22 | No dependency on `~/repositories/rbxport-beat-analysis` yet | A path dependency on a sibling folder breaks every other clone. Vendoring, a submodule or a git dependency is a decision for the `rekordbox` milestone | Decide in HANDOFF "Open decisions" |
| A23 | beets and MiniMediaScanner are references, not dependencies, and the MiniMediaScanner datasets are not used | See [research note 001](research/001-tag-lookup-options.md) and [decision 0002](decisions/0002-metadata-lookup-strategy.md) | Reopen decision 0002 |
| A24 | Download folders (Bandcamp, Soulseek) are on the owner's Windows desktop, and the library root is a folder on a local drive (G: in the rekordbox export) | The owner says the music lives only on the desktop | Tell me where downloads land. If they are on another machine, `ingest` has to poll |
| A25 | The track counts for 500 GB assume six-minute tracks, with AIFF at 16-bit 44.1 kHz and MP3 at 320 kbps | An estimate to size the database question, not a measurement of the owner's library | Replace with the real count from the first index |
| A26 | The NAS copy mirrors the library, so a move or delete in the library is a move or delete there (QSync syncs both ways, A33), and Navidrome may lose history when files move. This is a risk, not a fact | Not verified. It is why nothing moves in the existing library and replaced files are archived | Read Navidrome's Persistent IDs page before any import writes into a folder it indexes |
| A27 | Research relied on pages fetched on 2026-10-22 (vendor documentation and repository pages). Limits and prices can change | Nothing was tested against the live services | Re-check before building a provider |
| A28 | Withdrawn 2026-10-22: the owner chose to skip AudioMuse-AI and Navidrome integration | See [decision 0005](decisions/0005-integrations-through-navidrome.md) | Reopen decision 0005 |
| A29 | Withdrawn 2026-10-22 (see A28) | | |
| A30 | Withdrawn 2026-10-22 (see A28) | | |

## Platform, rekordbox and the owner's data

| ID | Assumption | Why | If wrong |
| --- | --- | --- | --- |
| A31 | Withdrawn 2026-10-22: playlist export targets were dropped | See [decision 0008](decisions/0008-rekordbox-readback-xml-first.md) | |
| A32 | Withdrawn 2026-10-22: playlist files were dropped (see A31) | | |
| A33 | The NAS copy is kept in step by QNAP QSync, which syncs both ways. The desktop owns the files. openCrate never writes to the NAS | The owner's answer. The owner says the detail is not important | If a NAS-side change ever comes back to the desktop, revisit [decision 0003](decisions/0003-local-library-synced-copy.md) |
| A34 | rekordbox owns the playlists. openCrate shows them read-only and writes no playlist files | The owner's answer ([decision 0008](decisions/0008-rekordbox-readback-xml-first.md)) | Not applicable |
| A35 | AIFF output uses the `.aif` extension | 1,845 `.aif` against 50 `.aiff` in the playlists | Change the extension in `oc-convert` |
| A36 | The sanitizer replaces forbidden characters with `_` and never deletes them | The owner's library has fused names where a `/` was deleted | Edit the replacement rule in `oc-organize` |
| A37 | Windows is the primary target. Development happens on a Mac, so Windows is exercised by CI and by the owner's checks ([decision 0007](decisions/0007-windows-first-develop-on-mac.md)) | The owner's music and rekordbox are on a Windows desktop | Not applicable |
| A38 | The repository is private. It lives on GitHub under the owner's account, created when the owner asked for a push, and CI uses GitHub Actions with Windows and macOS runners | The owner asked for a private repository and then for a push | Move the remote, or change the workflow files |
| A39 | The rekordbox XML writer reproduces the `Location` spelling seen in the owner's export: `file://localhost/<drive>:/`, forward slashes, and lowercase `%xx` for space, `'`, `&`, `[`, `]`, `%` and non-ASCII bytes | [Research 002](research/002-owner-data-shapes.md) | Re-measure with another export |
| A40 | The existing library is never changed. Only new imports follow the new naming rules | The plan says "index in place", and the library mixes naming styles | Not applicable |
| A41 | Importers read bytes, not only text. A line or file name that is not valid UTF-8 is kept as raw bytes, reported, and never crashes an import | Two lines in the owner's playlists could not be read as text | Not applicable |
| A42 | The owner's real exports (the rekordbox XML and the playlists) are only read in place, never copied into the repository. Tests use synthetic fixtures | Private data stays out of the repository, even a private one | Not applicable |
| A43 | Withdrawn 2026-10-22: playlist files and rekordbox's m3u8 import are out of scope | See [decision 0008](decisions/0008-rekordbox-readback-xml-first.md) | |
| A44 | The experimental `master.db` option is compiled into every build, so the SQLCipher build applies from milestone 5. If that costs too much (build time, installer size, Windows build requirements), the option moves behind a Cargo feature | The owner wants it as a setting, and a setting needs the code to be present | Move `rbl-db` behind a feature in `oc-rekordbox` |
| A45 | The experimental reader only ever opens a copy of `master.db`, never the live file, and treats it as read-only | The rule is never to touch the owner's rekordbox library. rbxport's own tooling copies first as well | Not applicable |
| A46 | The XML export file is chosen by the owner in settings and re-read on request. openCrate notices when its modified time changes but never exports on the owner's behalf | rekordbox cannot be asked from outside, so exporting stays a manual step in rekordbox | Add a reminder in the interface |
| A47 | Commits are authored with the owner's configured git identity (the GitHub no-reply address) | The owner told me to commit and push | Rewrite history before the repository is shared |

## Questions for the owner

1. Where do Bandcamp and Soulseek downloads land on the desktop?
2. Should every newly filed track go into rekordbox automatically (by an XML you import), or only when you ask?
3. Which ID3 version should the `tag` crate write (2.3 or 2.4)? rekordbox needs testing.
4. Final product name and bundle identifier?
5. For the rbxport crates, which the experimental `master.db` option needs: vendor them, add a submodule, or depend on a git URL?
6. Light theme later? Is the orange accent right?
7. pnpm or npm?
8. Can your Windows install enable long paths, or should the 260-character limit be treated as hard?
