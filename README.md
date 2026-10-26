# openCrate

openCrate is a free DJ library manager in development for Windows and macOS.
The stack is Rust, Tauri 2 and React. Downloads from any source
will become a clean `Artist/Release` library, with new tracks sent to rekordbox
through XML. rekordbox remains the owner of playlists, cues and beat grids.
The planned read-back is XML-first, with an experimental copy-only `master.db`
reader later. Playlists stay read-only.

Windows is the primary target. The product plan indexes existing libraries in
place without renaming them. New imports, a review queue and a waveform player
are later work, not features of the current foundation.

## Status

The reduced foundation in [decision 0009](docs/decisions/0009-reduced-foundation-and-recovery.md)
and visual sub-slice in [0010](docs/decisions/0010-synthetic-interface-shell.md)
are built locally. The Cargo workspace contains two library crates and a Tauri host:

- [oc-core-db](crates/oc-core-db/README.md): lifecycle schema v1 (`app_state`
  only), cooperative ownership, WAL, clean/dirty detection, online snapshots and
  non-overwriting restore.
- [oc-rekordbox](crates/oc-rekordbox/README.md): lossless Location decoding to
  bytes, not file matching or XML import.
- [opencrate-desktop](src-tauri/README.md): minimal Tauri 2 host for the
  React/Vite synthetic shell, with no custom commands or database connection.

The demo has virtualized track/release/artist views, worker-based search/sort,
format filters, selection and card drill-down. Only appearance persists in
`localStorage`. Folder pickers, XML, importing and playback are not connected.
The player is a disabled preview, and the interface is explicitly labelled
synthetic. The UI does not access `oc-core-db`; there is no windowed SQLite API,
scanner, library schema, reader pool or application database writer worker.

A macOS arm64 development bundle was built, but its native check established
process liveness only, not UI readiness. Rust Windows/macOS CI and frontend
Linux/browser CI are configured, with an on-demand unsigned Windows NSIS build;
neither workflow has run remotely. Milestone 0 is not complete. The repository
remains private, and no real music or exports are used by this demo.

## Local validation

Use Node 24/npm and Rust 1.99.0. From the repository root, build the frontend
before workspace Cargo checks because Tauri embeds `dist/`:

```sh
npx -y -p node@24 -p npm@11 npm ci
npx -y -p node@24 -p npm@11 npm run build
npx -y -p node@24 -p npm@11 npm test
npx -y -p node@24 -p npm@11 npm run test:browser
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo check --workspace --locked
cargo build --workspace --locked
```

If Chromium is missing, use
`npx -y -p node@24 -p npm@11 npm exec -- playwright install chromium`.
The npm wrapper selects Node 24 even when a different global version is installed.
Without `--workspace`, Cargo defaults to the two library crates.

Validation is local, on macOS, with synthetic fixtures only; the latest counts are
under "Try it on your own music" below. See
[research 003](docs/research/003-reduced-foundation-validation.md) and
[004](docs/research/004-synthetic-interface-validation.md) for the earlier
foundation and browser checks, and [research 008](docs/research/008-library-query-benchmark.md)
for the database query benchmark. Browser-demo and benchmark timings are not
Windows, import-load or audio performance guarantees.

## Preview the shell

After installing dependencies, use the Node-24-wrapped `npm run dev` for a
browser preview, or start the desktop preview:

```sh
npx -y -p node@24 -p npm@11 npm run tauri -- dev
```

Started like that, it is demo-only: a synthetic catalog in memory, nothing on disk.
`/` focuses search; arrows and Home/End move list focus; Enter selects a track or
opens a card. Ctrl/Cmd+K and playback shortcuts are not implemented. Stop the app
and development server after inspection.

## Try it on your own music (provisional, unreviewed)

The Rust library can index a folder and read a rekordbox XML export. Both are
opt-in through an environment variable, and both are strictly read-only:

```sh
OPENCRATE_DATA=/absolute/path/to/an/empty/folder \
  npx -y -p node@24 -p npm@11 npm run tauri -- dev
```

Then, in the window: Settings, Add folder, Scan; and on the rekordbox screen,
Choose export. The folder you pick is scanned for audio files; their tags are read
and a catalog is built in the database inside the `OPENCRATE_DATA` folder. The
export is parsed and matched to your files; tempo and key appear on matched
tracks and playlists appear in the sidebar. If your export was made on another
machine or drive, the screen suggests the path prefix that makes entries match,
based on files that really exist, and applies it only when you press the button.

Select a track and press **Cues and grid** to see the hot cues, memory cues, loops and beat grid rekordbox has
for it (after reading an export), exactly as rekordbox wrote them.

In Settings, **Download folders** takes the folders where your downloads land, whatever their source; the Import screen then lists
what is in them and which releases look finished downloading. Each release has a **Preview** button that shows where every file would go in your music folder, from its tags,
and what would stop it. It is a dry run: nothing is copied or imported yet.

What it never does: write, rename or move a music file; write to rekordbox or its
database; access the network; touch any folder you did not pick. Removing a
folder from the library removes only its index entries. Use a **copy** of a small
folder and a **copy** of an export first, because this build has not been reviewed
by you, has never run on Windows, and was verified only on macOS with synthetic
files. `OPENCRATE_DEMO=<tracks>` opens a throwaway synthetic database instead. If
the folder you named cannot be opened, the window says why and shows the demo.

Latest local results: 374 Rust tests, 52 unit tests and 53 browser runs (47 in Chromium; the drag and player tests also in WebKit) pass,
with strict clippy. See [HANDOFF](docs/HANDOFF.md) for what is built, what is
not, and the next steps, and [LOG](docs/LOG.md) for the details and the limits
of every check (including what could not be observed: the native window and
dialogs).

## Where to read

- [docs/README.md](docs/README.md) indexes every document and gives a reading order for a reviewer.
- [DJ Library Manager — Game Plan.md](<DJ Library Manager — Game Plan.md>) holds the decisions.
- [docs/BUILD-PLAN.md](docs/BUILD-PLAN.md) holds the milestones.
- [docs/HANDOFF.md](docs/HANDOFF.md) is the starting point for a coding agent.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
