# opencrate-desktop

Minimal Tauri 2 host for the synthetic React shell in
[decision 0010](../docs/decisions/0010-synthetic-interface-shell.md).
It is a workspace member alongside `oc-core-db` and `oc-rekordbox`.
The workspace's default members remain the two library crates; use
`--workspace` to include the desktop host.

## API and limits

This is a binary crate with no public Rust library API. It registers two commands,
`library_mode` and `library_read`, over `oc_core_db::service::LibraryService`. Inbound
messages are strict (`deny_unknown_fields`, windows of at most 200 rows, search text
capped); outbound ones are camelCase like `src/library/types.ts`. Without
`OPENCRATE_DEMO=<tracks>` there is no database, `library_mode` answers `synthetic`, and
the UI keeps its in-memory Web Worker catalog, as before. With it, the host opens a
synthetic database in a temporary folder (removed at exit), seeds it, and the UI reads
through `library_read`. `OPENCRATE_DATA=<absolute folder>` instead opens a persistent
library there. Whenever a library is open, Settings can add folders and scan them:
`library_add_root` opens the native folder dialog from Rust (the page never supplies a
path), `library_scan` runs one scan at a time in the background, and the UI polls
`library_scan_status` (no event permissions). Folders are only read. The app opens no
default app-data folder yet (A86). Tauri's own check refuses all these commands to any
non-local origin; tests cover that. The dialog itself needs a real window and is untested.

- No filesystem, network or shell plugins are installed. The main window's
  capability has an empty permissions list.
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
