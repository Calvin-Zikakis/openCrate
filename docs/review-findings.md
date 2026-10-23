# Review findings still open

Two advisory plan reviews were written on 2026-10-22: the GPT review (findings R1-R13) and the Opus review (findings
1-17). Both files were removed from the working tree on 2026-10-23 once this register held what is still open; the
full text is in git history at commit `3f1395b` (`plan_review_gpt6.md`, `plan_review_opus.md`). Recommendations here
are not accepted decisions; the Game Plan and decision records are.

Status words: done, partly, open, superseded. Milestone numbers follow [BUILD-PLAN](BUILD-PLAN.md).

## Findings

| Finding | Status | What is left, and the check that closes it |
| --- | --- | --- |
| R1, Opus 10: file moves and database changes need a recovery protocol | partly | Built: `oc-publish` (operation record, hard-link no-clobber publication, staging outside the synced folder, flushes, fault injection, recovery at start). Left: release-level grouping; retry with backoff on Windows transient locks (Defender, Qsync, rekordbox holding a file), tested on Windows; ENOSPC during every write |
| R2, Opus 1-2: upgrades and path fixes need a rekordbox write mode | superseded | Replaced by the [rekordbox sync design](design/rekordbox-sync.md) (database route opt-in behind a warning, backups, restore). Its closure checks still apply: a failed path update, an unavailable archive, restart recovery and a shifted-timing alternate master, on synthetic or disposable libraries |
| R3, R7, Opus 5, Opus 16 (offline twice): path identity and availability states | done | Contract approved in [0013](decisions/0013-filesystem-identity-and-availability.md), lifecycle and presence in A74. When touching paths, keep fixtures for punctuation, accents, case-distinct names on a case-sensitive volume, case aliases on Windows, non-UTF-8 paths, percent escapes (`#` and `+` left raw by rekordbox), UNC paths and root remapping |
| R4, Opus 3: writing XML is not confirmation | open | Model offered (written into an XML) versus confirmed (seen in a later read-back at the same path); freeze on offered. Write one stable import file. Tests: XML never imported, imported twice, partial import, stale read-back, track already present |
| Opus 6: test the XML writer against rekordbox early | open | Generated files with awkward names (space, `&`, `'`, `[`, `#`, `+`, `,`, accents, a long path) imported into the laptop's rekordbox after a backup; try lowercase and uppercase percent spellings; import twice with a changed cue to learn whether rekordbox updates existing entries. rbxport's `xml_export.rs` spelling differs from rekordbox's (uppercase hex, encoded colon), so it is not reused for `Location` |
| R5, Opus 7-8: unknown versions and independent signals | open | Unknown version stays distinct from Original Mix; define signal classes (tags and names from one download are one class); version or remixer disagreement forces review; the tenth-track rule versus A14; exact numbers only inside names, not the year field. Labels come from the owner's review decisions, kept in app data; an `evaluate` command prints counts only; keep a held-out part. Zero wrong in 200 bounds the error at about 1.5% (95%). Applies before any automatic filing |
| R6, Opus 9: download history and completeness | partly | Built: baseline when a folder is added (A94), stability rule (A92). Left: Bandcamp ZIP purchases (unpack into staging only, refuse escaping or oversized archives) and loose files at the top of a download folder; partial-download names (`.part`, `.crdownload`, `.tmp`); tests for restart while baselining, a late track and repeated "Import existing" |
| R8, Opus 13: database copies need a consistent snapshot | open | Copy `master.db` with its `-wal` while rekordbox is closed, `PRAGMA quick_check` the copy, show its age; WAL, crash and corrupt-copy fixtures; compare XML and database from the same known state (cue times, playlist order) |
| R9, Opus 11: conversion space and preservation | partly | Built: ffmpeg conversion (A98), files already in the target format kept, verification before filing. Left: per-volume peak space budget including expanded PCM; hi-res policy (some older Pioneer players stop at 48 kHz); whether AAC is kept as is; `source_quality` kept once the original is converted; FFmpeg and LAME licence review if ffmpeg ships; the symphonia-plus-LAME alternative (rbxport's `rbl-audio`) if the sidecar is dropped |
| Opus 12: MP3 cue offsets between decoders | open | Synthetic MP3s with and without LAME/Xing headers, a click at a known sample; match rekordbox's gapless handling; one owner check by setting a cue on the click. Matters now that the waveform shows rekordbox cues, and before any upgrade carries cues across formats |
| R10, Opus 4, Opus 14: build and distribution | open | Supported OS versions and CPUs; test the declared Rust MSRV or set it to the pinned toolchain; signing, notarization and the updater key; licence notices and source delivery; installer upgrade and uninstall keep data; one CI run with `rbl-db` linked (vendored OpenSSL needs Strawberry Perl and a short target directory on Windows) before SQLCipher enters every build; rustls for any HTTP client |
| R11: UI budgets need end-to-end measurement | partly | Database benchmark exists ([research 008](research/008-library-query-benchmark.md)). Left: keystroke-to-painted-result in the real Windows app, under import load, with reference hardware and percentiles |
| R12: provider caching and retries | partly | Built: MusicBrainz on request ([0014](decisions/0014-musicbrainz-lookup-on-request.md)), spacing, 503 retries, response cap. Left: miss versus failure, cache expiry and refresh, persisted backoff with `Retry-After`, tokens in the OS keychain and never in logs or snapshots, consent for outgoing fingerprints, AcoustID terms for the intended use, late results as suggestions only |
| R13: milestones are not sized | open | Give each slice an observable demo, an hour estimate, dependencies and a defer or split condition |
| Opus 15: no component runs the pipeline | partly | Import runs from `src-tauri/src/import.rs`; move it into a crate testable without Tauri before automation, cancellation or concurrency |
| Opus 16: data model gaps | open | Bulky rebuildable data (provider responses, fingerprints) in a separate cache database; copied BPM and key go stale when the mirror changes (join at read time or rebuild together); write source quality and chosen release IDs into the files and the backup list |
| Opus 17: template disambiguator | open | Two releases with the same title by the same artist share a folder; add a suffix (year or catalog number) when the folder already holds another release. The version grammar half is done (A88) |

## Smaller items, not rechecked since the review

- Tauri capabilities and asset-protocol scope limited to what the app needs.
- The archive grows without limit: show its size.
- Store one canonical musical key and convert for display; record rekordbox's `Tonality` format.
- A denser row height for DJs used to rekordbox.
- Faint text `#80808c` is under 4.5:1 on raised and hover surfaces.
- Import state: a path back to review when filing finds a clash, and a clean-up rule for rejected items' staging files.
