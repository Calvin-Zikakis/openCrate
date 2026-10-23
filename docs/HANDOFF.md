# Handoff for coding agents

State as of 2026-10-22: planning is complete, the owner's latest decisions are applied to the docs, and there is no application code. The repository is private on GitHub (Calvin-Zikakis/openCrate). The next step is milestone 0, the foundation and Windows CI. This file is for an AI coding agent picking up openCrate work. Read it before changing anything, then read the newest entry in [LOG.md](LOG.md).

## 1. What this repository is

openCrate (a working name, see assumption A1) is a free GPL-3.0 desktop app in Rust, Tauri 2 and React for macOS and Windows. Bandcamp and Soulseek downloads go in, a clean Artist/Release library comes out, it sends new tracks to rekordbox through XML, and a player shows rekordbox's cues on a waveform. The interface follows Spotify's layout and feel, and nothing may feel slow.

The owner's reference setup is a Windows desktop. The music, rekordbox and the library folder (on drive G:, about 9,000 tracks in rekordbox) are all on it. QNAP QSync mirrors the library folder to a NAS in both directions, with the desktop owning the files, and the owner's Navidrome serves the NAS copy to a phone. openCrate works on the desktop folder only, never writes to the NAS, never talks to QSync and never calls Navidrome ([decision 0003](decisions/0003-local-library-synced-copy.md)). The owner says the sync detail is not important.

Decisions made on 2026-10-22, all by the owner:

- rekordbox owns the playlists, cues and grids. openCrate shows playlists read-only and writes no playlist files ([decision 0008](decisions/0008-rekordbox-readback-xml-first.md), accepted).
- The rekordbox read-back defaults to the XML export. An experimental setting, off by default, reads `master.db` through rbxport's `rbl-db`, on a copy only (decision 0008, assumptions A44 and A45).
- Playlists for Navidrome, and any Navidrome or AudioMuse-AI integration, are dropped. Decisions [0005](decisions/0005-integrations-through-navidrome.md) and [0006](decisions/0006-playlists-m3u8-and-rekordbox.md) are withdrawn and kept only as records.
- The workspace has nine crates plus the Tauri app. `oc-playlist` is dropped.
- The repository is private on GitHub and CI is GitHub Actions on Windows and macOS (A38). The owner told the agent to commit and push (A47).

Windows is the primary target and the Mac is for development only ([decision 0007](decisions/0007-windows-first-develop-on-mac.md)).

## 2. Read these first

| Document | Why |
| --- | --- |
| [DJ Library Manager — Game Plan.md](<../DJ Library Manager — Game Plan.md>) | Every decision, the architecture, the identification rules, the speed budgets |
| [BUILD-PLAN.md](BUILD-PLAN.md) | Milestones 0 to 9, acceptance checks, test strategy, risks, what is out of version 1, and the questions for the review |
| [design/interface.md](design/interface.md) | Screens, layout, visual tokens, behavior rules, how speed is measured |
| [design/data-model.md](design/data-model.md) | The database schema, conventions and invariants |
| [design/import-pipeline.md](design/import-pipeline.md) | Stages, import states, where files live, failure handling |
| [ASSUMPTIONS.md](ASSUMPTIONS.md) | Calls made without the owner (A1 to A47, with A28 to A32 and A43 withdrawn), and the questions waiting for an answer |
| [decisions/](decisions/) | Records 0001 to 0008: SQLite, metadata lookup, local library with a synced copy, interface and speed, Navidrome integration (withdrawn), playlists as m3u8 (withdrawn), Windows first, rekordbox read-back from XML first (accepted) |
| [research/001](research/001-tag-lookup-options.md) | The evidence behind decision 0002: rate limits, beets, MiniMediaScanner, Lexicon, MusicBee |
| [research/002](research/002-owner-data-shapes.md) | Aggregate facts about the owner's rekordbox XML export, and about the old playlist files (now historical) |
| [README.md](README.md) | An index of everything in `docs/`, and a reading order for a reviewer |
| [LOG.md](LOG.md) | Dated progress log, newest first |

## 3. Rules

The rules are in [AGENTS.md](../AGENTS.md). In short: document as you go, use the `documenter` agent, no emojis in docs, never touch the owner's real library, keep private material out, ask before committing or pushing unless the owner has just said to, and measure before claiming speed.

## 4. Environment (checked 2026-10-22)

Two machines are involved.

- **Dev Mac:** where code is written and where the agent runs. It holds no music.
- **The owner's Windows desktop:** where the library, rekordbox and the NAS sync live. Nothing can be run there from the dev Mac, so every check that needs real data or Windows behavior is run by the owner and reports aggregate numbers only ([decision 0007](decisions/0007-windows-first-develop-on-mac.md)).
- **Real exports:** the owner's rekordbox XML export and a copy of the old playlist folder sit in `~/Downloads` on the dev Mac, outside the repository. The XML is what the read-back is built against. The playlist folder is no longer used by the plan. Read them in place if a check needs them. Never copy them, or any name from them, into the repository ([research 002](research/002-owner-data-shapes.md) holds the aggregate facts).

The dev Mac:

