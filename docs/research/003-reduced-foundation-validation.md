# 003: Reduced foundation validation

Date: 2026-10-22.

## Scope and provenance

This records results supplied by the execution and parent sessions, checked
against the current public APIs, manifests, schema, tests and workflow during
documentation reconciliation. The documentation pass did not rerun the commands.
The scope is [decision 0009](../decisions/0009-reduced-foundation-and-recovery.md):
database lifecycle, lossless Location decoding and Rust CI configuration.

Environment: arm64 macOS; Rust 1.99.0 (`b940084d7 2026-09-28`).
The pinned toolchain is 1.99.0 and workspace `rust-version` is 1.99.
Commands run from the workspace root. Use `~/.cargo/bin/cargo` when Cargo is
absent from PATH.

## Commands and results

| Command/check | Reported result |
| --- | --- |
| `~/.cargo/bin/cargo fmt --all` | Completed |
| `cargo fmt --all --check` | Passed, including after the final reader test |
| `cargo test --workspace --locked` | Final run: 25 passed, zero failed; 18 `oc-core-db`, seven `oc-rekordbox`; zero doctests |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed, including after the final reader test |
| `cargo build --workspace --locked` | Passed before the final test-only addition |
| `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked` | Passed before the final test-only addition |
| Ruby/Psych workflow parse and assertions | Passed; exact invocation was not supplied |
| `git check-ignore --no-index --stdin` | All six supplied synthetic paths were ignored; no files created |

The workflow check used [.github/workflows/rust.yml](../../.github/workflows/rust.yml)
as its input. It checked `[windows-latest, macos-latest]`, `contents: read` and
the exact format, clippy, test and build steps listed above. The workflow runs
on push, pull request and manual dispatch. Parsing and assertions do not run
jobs on GitHub or validate Windows behavior.

The ignore check covered the `.sqlite3` database, its `-wal`, `-shm` and
`-journal` sidecars, and the `schema-*.partial` and `restore-*.partial` snapshot
patterns. The exact input strings were not retained in this note.

## Final documentation verification

The parent reported these checks passed on 2026-10-22 after reconciliation.
They were not rerun by this documentation pass.

| Command/check | Dataset and result |
| --- | --- |
| `git diff --check` | Working-tree diff: clean |
| Python inline local-link checker | All tracked and nonignored untracked Markdown: 236 local targets in 29 files, zero missing |
| Emoji checker | `docs/`, root README/CLAUDE and crate READMEs: zero emoji in 24 files |

The local-link checker skipped external, mail and anchor-only links. It stripped
fragments and URL-decoded angle-bracket paths before checking local targets.
It did not validate fragment anchors or external destinations. Exact invocations
for the inline link and emoji checkers were not supplied.

The parent also confirmed that current status counts consistently report
25 total tests, 18 database and seven codec. Historical LOG counts were
intentionally left unchanged. These documentation checks add no runtime,
performance or hosted-CI evidence.

## Synthetic dataset

There is no track corpus or benchmark dataset in this run.

- [Database tests](../../crates/oc-core-db/src/tests.rs) create temporary
  databases with lifecycle state and small test-only tables/rows. They cover
  application/schema refusal, owner locks in-process and across a child process,
  read-only readers, clean versus dirty reopen, integrity checks, FTS5
  availability, committed WAL snapshots and no-clobber restore.
- Migration fixtures inject a test-only second migration, invalid SQL, an
  unavailable snapshot directory, and a child process exiting during an
  uncommitted migration-like transaction. Production ships only
  [schema v1](../../crates/oc-core-db/src/schema/001_lifecycle.sql), with
  `app_state` and no library tables.
- Recovery fixtures use corrupt, foreign, future-version and missing inputs.
  Creation/restore tests preserve orphan `-wal`, `-shm` and `-journal` files.
  The symlink/directory-alias test is Unix-only.
- `read_snapshot_stays_consistent_while_writer_commits` uses a synthetic table.
  A read transaction sees one row before and after the owner commits a second
  row. After ending the transaction, the reader sees two. This proves that test
  case, not a reader pool, worker implementation or throughput result.
- [Location tests](../../crates/oc-rekordbox/src/location.rs) use synthetic
  strings: drive, UNC and POSIX shapes; literal punctuation; mixed-case percent
  escapes; non-UTF-8 bytes; distinct Unicode forms and case; invalid escapes,
  NUL and unsupported prefixes. No XML file or actual filesystem match is used.

The 18 database tests include one child-process helper. That helper normally
returns without work and is also invoked by parent tests. The Unix-only test
will not run on Windows, so do not promise identical counts there.

## Failure correction and final additions

The first database run passed 15 of 16 tests. The restore-path assertion
compared a caller's temporary path with the canonical path returned by the
implementation. macOS aliases temporary directories, so the assertion was wrong.
It was changed to compare canonical paths; restore behavior was not changed to
hide a failure.

An orphan-journal preservation test was then added. The final independent
addition was the read-transaction test described above, with no production
implementation change. The last workspace test run passed all 25 tests.
Format checking and strict clippy were rerun successfully after that addition.
Earlier validation counts remain in the historical LOG entries.

## Limits and follow-up

- No Windows execution or hosted Windows/macOS workflow result exists.
- No UI/Tauri, installer, scanner, library schema/search API, matching, reader
  pool or background writer worker exists. FTS5 availability is not search.
- No private export, music, download folder, live rekordbox database or NAS was
  accessed. No real-data test, audio test or timing measurement was run.
- The caller chooses a private local app-data directory. The code does not
  detect network filesystems. `std::fs::File::try_lock` is cooperative ownership,
  not a lock against unrelated writers.
- Online backup and restore checks concern openCrate's own database, not
  acquisition of a consistent external `master.db` copy. Retention and
  scheduling remain later work. Process exit tests are not power-loss testing.
- Location bytes are not filesystem identity. Review R3 remains open, as do the
  mutation and product gates in [HANDOFF](../HANDOFF.md#5-unresolved-review-gates).
- No commit or push was made. Parent documentation checks passed as recorded
  above; no link or diff-whitespace checkpoint remains pending.
