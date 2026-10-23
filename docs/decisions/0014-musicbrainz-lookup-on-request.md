# 0014: Look releases up in MusicBrainz only on request

Date: 2026-10-23.
Status: implemented and validated locally on macOS against the live service; Windows unverified.

## Context

Decision [0002](0002-metadata-lookup-strategy.md) puts local signals first and then one release
lookup per release. The owner asked for "Import all" to point out uncertain tags and offer to
fill them in. This is the first time openCrate uses the network.

## Decision

- Lookups happen only when the owner presses "Look up N releases on MusicBrainz" in the Import all
  review. Nothing is looked up in the background, and nothing is written until the owner imports.
- One release per request pair: a release search by the artist and album most of the files name,
  then the chosen release with its tracks, artist credits and labels. Candidates below a score of
  80 are not trusted; ties go to the one with as many tracks as there are files, then the earliest
  year (the original over reissues), then MusicBrainz's order.
- Files are matched to tracks by number and disc confirmed by title or length (within 4 s), then
  by title alone. A track goes to one file at most. Only values that differ are suggested.
- The client (`src-tauri/src/lookup.rs`) spaces requests 1.1 s apart, names itself in the
  User-Agent, retries a busy answer (503) twice with a pause, and caps an answer at 4 MB. It uses
  `ureq` with rustls; the parsing and matching live in `oc-lookup`, which has no network code.
- Accepted values travel with the import as per-file values and are applied over the file's own
  tags (after tidying); the preview marks them, and the import writes them into the staged copy.

## Alternatives rejected

- **Look up automatically when a release arrives.** Sends data without being asked and spends the
  rate limit on releases whose tags are already right.
- **reqwest.** In the lockfile through Tauri but not built on macOS; it would add an async stack
  for a handful of sequential requests.
- **Per-track recording search.** More requests for a worse answer; a release carries the album,
  album artist, year, label and track numbers at once.

## Evidence

`oc-lookup` tests use synthetic answers in the service's shape; `lookup.rs` tests compare and
space requests. The live check (`cargo test -p opencrate-desktop live_lookup -- --ignored`) asked
for a well-known public release: the original release was chosen, both files matched by number
and title, and the title, album artist, year and label were suggested. During development the
service once answered "busy", which is why busy answers are retried.
