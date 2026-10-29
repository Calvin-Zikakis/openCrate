# opencrate-desktop

The Tauri 2 host for the React interface. It began as the synthetic shell in
[decision 0010](../docs/decisions/0010-synthetic-interface-shell.md).
It is a workspace member alongside `oc-core-db` and `oc-rekordbox`.
The workspace's default members remain the two library crates; use
`--workspace` to include the desktop host.

## API and limits

This is a binary crate with no public Rust library API. It registers about 150 commands over
`oc_core_db::service::LibraryService`, all named `library_*` or `rekordbox_*` (see the `generate_handler!` list in
`src/main.rs`): reading pages of the library, scanning folders, imports and their lookups, the tag editor, crates,
playlists, Clean up (moved files, genres, titles, folder layout, album art, Reorganize sittings and putting them
back), and reading and writing rekordbox. Inbound messages are strict (`deny_unknown_fields`, windows of at most 200
rows, search text capped); outbound ones are camelCase like `src/library/types.ts`.

Without `OPENCRATE_DATA` or `OPENCRATE_DEMO`, `library_mode` answers `synthetic` and the UI keeps its in-memory
catalog. `OPENCRATE_DEMO=<tracks>` opens a seeded throwaway database; `OPENCRATE_DATA=<absolute folder>` opens a
persistent library there. Folder pickers are native dialogs opened from Rust (the page never supplies a path).
Commands that move or rewrite files share one hold, so two of them never change one file at once. Tauri's own check
refuses all these commands to any non-local origin; tests cover that.

- Only the dialog plugin is installed, used from Rust; there are no filesystem, network or shell plugins.
  The main window's capability has an empty permissions list. Network lookups (MusicBrainz, the Cover Art
  Archive, Discogs) are made by the host, not the page.
- The configured CSP permits local assets, workers, data images and Tauri's
  local IPC transport, not remote services. Inline styles are allowed for
  virtualization and generated artwork. This is not an operating-system sandbox.
- The window is labelled as a demo. Folder pickers, XML, import and playback are
  not connected. Only compact rows and reduced-motion preferences persist,
  using frontend `localStorage`.
- Desktop icons are generated from the original `icon.svg`. No mobile icons
  are retained.

## Build and validation

From the repository root, use Node 24/npm and Rust 1.99.0. The explicit npm
wrapper avoids the machine's global Node version:

```sh
npx -y -p node@24 -p npm@11 npm ci
npx -y -p node@24 -p npm@11 npm run build
npx -y -p node@24 -p npm@11 npm test
npx -y -p node@24 -p npm@11 npm run test:browser
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo check --workspace --locked
npx -y -p node@24 -p npm@11 npm run tauri -- build --debug --bundles app
```

Build the frontend before direct workspace Cargo checks: the host embeds `dist/`.
The final command is the macOS development bundle check, not a Windows build.
See [research 004](../docs/research/004-synthetic-interface-validation.md) for
the environment, Chromium setup, results and limits.

The parent reported 25 passing Rust tests across the two library crates and
zero desktop tests. Web tests passed separately. The arm64 macOS development
bundle was built; its executable stayed alive for three seconds before being
stopped. This is process liveness only, not native UI readiness or responsiveness.
Hosted workflows and Windows installation remain unverified.

For an owner preview:

```sh
npx -y -p node@24 -p npm@11 npm run tauri -- dev
```

This opens synthetic data only. It cannot import, play or connect a library.
Stop the app and development server after inspection.
