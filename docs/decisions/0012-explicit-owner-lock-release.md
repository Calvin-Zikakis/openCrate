# 0012: Release the owner lock explicitly

Date: 2026-10-22.
Status: implemented and validated locally on macOS; Windows and Linux unverified.

## Context

The owner lock from [0009](0009-reduced-foundation-and-recovery.md) is an
exclusive `File::try_lock` on `owner.lock`. Dropping the file closed its
descriptor and was assumed to release the lock. [Research 006](../research/006-owner-lock-inheritance.md)
shows it does not when another thread spawns a process at that moment: the child
holds a copy of the descriptor until it execs, the lock belongs to the shared
open file description, and the directory stays locked. Two database tests
failed intermittently (24 of 100 runs) because of this.

## Decision

Wrap the locked file in a private `OwnerLock` type whose `Drop` calls
`File::unlock` before the descriptor closes. `own_directory` returns that type,
so `Database::open`, its error paths and `restore` all release the lock the same
way. `Database` declares its connection before the lock, so the connection
closes first. A failed unlock is ignored, because drop cannot report it and the
operating system still releases the lock when the last copy closes.

The lock's lifetime is now the lifetime of the owning value, not whenever the
last copy of a descriptor closes.

## Alternatives rejected

- **Run tests serially, in CI or by default.** The failure disappears with
  `--test-threads=1`, but the defect is in the product's lock lifetime, so this
  only hides it. The handoff also forbade it.
- **Retry `Database::open` after `AlreadyOpen`.** The delay is arbitrary and
  cannot tell real contention from a transient copy. It changes what
  `AlreadyOpen` means to callers.
- **Serialize test spawns behind a lock.** Protects only these tests. Future
  production code that spawns conversion or tagging tools from another thread
  has the same exposure.
- **Prevent the child from inheriting the descriptor.** Rust already opens files
  close-on-exec, and the window exists before exec. Any spawner, including a
  dependency, can create it, so library code cannot rule it out.

## Consequences

- Two regression tests copy the descriptor with `File::try_clone` and check that
  close, drop and a bare `own_directory` value release the directory. One carries
  a single `#[expect(clippy::used_underscore_binding)]` because it reads the
  `_owner` field, which exists only to be dropped.
- Cooperating second owners are still refused while a `Database` lives. Nothing
  changes for non-cooperating writers.
- Windows uses a different lock call. The explicit-unlock behavior with a
  duplicated handle is assumed, not measured (A66). If Windows CI shows the new
  tests failing, investigate there; do not skip them.
- No change to schema, the public API or product decisions.
