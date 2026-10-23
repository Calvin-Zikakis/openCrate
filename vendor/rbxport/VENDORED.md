# Vendored from rbxport

- Source: the owner's fork, `Calvin-Zikakis/rbxport-beat-analysis` (upstream `chrisle/rbxport`), local clone
  `~/repositories/rbxport-beat-analysis`.
- Commit: `1a0a8246` (2026-10-23).
- Crates: `rbl-core`, `rbl-anlz`, `rbl-db` (their `Cargo.toml`, `README.md` and `src/` only; tests and examples are
  not copied).
- Licence: GPL-2.0-or-later (see `LICENSE`), used here under GPL-3.0-or-later.
- Local changes: the workspace manifest lists only these crates and drops the release profiles (the openCrate root
  profiles apply); `rbl-anlz`'s dev-dependencies are removed because they name crates not copied here;
  `rustfmt.toml` turns formatting off so the code stays as published. No source file is changed.
- To update: copy the same files from a newer commit, record it here, and read every change to `rbl-db/src/write.rs`
  before use.