- **Machine:** Apple Silicon Mac, macOS, zsh.
- **Rust:** rustup's default toolchain is `stable-aarch64-apple-darwin` (cargo 1.99.1). `rust-toolchain.toml` pins 1.99.0, which is installed with clippy and rustfmt. One terminal session did not have `~/.cargo/bin` on its PATH, so use `~/.cargo/bin/cargo` if `cargo` is not found.
- **Node:** 26.12.0 with npm 11.20.0. pnpm and corepack are not installed. rbxport pins Node 24 because DOM tests fail under Node 26.
- **ffmpeg:** 9.0.2 is installed.
- **Network:** crates.io, the npm registry and gnu.org were reachable from the agent terminal.
- **Reference code:** `~/repositories/rbxport-beat-analysis` holds the `rbl-*` crates the `rekordbox` crate will reuse, plus the lint posture and Tauri configuration this repository follows.

## 5. Current state

Planning is complete. There is no application code, nothing has been compiled, and no speed number exists yet.

Files that exist:

| Item | Status |
| --- | --- |
| `README.md` (root) and [docs/README.md](README.md) | done |
| [Game Plan](<../DJ Library Manager — Game Plan.md>) | done, the source of truth for decisions |
| `docs/BUILD-PLAN.md`, `docs/design/` (interface, data-model, import-pipeline) | done, proposed for review |
| `docs/decisions/` 0001 to 0008 | done (0004 and 0008 accepted; 0005 and 0006 withdrawn; the rest proposed) |
| `docs/research/` 001 and 002 | done |
| `docs/ASSUMPTIONS.md` (A1 to A47), `docs/LOG.md`, this file | done |
| `AGENTS.md`, `.github/agents/documenter.agent.md` | done |
| `LICENSE` (GPL-3.0 text from gnu.org) | done |
| Root `Cargo.toml`, `rust-toolchain.toml`, `clippy.toml`, `.gitignore` | done, not yet built |
| Crates under `crates/`, `src-tauri/`, frontend | not started |
| GitHub Actions workflows | not started (milestone 0) |

The root `Cargo.toml` lists `src-tauri` and `crates/*` as workspace members and neither exists yet, so `cargo` commands in this folder fail until milestone 0 creates the workspace members. That is expected, not a bug.

The repository is private on GitHub (Calvin-Zikakis/openCrate), and CI is GitHub Actions on Windows and macOS runners (assumption A38). The owner has told the agent to commit and push (A47). Do not change the repository's visibility. For any later commit or push, ask first unless the owner has just said to.

## 6. The foundation (milestone 0)

Nothing is built. The scope and acceptance checks are in [BUILD-PLAN](BUILD-PLAN.md), milestone 0. The workspace has nine crates plus the Tauri app: `oc-core-db`, `oc-ingest`, `oc-identify`, `oc-dedupe`, `oc-convert`, `oc-tag`, `oc-organize`, `oc-rekordbox` and `oc-player`. There is no `oc-playlist` crate ([decision 0008](decisions/0008-rekordbox-readback-xml-first.md)). Crates that milestone 0 does not need are empty with a purpose line. The schema is in [data-model](design/data-model.md), the screens in [interface](design/interface.md).

## 7. Milestone order

The order, scope and acceptance checks are in [BUILD-PLAN](BUILD-PLAN.md). The order itself is open for the review. In one line: 0 foundation and Windows CI, 1 index in place and library views, 2 rekordbox read-back from XML (playlists read-only), 3 import pipeline (dry run), 4 convert, tag and file, 5 rekordbox export and the experimental `master.db` option, 6 player, 7 online lookups, 8 dedupe and upgrades, 9 installers, onboarding and user docs.

## 8. Open questions

The list is the "Questions for the owner" at the bottom of [ASSUMPTIONS.md](ASSUMPTIONS.md), and "Questions for the review" in [BUILD-PLAN](BUILD-PLAN.md). The owner's questions, as of 2026-10-22:

| Question | Where | Blocks |
| --- | --- | --- |
| Where do Bandcamp and Soulseek downloads land on the desktop? | ASSUMPTIONS question 1, A24 | Watch folders, milestone 3 |
| Should every newly filed track go into rekordbox automatically, or only when asked? | ASSUMPTIONS question 2 | Milestone 5 |
| ID3 version to write, 2.3 or 2.4 | ASSUMPTIONS question 3 | Milestone 4, after a test in rekordbox |
| Final product name and bundle identifier | ASSUMPTIONS question 4, A1, A3 | The first release |
| How to reuse rbxport's `rbl-*` crates, which the experimental `master.db` option needs: vendor, submodule or git dependency | ASSUMPTIONS question 5, A22 | Milestone 5 |
| Light theme later, and is the orange accent right | ASSUMPTIONS question 6, A16 | Interface tokens |
| pnpm or npm | ASSUMPTIONS question 7, A4 | The frontend setup in milestone 0 |
| Can the Windows install enable long paths | ASSUMPTIONS question 8, A15 | How hard the 260-character limit is |

Also open: whether the free GitHub Actions minutes cover a Windows and a macOS job (BUILD-PLAN review question 4).

## 9. Where to start

Begin milestone 0, in the order BUILD-PLAN lists its scope: the Cargo workspace, then `oc-core-db` with its benchmark, `oc-organize`, `oc-identify` and `oc-ingest`, then the interface shell on mock data, then CI (GitHub Actions, Windows and macOS). Keep Windows-style path fixtures (`G:\...`, trailing dots, reserved names such as `CON`) in the tests so they run on macOS too. Use synthetic data and temp folders only. After each step add a LOG entry and call the `documenter` agent.
