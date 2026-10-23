# Research 006: Owner-lock inheritance and the intermittent database test failure

Date: 2026-10-22. Synthetic data only (temporary directories), arm64 macOS,
pinned Rust 1.99.0. Resolves the instability recorded in
[research 005](005-read-only-xml-validation.md#limits). The chosen fix is
[decision 0012](../decisions/0012-explicit-owner-lock-release.md).

## Symptom

`distinguishes_clean_close_from_dropped_owner` failed with `AlreadyOpen` when it
reopened a directory straight after `close()`. `refuses_corrupt_database_and_invalid_lifecycle_state`
failed its `InvalidState` assertion after `drop`, which prints no error value.
The two failed together in the same run. The suite was otherwise passing.

## Narrowing the cause

Commands run from the repository root against the unfixed code. "Test binary" is
`target/debug/deps/oc_core_db-<hash>` from `cargo test -p oc-core-db --locked --lib --no-run`,
run directly in a shell loop of 100 runs per row.

| Command | Result |
| --- | --- |
| `cargo test -p oc-core-db --locked`, five runs | Two runs failed: 16 passed, 2 failed, the same two tests each time |
| Test binary, default threads | 24 of 100 runs failed |
| Test binary, `--test-threads=1` | 0 of 100 failed |
| Test binary, skipping both tests that spawn a child process | 0 of 100 failed |
| Test binary, skipping only `owner_lock_blocks_second_instance` | 6 of 100 failed |
| Test binary, skipping only `process_exit_rolls_back` | 5 of 100 failed |

Two tests start a child process through `run_child` (`Command::output`). Either
one alone is enough to produce failures; removing both removes them. Running
serially also removes them, which is why serial execution was rejected as a fix:
it hides the dependency instead of explaining it.

## Isolating the mechanism

A standalone program outside the repository, standard library only: four threads
each run 5,000 cycles of "take an exclusive `File::try_lock` on their own lock
file, release it, immediately take it again", counting refused acquisitions. In
the spawn variant a fifth thread spawns `/usr/bin/true` with `Command::output` in
a loop until the others finish. The first version panicked on the first refusal;
it was rewritten to count refusals and retry. Three rounds each:

| Release by | Child spawner | Refused acquisitions per 20,000 cycles | Children spawned |
| --- | --- | --- | --- |
| Dropping the file (close) | none | 0, 0, 0 | 0 |
| Dropping the file (close) | running | 64, 63, 49 | 126 to 130 |
| `File::unlock()`, then dropping | running | 0, 0, 0 | 129 to 132 |

A lock belongs to the open file description, not to one descriptor. A child
created by `fork` or `posix_spawn` holds a copy of every open descriptor until
it execs, even descriptors opened close-on-exec. While the copy exists, closing
the parent's descriptor leaves the lock held. `unlock()` acts on the shared
description, so it releases the lock for every copy. The refusals appear only
with a concurrent spawn and disappear with an explicit unlock, which establishes
the cause. The window length was not measured.

The `InvalidState` failure is inferred to be the same event, not observed: its
assertion discards the error. The inference rests on the shared pattern, the
matching skip results above and the failure disappearing after the fix.

## Regression tests and result

Two deterministic tests were added to `oc-core-db` before the fix. They use
`File::try_clone` to create the same kind of copy a child would hold, then
close or drop the owner and reopen.

| Check | Result |
| --- | --- |
| New tests on the unfixed code, three runs | Failed every time, `AlreadyOpen` on reopen |
| New tests after the fix | Passed |
| Test binary, default threads, final code | 0 of 300 runs failed (was 24 of 100) |
| `cargo test --workspace --locked`, five runs, final code | All passed: 20 database, 23 rekordbox, zero Tauri tests |
| `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo check --workspace --locked`, `git diff --check` | Passed |

## Limits

- Measured on one macOS machine. Linux and Windows were not run. `File::try_lock`
  uses `flock` on Unix; Windows uses a different lock call, and the same
  behavior with a duplicated handle is assumed, not measured (A66).
- Failure rates depend on machine load and are not stable figures.
- The application does not spawn processes today. The exposure is a release
  followed quickly by a reacquire in one process, or a failed open retried at
  once, while another thread spawns. It becomes real when conversion or tag
  tools spawn children.
- This does not change the lock's purpose: it excludes a second cooperating
  owner and does not stop non-cooperating SQLite writers.
