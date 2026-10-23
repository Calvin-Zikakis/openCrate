# Research 008: Windowed library queries on 100,000 synthetic tracks

Date: 2026-10-22. Synthetic data only. Apple M3 Pro, 36 GiB, macOS 27.0.1, Rust
1.99.0, bundled SQLite via rusqlite 0.40. Cargo `dev` profile (dependencies at
opt-level 2, the crate itself unoptimized). A temporary directory, WAL mode,
`synchronous = FULL`; one read-only connection; 30 timed runs after 3 warm-ups.

Command, from the repository root:

```sh
cargo test -p oc-core-db --locked -- --ignored --nocapture benchmark_100k
```

Dataset: `synthetic::seed(100_000)` gives 100,000 tracks, 10,000 releases, 240
artists, one file per track (AIFF, MP3, FLAC in turn), FTS5 rows for tracks,
releases and artists. Seeding took 1.8 s; the database file is 79 MiB. Windows
are 100 rows and include the total.

## Results (p95, ms)

| Case | Direct query | Cached reader, cold | Cached reader, warm |
| --- | --- | --- | --- |
| Tracks, first window, any sort | 0.2 | 9.9 | 0.2 |
| Tracks, deep window (start 99,900), Recent / Title / Artist / BPM | 17.8 / 29.2 / 13.6 / 86.6 | 9.9 | 0.2 |
| Tracks, Title, AIFF only (33,334 rows), first / deep | 15.9 / 63.6 | 18.9 | 0.2 |
| Search "night" (26,669 rows) | 36.2 | 10.6 | 0.2 |
| Search "ni" (26,669 rows) | 34.2 | 10.5 | 0.2 |
| Search "night drive" (12,500 rows) | 24.3 | 8.0 | 0.2 |
| Search "studio 001" (690 rows) | 2.0 | 1.0 | 0.2 |
| Search "night", FLAC, Title (8,335 rows) | 25.3 | 12.6 | 0.2 |
| Releases (10,000), Title, deep / search "nightfall" | 1.9 / 0.9 | 1.1 / 0.7 | 0.2 |
| Artists (240), Title / search "ensemble" | 0.6 / 0.4 | 0.7 / 0.4 | 0.6 / 0.4 |

"Direct" is `read_page`: a count plus an offset walk on every call. "Cached
reader" is `LibraryReader`: the first call of a query builds the ordered id list
(cold); later windows of that query read only their rows (warm).

## What it shows and what it does not

- The direct query was fine for first windows but cost up to 87 ms for deep
  windows and 36 ms for a broad search, because every window repeated the filter.
  The cached reader makes the first window of any query at most about 19 ms and
  scrolling about 0.2 ms, against the 50 ms search and 60 fps budgets of
  [decision 0004](../decisions/0004-interface-and-speed-budgets.md).
- A covering index on `file(track_id, lifecycle, container)` cut the format
  filter from 26 ms to 16 ms.
- Not measured: IPC, rendering, a release build, Windows, a cold OS cache,
  concurrent writes during reads, memory beyond the id lists (about 800 KB per
  cached query), or a real library's text and distribution. These are
  start-of-slice numbers on one machine and not guarantees.
