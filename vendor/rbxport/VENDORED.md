# Vendored from rbxport

- Source: the owner's fork, `Calvin-Zikakis/rbxport-beat-analysis` (upstream `chrisle/rbxport`), local clone
  `~/repositories/rbxport-beat-analysis`.
- Commit: `1a0a8246` (2026-10-23).
- Crates: `rbl-core`, `rbl-anlz`, `rbl-db` (their `Cargo.toml`, `README.md` and `src/` only; tests and examples are
  not copied).
- Licence: GPL-2.0-or-later (see `LICENSE`), used here under GPL-3.0-or-later.
- Local changes: the workspace manifest lists only these crates, by name rather than a glob (a glob makes Dependabot
  abort), and drops the release profiles (the openCrate root profiles apply); `rbl-anlz`'s dev-dependencies are removed because they name crates not copied here;
  `rustfmt.toml` turns formatting off so the code stays as published. One source change, marked "openCrate
patch": `normalized()` in `rbl-db/src/write.rs` returns forward slashes on Windows, the form rekordbox uses for its
own paths there, so imported files are stored as `C:/...` and `track_id_at` finds them. openCrate also refuses to add
files to a library that stores backslashes (`oc-rekordbox` `check_path_form`). Carry the patch over on an update.
- The crates' READMEs are kept as published. Their links to rbxport's own `docs/` folder point at pages that were not
  copied; read those in the source repository.
- To update: copy the same files from a newer commit, record it here, and read every change to `rbl-db/src/write.rs`
  before use.
