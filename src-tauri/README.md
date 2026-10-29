# opencrate-desktop

The desktop app: the Tauri 2 host that opens the window, serves the React interface built into `dist/`, and does
the work the interface asks for through the library crates. It is a binary with no public Rust API.

The workspace's default members are the library crates only, so a plain `cargo test` or `cargo build` skips this
crate. Use `--workspace` to include it.

## Commands

`src/main.rs` registers about 150 commands, all named `library_*` or `rekordbox_*` (see its `generate_handler!`
list). Each module covers one area:

| Area | Modules |
| --- | --- |
| The library lists, folders and scans | `folders`, `ipc`, `artwork`, `export` |
| Watch folders and importing | `downloads`, `tagview`, `plan` (the dry run), `import`, `imports`, `lookup`, `layout` |
| Playing and waveforms | `player`, `waves` |
| Editing and crates | `editor`, `crates` |
| Clean up and the library check | `art_fix`, `moved`, `refile`, `reorganize`, `library_fix` |
| rekordbox | `sync` (XML export), `rekordbox_db` (reading its library, backups, restore), `rekordbox_send` (openCrate's XML file), `rekordbox_write` (writing), `rekordbox_merge` (Sync) |
| macOS menu | `menu` (Settings with Cmd+,) |

Messages from the page are strict (unknown fields are refused, at most 200 rows per window, search text capped);
answers are camelCase, matching `src/library/types.ts`.

## Where the library is

At start-up the app opens, in order of preference:

1. `OPENCRATE_DATA=<absolute folder>`: a library in that folder.
2. `OPENCRATE_DEMO=<tracks>`: a synthetic library of 1 to 100,000 tracks in a temporary folder, removed when the
   app quits.
3. Otherwise the app's local data folder (`%LOCALAPPDATA%\com.opencrate.desktop` on Windows,
   `~/Library/Application Support/com.opencrate.desktop` on macOS), never a roaming or synced one.

If the named folder cannot be opened, the window says why and shows the in-memory demo. An import that a crash cut
short is finished or set aside before anything else runs.

## Safety and limits

- Only the dialog plugin is installed, and it is used from Rust: folder and file pickers are native dialogs, and
  the page never supplies a path. There are no file system, network or shell plugins, and the window's capability
  grants no permissions. Tauri refuses the app's commands to any origin other than the local one.
- Network lookups (MusicBrainz, the Cover Art Archive, Discogs) are made by the host, never by the page, and only
  when the owner asks.
- Commands that move or rewrite files share one hold, so two of them never change one file at once.
- The content security policy allows local assets, workers, data images, the `artwork:` scheme and Tauri's local
  IPC, not remote services. Inline styles are allowed for the virtualised lists. This is not an operating-system
  sandbox.
- ffmpeg is started without a console window on Windows (see `oc-convert`).

## Build and validation

The setup and every check are in the [repository README](../README.md#development). Build the frontend first,
because the host embeds `dist/`. To check this crate on its own:

```sh
npx -y -p node@24 -p npm@11 npm run build
cargo test -p opencrate-desktop --locked
cargo clippy -p opencrate-desktop --all-targets --locked -- -D warnings
```

The tests use temporary folders, synthetic audio files and synthetic rekordbox libraries.
