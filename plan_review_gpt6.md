# openCrate Plan Review

Date: 2026-10-22

Status: Complete. Recommendations below are not accepted changes to the plan.

## Scope

This is a design review of the current working tree, including the uncommitted
download-folder changes. There is no application code to test. The absent Cargo
workspace members are intentional scaffolding, not a defect. No private exports,
music folders, live rekordbox database, NAS or external library were accessed.

Severity: High means a risk to files, metadata or rekordbox references, or a
missing prerequisite for a promised feature. Medium means a material delivery,
correctness or validation gap. These are plan defects or omissions, not claims
about bugs in an implementation that does not exist yet.

## Findings

### R1. High: File operations and database transitions need a recovery protocol

Evidence: [Import state machine](docs/design/import-pipeline.md#L55),
"Stages", "State of an import item" and "Failure handling";
[docs/design/data-model.md](docs/design/data-model.md), `import_item` and `file`;
[Milestone 4](docs/BUILD-PLAN.md#L96), acceptance.

The pipeline moves a file in File, then records its new location in Record. Its
recovery description compares staging folders with import rows and removes
`.part` files. A same-volume rename followed by a crash before the database
commit leaves the completed file in the library while the database still points
at staging. Writing a transition before starting the next stage does not make a
filesystem operation and a SQLite transaction atomic. A retry can encounter a
name clash or fail to find the staged file instead of resuming deterministically.

Recommendation: specify a durable operation record before each external side
effect, including source, destination, expected identity, phase and recovery
action. Recovery must inspect both staging and destination, recognize an already
completed operation, and commit its file, import and audit changes together.
Define release-level behavior when several tracks or remixer destinations are
involved. Never overwrite a destination merely because the database says a move
was pending.

Close before milestone 4: fault-injection tests immediately before and after
rename, verification, staging removal and database commit. Each restart must
produce exactly one active file per accepted track, preserve recoverable audio,
and converge without duplicate filing. The operation contract belongs before the
filesystem helpers in milestone 0 are treated as complete.

There is a second concrete inconsistency: the cross-volume rule copies to a
temporary name beside the destination, inside the synced library. A `.part`
suffix alone does not prevent QSync from copying it. Use a publication staging
area on the destination volume but outside the synced subtree, or explicitly
limit the guarantee when that is impossible. Also specify an atomic no-clobber
publication primitive: checking that a path is absent and then using an ordinary
rename is not a portable guarantee against overwriting a concurrent arrival.

### R2. High: Synced upgrades depend on an unspecified rekordbox write mode

Evidence: [Game Plan](DJ%20Library%20Manager%20%E2%80%94%20Game%20Plan.md#L11),
"Decisions so far" and "rekordbox";
[Milestone 5](docs/BUILD-PLAN.md#L107) and [milestone 8](docs/BUILD-PLAN.md#L138);
[docs/design/import-pipeline.md](docs/design/import-pipeline.md), rules 4 and 7.

The plan promises opt-in path fixes and upgrades that preserve cues. Milestone 5
only introduces a copy-only experimental database reader, yet milestone 8 needs
a way to update rekordbox references. A reader operating on a copy cannot update
the live application's paths. The plan also needs to reconcile "the existing
library is never touched" with replacement and archival of an existing file.

Recommendation: make a deliberate scope decision. Either keep synced upgrades
in review for v1, or add a separately approved write-mode design covering supported
rekordbox versions, process/lock checks, consistent backup, rollback and all
related path references. Reading an experimental copy must not imply permission
to write the live database. Also require timing/alignment compatibility before
carrying cues and grids to another audio version; a better bitrate does not prove
identical cue timing.

Close before milestone 8: a concrete upgrade sequence tested on synthetic or
disposable libraries, including a failed path update, an unavailable archive,
restart recovery, and an alternate master with shifted timing. Keep XML-only
users' files frozen unless an independently verified safe replacement workflow
is explicitly adopted.

### R3. High: Search normalization must not define filesystem identity

Evidence: [Key conventions](docs/design/data-model.md#L8),
`file`, `rb_track` and invariant 3;
[docs/decisions/0003-local-library-synced-copy.md](docs/decisions/0003-local-library-synced-copy.md),
root remapping; [docs/ASSUMPTIONS.md](docs/ASSUMPTIONS.md), A15 and A41.

The convention for every `*_key` strips punctuation and diacritics and case-folds
text. Applying it to `rb_track.location_key` would collapse distinct paths such
as the synthetic `A-B.aif` and `AB.aif`. Conversely, a textual unique constraint
on `(root_id, rel_path)` alone does not model a case-insensitive Windows volume.
The relationship between raw non-UTF-8 paths and that uniqueness rule is also
undefined. A false file match attaches the wrong cues and playlist membership.

Recommendation: separate fuzzy metadata keys from lossless path identity. Define
Windows drive/UNC and macOS volume semantics, Unicode handling, raw path storage
and an explicit XML-prefix-to-root mapping. Merely changing `library_root.path`
cannot explain an old drive prefix still present in a rekordbox export. Preserve
existing names; apply the sanitizer only to newly generated destinations.

Close before schema/path work is accepted in milestone 0: fixtures for punctuation,
accents, case-distinct files on a case-sensitive volume, case aliases on Windows,
non-text paths, percent escapes, UNC paths and root remapping. Validate containment
through symlinks/junctions and when roots change, not only when a download folder
is first added. Measure Windows limits in the API's actual units and include the
absolute root, extension and temporary suffix; shortening the stem cannot fix
an already overlong root or parent directory.

### R4. High: Writing XML is not confirmation that rekordbox imported it

Evidence: [Sync state](docs/design/data-model.md#L50);
[docs/BUILD-PLAN.md](docs/BUILD-PLAN.md), milestone 5;
[docs/decisions/0008-rekordbox-readback-xml-first.md](docs/decisions/0008-rekordbox-readback-xml-first.md),
manual export/import and path matching.

`sync_state` has a TrackID and `synced_ms` but no distinction between an XML file
being generated and the owner successfully importing it. The freeze rule depends
on that distinction. The plan also does not say how existing indexed tracks that
are already in rekordbox acquire protection, or how a generated XML TrackID is
reconciled with the authoritative ID read back from rekordbox.

Recommendation: model pending export, exported/awaiting confirmation, confirmed
presence, and stale/error outcomes. Freeze a path once an exported artifact can
be imported, not only after confirmation. Protect existing read-back matches as
well. Reconcile authoritative source-scoped IDs and paths on read-back; do not
assume an ID selected by the writer becomes the live database's ID. Publish XML
atomically and retain enough information to make repeated exports idempotent.

Close before milestone 5: tests for an XML never imported, the same XML imported
twice, a partial import, a stale read-back and an already-present library track.
The owner check must cover identity and cue preservation, not just entry counts.

### R5. High: Unknown versions and correlated signals can produce confident mistakes

Evidence: [A13 and A14](docs/ASSUMPTIONS.md#L33);
[docs/decisions/0002-metadata-lookup-strategy.md](docs/decisions/0002-metadata-lookup-strategy.md),
scoring; [docs/BUILD-PLAN.md](docs/BUILD-PLAN.md), milestones 3 and 7.

A13 equates a missing version with "Original Mix". Missing metadata is not proof
that a track is the original rather than an unlabelled edit, remix or extended
mix. Tags and filenames can also derive from the same incorrect release source,
so two different parsers are not necessarily two independent signals. A weighted
score or a 9-of-10 release majority must not override a per-track version conflict.

Recommendation: keep unknown distinct from original; record signal provenance;
make explicit version/remixer and relevant numeric disagreements hard review
conditions. Define missing-signal behavior, exact-match fields, score weights,
threshold and runner-up margin before calling the policy implemented. Use
fingerprints as supporting evidence, not proof of version or timing equivalence.

Close before automatic filing is enabled: a separately held-out labelled set
with ambiguous mixes, multi-disc releases, incomplete albums and conflicting
signals. Report accepted count, wrong accepts and coverage, including results
by source. Zero errors in 200 independently sampled auto-accepted releases only
gives a one-sided 95% upper error bound of about 1.49%; if only a subset auto-files,
the bound is worse. A curated set has no such population guarantee. "No wrong
auto-file" is a useful regression gate, not a demonstrated universal property.

### R6. Medium: Download discovery needs persistent history and completeness rules

Evidence: [docs/design/import-pipeline.md](docs/design/import-pipeline.md),
"Download folders" and Stage; [docs/design/data-model.md](docs/design/data-model.md),
`watch_folder` and `import_item`; [A49-A50](docs/ASSUMPTIONS.md#L79).

Adding a folder must baseline its existing contents without staging them, while
future arrivals auto-stage and originals remain forever. The schema records
import attempts but no explicit observation/baseline inventory or durable source
identity. A timestamp alone cannot distinguish old downloads from new copies
that retain old mtimes. Restart, re-enable, remove/re-add and missed watcher events
can therefore cause skipped arrivals or repeated imports unless their semantics
are specified.

Recommendation: persist the baseline and observed source generations, reconcile
watch events with scans, and define pause/resume and remove/re-add behavior. Set a
release-level quiet window, recheck the source before and after staging, and
define handling of late tracks and interrupted copies. Size/mtime stability is
only a heuristic for a completed download. Keep "Import existing" explicit and
idempotent.

Close in milestone 3: restart during baselining, overflowed events, a new file
with preserved timestamps, a paused transfer that resumes during copying, a late
track and repeated "Import existing" must neither change originals nor silently
lose or multiply imports.

### R7. Medium: Offline, unmatched and missing are different states

Evidence: [docs/design/data-model.md](docs/design/data-model.md), `file.state`,
`rb_track.file_missing` and invariants 1-2;
[docs/design/interface.md](docs/design/interface.md), offline behavior;
[docs/BUILD-PLAN.md](docs/BUILD-PLAN.md), milestone 2 matching.

Library views select only `active` files, but `offline` is also a file state and
offline libraries must remain browsable. The plan needs to say whether root
availability is orthogonal to a file's lifecycle. An unmatched XML entry may be
outside configured roots, not indexed yet or offline; it is not necessarily
missing. Replacing mirror rows also needs a policy for cached BPM/key values when
a match disappears or the source changes, or old values can remain authoritative.

Recommendation: separate lifecycle from availability and match status; only call
a file missing after a successful check of a reachable location. Replace the
mirror and reconcile derived fields atomically, scoped to a selected source and
generation. Mark stale data rather than silently presenting it as current.

Close in milestones 1-2: offline browsing retains rows and cached covers; an
unconfigured root does not create a false missing-file report; failed or partial
read-back preserves the old snapshot; successful removal clears or marks derived
values appropriately.

### R8. Medium: Copy-only database access still needs a consistent snapshot

Evidence: [A45](docs/ASSUMPTIONS.md#L75),
[docs/decisions/0008-rekordbox-readback-xml-first.md](docs/decisions/0008-rekordbox-readback-xml-first.md)
and [milestone 5](docs/BUILD-PLAN.md#L107).

Not writing the original protects rekordbox, but copying only `master.db` while
rekordbox is running may omit committed WAL data or capture inconsistent pages.
Copying a live database and its WAL sequentially is not automatically a snapshot
either. Comparing it with an XML exported at another time cannot establish reader
correctness. [SQLite documents these risks](https://www.sqlite.org/howtocorrupt.html).

Recommendation: define how the owner supplies a quiescent, consistent snapshot,
including journal/WAL handling, and validate the copy before replacing the mirror.
Do not solve this by silently granting the experimental reader live-database
access. Compare XML and database fixtures from the same known state, including
cue times and playlist order rather than counts alone.

Close before enabling the option: synthetic WAL/crash/corrupt-copy fixtures and
a known-state comparison. Separately, test restore of the app's own pre-migration
snapshot, including an interrupted migration and incompatible schema. Original
`source_quality` and pending operations also need backups; they cannot generally
be reconstructed from the tags on a converted AIFF file.

### R9. Medium: Conversion has no complete space or audio-preservation contract

Evidence: [Disk-full handling](docs/design/import-pipeline.md#L94),
[milestone 4](docs/BUILD-PLAN.md#L96) and the Game Plan's conversion rule.

Checking free space against the downloaded release size is insufficient for
compressed lossless-to-AIFF conversion. Originals, staged inputs, expanded
outputs, publication copies and archives can coexist on different volumes.
Concurrent workers can each pass a preflight check and then exhaust the disk.
The output policy also leaves bit depth, sample rate, channels, MP3 quality and
already-compliant MP3/AIFF passthrough unspecified.

Recommendation: budget peak space per volume, account for expanded PCM and bounded
concurrency, and handle ENOSPC during every write. Specify preservation rules and
explicit encoder settings. Do not re-encode an already compliant MP3 merely to
retag it or infer an upgrade from the converted file's bitrate. Keep fake-lossless
analysis advisory as already planned.

Close in milestone 4: low-space and cancellation injection; 16/24-bit fixtures,
several rates/channel layouts; decoded-audio equivalence for lossless conversion;
retained tags/artwork; and an MP3 passthrough check. Verify the filed file before
discarding its staging recovery material.

### R10. Medium: Build and distribution risks need early, explicit gates

Evidence: [A5](docs/ASSUMPTIONS.md#L20), [A44](docs/ASSUMPTIONS.md#L74),
[Cargo.toml](Cargo.toml) and milestones 0, 4, 5 and 9 in
[docs/BUILD-PLAN.md](docs/BUILD-PLAN.md).

The accepted experimental option changes the whole application's SQLite build,
yet its Windows build cost is not tested until milestone 5. Inspection of the
local rbxport workspace and `rbl-db` manifests found `rusqlite` 0.40, matching this
repo, and the documented vendored SQLCipher/OpenSSL feature. This rules out an
obvious declared-version mismatch, not feature, FTS5, packaging or migration bugs.

Recommendation: run a small Windows/macOS dependency spike early, pin a reusable
rbxport revision, and test unencrypted app databases under both SQLite builds.
Do not expand the experimental scope or change its accepted shipping policy
without the owner. The audio choices are plausible: Symphonia 0.6.1 advertises
AIFF and MP3 support, but both require non-default features
([feature list](https://docs.rs/crate/symphonia/0.6.1/features)). Exercise the actual
AIFF/tag/seek path before building the full player around it.

Define supported OS versions and CPU architectures, choose a supported Node
version rather than dropping DOM tests to accommodate Node 26, and include
keyboard/focus/virtualization tests. Test the declared Rust MSRV or stop advertising
an untested minimum. Set signing/notarization and WebView2 installation policy
early; an unsigned owner build is not the same as a clean-machine release.

Close before external distribution: pinned sidecar provenance and hashes,
license notices and corresponding-source delivery to recipients, installer
upgrade/uninstall tests that preserve data, and signed builds or a documented
unsigned-release policy. A private source repository is compatible with private
development, but does not itself fulfill redistribution obligations. Review the
chosen FFmpeg build, including nonfree components and LAME, against its actual
license ([FFmpeg guidance](https://ffmpeg.org/legal.html)). No repository visibility
change is required by this review.

### R11. Medium: Current performance checks cannot enforce the stated UI budgets

Evidence: [Milestone 0 acceptance](docs/BUILD-PLAN.md#L45),
[decision 0004](docs/decisions/0004-interface-and-speed-budgets.md#L19) and
[mock search measurement](docs/design/interface.md#L76).

Decision 0004 defines user-visible interaction times, not a database-only budget,
although milestone 0 asks the database benchmark to meet its "database budgets".
A fast mock UI plus a fast standalone query does not establish combined latency
through Tauri, serialization, rendering and concurrent background work. Shared
CI hardware also cannot make an unqualified "no frame over 33 ms" a stable gate.

Recommendation: define named release-build workloads, reference hardware,
warm/cold cache states, samples and percentiles. Allocate backend and IPC/render
budgets within the interaction limit; measure keystroke-to-correct-painted-result
in the actual Windows app, including stale-response cancellation and import load.
Use stable CI regression tests plus owner/reference-machine absolute acceptance.
Specify how a page and its total remain coherent while the index changes.

Close from milestone 0 onward: benchmark first and deep windows, filtered search,
common/rare queries, null BPM, duplicate sort values and deterministic tie-breaks.
Start with indexed position-based reads; add a position index only if measured
workloads miss their budget. Defer neither instrumentation nor accessibility
testing to the final milestone.

### R12. Medium: Provider caching and retries need lifecycle and privacy rules

Evidence: [Cache schema](docs/design/data-model.md#L36),
[docs/decisions/0002-metadata-lookup-strategy.md](docs/decisions/0002-metadata-lookup-strategy.md)
and [milestone 7](docs/BUILD-PLAN.md#L129).

"Nothing is asked twice" conflicts with `expires_ms` and is especially harmful
for negative results on newly released music. The plan promises restartable
queues but does not specify persisted jobs, retry times or rate-limit state.
Later enrichment also needs to respect already accepted edits and frozen files.

Recommendation: distinguish a genuine miss from network/rate-limit failures;
define on-demand expiry, explicit refresh, deduplication, persisted backoff with
jitter and `Retry-After` handling. Do not implement automatic MusicBrainz change
polling. Per-IP limits are shared behind NAT and with other software, not a
guaranteed independent allowance per DJ
([MusicBrainz rules](https://musicbrainz.org/doc/MusicBrainz_API/Rate_Limiting)).
Make late results suggestions when they conflict with approved metadata.

Store user tokens in the OS credential store and redact them from logs, cached
URLs and audit records. Define consent for outgoing metadata/fingerprints and
limit fetched tag URLs/redirects, response sizes and artwork decoding. Free
pricing alone is not a determination of AcoustID's non-commercial eligibility;
confirm the intended distribution/use with the service before release
([usage guidelines](https://acoustid.org/webservice)).

Close in milestone 7: fake-server tests for a miss becoming a later hit, 429/503,
offline restart, duplicate jobs, revoked credentials and a late response after
manual approval. Local imports must continue when providers are unavailable.

### R13. Medium: Milestones are ordered sensibly but are not actually sized

Evidence: [Two-to-five-week claim](docs/BUILD-PLAN.md#L28),
[milestone 0 scope](docs/BUILD-PLAN.md#L30) and the Game Plan's 10-15 hours/week.

One milestone therefore has roughly 20-75 engineering hours. Milestone 0 includes
the full proposed schema, migrations, concurrency, search, benchmarks, safe file
operations, scoring, a watcher tracker, multiple UI views and two-platform CI.
Milestone 5 combines a user feature with a dependency/build migration. These are
scope lists, not estimates supported by a breakdown or feasibility results.
Ten such windows imply 20-50 working weeks before contingency, not a short build.

Recommendation: split the foundation and the XML writer/database experiment,
deliver vertical slices, and re-estimate after the first Windows artifact and
database benchmark. Keep schema contracts early, but implement later tables and
workers when their slice needs them. Reserve time for owner-run Windows checks,
failed experiments, documentation and packaging.

Close before scheduling: give each slice an observable demo, an hour estimate,
explicit dependencies and a defer/split condition. Use the sequence below as a
proposal, not a silent change to the owner's accepted scope.

## Verdict and sequence

The product plan makes sense. SQLite, a local metadata store, read-only indexing,
an XML-first mirror, local artwork/waveform caches and a dry run before filing are
appropriate choices. Keep the nine crate boundaries if they remain lightweight;
there is no reason to add a server database, hosted catalog or speculative position
index now. There are five high-priority findings and eight medium-priority gaps.

Proceed with a reduced, read-only foundation once the path and state contracts
are settled. Do not treat automatic filing or synced upgrades as implementation-ready.

| Order | Recommended slice | Exit evidence |
| --- | --- | --- |
| 0a | Reproducible toolchains, thin Tauri shell, CI and Windows artifact | Owner can install; both OS builds and basic UI tests pass |
| 0b | Library schema, migrations, read API, synthetic generator and real IPC list | Restore test; indexed queries and end-to-end baseline |
| 1 | Existing-library indexing, offline browsing and artwork | No writes; resumable scan and useful Windows library view |
| 2 | XML mirror and explicit root/path mapping | Identity, cue, playlist and snapshot tests |
| 6, reduced | Playback and waveform using that mirror | Early AIFF/MP3 decode, seek and cue-timing proof; full polish can follow |
| 3 | Download baselining, local identification and dry-run review | Safe restart/idempotency and measured match precision/coverage |
| 4 | Conversion and journaled publication | Space, crash, cancellation and no-clobber tests |
| 5a | XML sending and confirmation | Repeated/partial import tests; existing links protected |
| 5b | Experimental copy reader, separately gated | Early build spike first; consistent snapshot and known-state parity |
| 7, then 8 | Online enrichment, then dedupe/upgrades | Provider lifecycle tests; no synced replacement without an approved write design |
| 9 | Final installer, onboarding and documentation checks | Packaging/security work started earlier, clean-machine release tests |

This keeps read-back before import and gives an early useful player without
making the XML sender depend on the database experiment. Deferring the accepted
experimental option or synced upgrades beyond v1 requires an owner decision.

## Answers to the seven review questions

These correspond to the questions in [docs/BUILD-PLAN.md](docs/BUILD-PLAN.md).

1. **Order:** keep indexing, then XML read-back, then import. Bring a small player
	slice immediately after read-back, before XML sending, since it uses only
	read-only data and validates the dominant audio format early. Split milestones
	0 and 5 as above; the current numbering and the question about player-before-5
	should be reconciled when the owner chooses an order.
2. **Dry run:** keep it. It validates identification, path plans and review
	ergonomics without authorizing library writes. Use the same immutable plan
	representation and executor inputs later, with revalidation at commit time,
	so it does not become a separate implementation that disagrees with execution.
3. **Zero wrong auto-files / 200 downloads:** keep zero observed wrong accepts as
	a regression gate, not a guarantee. Start with manual approval, tune on one
	set and evaluate on a held-out set. The analytical bound for zero errors in
	$n$ independent accepted trials is $p_{95} = 1 - 0.05^{1/n}$; $n=200$ gives
	about 1.49%. Require coverage and accepted count, not just total downloads.
4. **CI minutes:** not yet answerable for this account or these builds. Current
	[GitHub billing documentation](https://docs.github.com/en/billing/concepts/product-billing/github-actions)
	lists 2,000 minutes/month and 500 MB artifact storage for GitHub Free. The
	[runner pricing page](https://docs.github.com/en/billing/reference/actions-runner-pricing)
	lists standard Windows at USD 0.010/minute and macOS at USD 0.062/minute for
	paid usage; those are not measured costs for this repo. Check the actual plan,
	other repositories' usage and current allowance accounting, then measure cold
	and cached jobs before budgeting. Set short installer retention, cancellation
	of superseded runs, caches and a spending limit. Reconcile "every change on
	both OSes" with "macOS only on pull requests"; direct pushes otherwise miss
	that gate. Keep the repository private.
5. **Position index:** no pre-emptive one. Measure indexed offset windows at
	100,000 rows with the real sort/filter combinations and stable tie-breaks.
	Add a position index only for a demonstrated failing workload. Metadata size
	and audio size are different concerns, as the plan correctly recognizes.
6. **Scoring:** the intent is conservative, but A13 and signal provenance make
	it insufficiently safe, and no precise policy yet proves useful coverage.
	Resolve R5 before tuning; foreign tools' distance or fingerprint percentages
	are not portable error probabilities.
7. **Assumptions that can change the plan:** A13-A14 (identity/automation),
	A15/A39/A41 (paths and XML matching), A18/A22/A44-A45 (database build, reuse and
	snapshots), A26/A33/A40 (sync propagation versus upgrades), and A48-A50
	(download observation/retention) are material. Settle A3-A6/A8 before creating
	distributable scaffolding and pin dependencies. ID3 output policy needs the
	planned rekordbox test. Name/accent/package-manager preferences are ordinary
	choices, not evidence that the architecture should change.

## Verification and limits

- Reviewed the Game Plan, build plan, all three design documents, assumptions,
  active decision records, both research notes, handoff/index/log, workspace
  manifest and lint/toolchain configuration. Withdrawn playlist/Navidrome
  decisions were treated as history, not requirements.
- Read the two local rbxport Cargo manifests only to check declared dependency
  compatibility. No rbxport code, audio or databases were executed or modified.
- Consulted the upstream Symphonia feature list, SQLite snapshot guidance,
  GitHub billing/pricing, MusicBrainz limits, AcoustID terms and FFmpeg licensing
  pages on 2026-10-22. These are documentary checks, not interoperability tests
  or legal clearance. No authenticated provider calls were made.
- Local validation used Node to check Markdown link targets, line references,
  duplicate finding IDs and control characters. The statistical figure is a
  calculation, not a benchmark: `node -e 'console.log(100*(1-0.05**(1/200)))'`.
- No application tests, Cargo build, latency benchmark, Windows installer check,
  rekordbox round trip or real-library inspection was run. No performance or
  real-world match-accuracy claim is established by this review.
- Product decisions remain unchanged. Recommendations should become accepted
  decisions and updated acceptance criteria only after review; this document
  does not authorize live database writes, real-library experiments or commits.