# openCrate

openCrate is a free DJ library manager for Windows and macOS, written in Rust and Tauri 2 with a React interface. Bandcamp and Soulseek downloads go in, a clean `Artist/Release` library comes out, and new tracks go to rekordbox through XML. rekordbox stays the owner of playlists, cues and beat grids: openCrate reads them back from the rekordbox XML export (an experimental setting can read `master.db` on a copy) and shows playlists read-only. The interface follows Spotify's layout and feel: dark, artwork first, and fast. A player shows rekordbox's cues on a waveform.

Windows is the primary target. Existing libraries are indexed in place and never renamed. Only new imports follow the new rules, and confident matches file themselves while the rest wait in a review queue.

## Status

Planning is complete and there is no code yet. The owner's latest decisions are applied to the docs. The repository is private on GitHub, and CI will be GitHub Actions. The next step is milestone 0: the workspace of nine crates plus the Tauri app, the database, the interface shell and Windows CI. `cargo` fails until milestone 0 creates the workspace members.

## Where to read

- [docs/README.md](docs/README.md) indexes every document and gives a reading order for a reviewer.
- [DJ Library Manager — Game Plan.md](<DJ Library Manager — Game Plan.md>) holds the decisions.
- [docs/BUILD-PLAN.md](docs/BUILD-PLAN.md) holds the milestones.
- [docs/HANDOFF.md](docs/HANDOFF.md) is the starting point for a coding agent.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
