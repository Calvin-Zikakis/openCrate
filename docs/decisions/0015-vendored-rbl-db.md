# 0015: Read rekordbox's database with the owner's vendored rbl-db

Status: accepted 2026-10-23, following the owner's answer ("Do we trust that project? ... If so, bring it in") and the
agreed [rekordbox sync design](../design/rekordbox-sync.md).

## Decision

- `rbl-db`, `rbl-core` and `rbl-anlz` from the owner's rbxport fork are vendored under `vendor/rbxport` at a recorded
  commit, as their own excluded workspace, built unchanged (see `vendor/rbxport/VENDORED.md`). Builds stay offline
  and reproducible.
- `oc-rekordbox` depends on `rbl-db`. Its SQLite feature (`bundled-sqlcipher-vendored-openssl`) is unified into every
  build, so the app's own database now runs on SQLCipher's bundled SQLite without a key (A44's "in every build").
- openCrate reads only copies of `master.db` taken while rekordbox is closed. Writes to rekordbox come in a later
  plan, through `rbl-db`'s `Writer`, after its write paths are read in full.

## Consequences, measured

- SQLCipher's bundled SQLite is 3.51.3 (the plain bundle is 3.53.2). On 3.51.3 a correlated `WHERE EXISTS (...)`
  filter combined with `OFFSET` returned the wrong rows (releases 91-85 instead of 5-1 at offset 95). The five paged
  filters in `oc-core-db` now use `IN (SELECT ...)`; `every_page_is_a_slice_of_the_whole_list` guards it. New paged
  queries must not use a correlated `EXISTS`.
- That rewrite slowed the cold first window of Releases from 1.1 to 7.0 ms and Artists from 0.7 to 4.6 ms (p95,
  research 008 command, dev profile); warm windows stay at 0.2 ms. All within decision 0004's budgets.
- A clean build adds OpenSSL and SQLCipher compilation (needs Perl, present on macOS; Windows needs Strawberry Perl
  and a short target directory, per rbxport's CI). Not yet measured on Windows.
- Move to a newer `libsqlite3-sys` when its SQLCipher bundle is on SQLite 3.52 or later, then re-run the test above.
