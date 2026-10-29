# oc-publish

Moving a staged file into the library so that a crash at any moment loses nothing and duplicates nothing. Every
import goes through it.

## The protocol

For each file:

1. The operation is recorded as `Planned`, with the source, the destination and the source's size and SHA-256.
2. Before the one change on disk, the intent is recorded as `Publishing`.
3. The staged file is hard-linked to the destination. A hard link fails if the destination exists, so publication
   can never overwrite a file, and it happens in one atomic step.
4. The database change that makes the file part of the library records the operation as `Recorded` in the same
   transaction.
5. Only then is the staged name removed (`Cleaned`).

Recovery runs the same code: each step looks at what is on disk and at the recorded phase, so a restart finishes
the job whatever the crash point. A destination that holds anything other than this operation's file is never
overwritten; the operation is rejected and the staged file stays.

## Public API

| Item | Purpose |
| --- | --- |
| `plan(id, source, destination) -> Operation` | Record what will happen, with the source's identity |
| `Executor::new(store, faults)`, `run(operation)`, `recover_all()` | Carry operations forward, fresh or after a crash |
| `Store` | Where operations are recorded; `oc-core-db` implements it on SQLite, `MemoryStore` is for tests |
| `stage_copy(source, staged, faults) -> Identity` | Copy a file onto the destination's volume under a `.part` name, flush it, check it against the source, then rename it |
| `sweep_partials(folder)` | Remove `.part` copies a crash left behind |
| `link_no_clobber(source, destination)` | The hard link that refuses an existing destination |
| `identity_of(path)` | What "the same file" means: its size and a SHA-256 of its content |
| `Faults`, `FaultPoint`, `NoFaults` | Crash injection: a test picks where to "lose power" and the run stops there |

## Limits

- The staging folder must be on the destination's volume, since a hard link cannot cross volumes, and outside any
  synced folder, so a sync tool never sees a half-written or duplicate name. The desktop app stages beside the
  music folder.
- Hard links need a file system that has them: NTFS, APFS and ext4 do; exFAT and some network shares do not. There
  publication fails with `Error::LinksUnsupported` rather than falling back to something that could overwrite a
  file.

## Validation

From the workspace root:

```sh
cargo test -p oc-publish --locked
cargo clippy -p oc-publish --all-targets --locked -- -D warnings
```

The tests crash a run at every fault point and check that recovery ends with exactly one copy of the file and an
untouched destination when one was already there.
