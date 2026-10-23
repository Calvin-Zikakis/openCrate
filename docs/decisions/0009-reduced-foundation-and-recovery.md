# 0009: Reduced foundation and database recovery

Date: 2026-10-22.
Status: accepted for implementation under the owner's instruction to continue.

## Context

Both [Opus](../../plan_review_opus.md) and [GPT review](../../plan_review_gpt6.md)
identify an oversized foundation and unresolved mutation contracts. Their
recommendations disagree on later milestone ordering; neither is adopted wholesale.

## Decision

Start milestone 0 with the two needed Rust crates, not empty placeholders:
`oc-rekordbox` for a lossless Location decoder and `oc-core-db` for database
lifecycle. Add Windows/macOS Rust validation. The UI shell, installer, library
schema, reader pool and performance workload are subsequent foundation slices,
not claims made by this step.

The first database schema contains only lifecycle state. It does not bake in the
reviewed path, sync or import schema defects. Search normalization must never
define filesystem identity. The Location codec preserves bytes and does not
claim to resolve a file, fold case, normalize Unicode or remap roots.

Use a cooperative exclusive OS file lock for one database owner, an application
identifier and schema-version checks before changing an existing database, and
an explicit clean-shutdown marker. Dropping the handle without an orderly close
leaves the marker dirty; reopening checks integrity.

Use SQLite's online backup API rather than copying a live database or WAL.
Produce a checked, flushed snapshot before each upgrade of an existing schema.
The initial empty database needs no pre-creation backup. Migrations and their
version updates commit together. Restore creates a new file and never overwrites
the source or a damaged database; callers must select that restored file
explicitly. Snapshot retention and scheduled backups remain later work.

No live rekordbox writes, synced upgrades, media conversion policy changes,
filesystem publication, automatic identification or real-library experiments
are authorized by this decision. Their review findings remain gates for the
milestones that implement them. Existing playlist ownership and XML-first
read-back decisions are unchanged.

## Validation

Synthetic temporary databases cover ownership refusal, future versions,
exclusive ownership, read-only readers, clean and unclean reopen, failed and
interrupted migrations, consistent WAL snapshots, no-clobber restore and corrupt
input. Local checks are not Windows CI evidence or performance measurements.
