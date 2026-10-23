# Research 010: Reading the laptop's rekordbox library through a backup copy

Date: 2026-10-23. Apple M3 Pro, macOS 27, rekordbox 7.2.19.0342 installed on the development laptop (the owner allows
testing there; the main library on the Windows desktop was not touched). Rust 1.99.0, release profile.

## Command

With rekordbox and its agent closed (`ps -axo comm | grep -ic rekordbox` gave 0), the laptop's
`~/Library/Pioneer/rekordbox` folder and the agent's `options.json` were first copied to the session scratchpad (319 MB;
`master.db` SHA-256 identical to the original). Then:

```sh
cargo test -p oc-rekordbox --release -- --ignored --nocapture live_installed_read
```

The test finds the library from the agent's `options.json` (`rbl_db::detect`), takes a snapshot into a temporary
folder (copy `master.db` and its `-wal`, fold the log in, `quick_check`, schema probe), reads the copy with
`read_snapshot`, and prints counts only.

## Results

| Measure | Value |
| --- | --- |
| `djmdProperty.DBVersion` | 6000 |
| Tracks, rbl-db count / openCrate read | 503 / 503 |
| Playlist nodes, rbl-db count / openCrate tree | 96 / 96 (12 folders, 84 playlists) |
| Playlist entries | 603, none pointing at a missing track |
| Tracks with My Tags / rated / analysed (BPM set) | 0 / 186 / 467 |
| Cues and loops | 1,945 |
| Snapshot (copy and check) / total with the read | 144 ms / 196 ms |

Afterwards the live `master.db` had the same SHA-256 as before, and no `-wal`, `-shm` or temporary file was left beside
it.

## What it shows and what it does not

- The reader agrees with rbl-db's own counts on a real rekordbox 7.2.19 library, including the playlist tree, and every
  playlist entry resolves.
- The laptop library has no My Tags, so My Tags were checked only on synthetic fixtures. The owner's desktop library uses
  them; check counts there before relying on them.
- Values were not compared field by field with an XML export of the same library; that comparison (cue times, ratings,
  colours) is still to do before the database read replaces the XML route by default.
- Windows was not run. rekordbox's version is read only on macOS for now.
