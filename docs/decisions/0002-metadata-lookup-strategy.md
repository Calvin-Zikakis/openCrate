# 0002: Metadata lookup strategy

Status: proposed 2026-10-22 and recorded in the Game Plan. Pending owner review.

## Context

The owner wants a decent way to find tags, and noted that MusicBee and Lexicon do not seem to be rate limited. beets and MiniMediaScanner (with its datasets) were also to be evaluated. Evidence is in [research 001](../research/001-tag-lookup-options.md).

## Decision

1. Local first: embedded tags, MusicBrainz IDs already in the files, folder and file names, and the Bandcamp page.
2. Online lookups are per release, not per track. Fingerprint one or two tracks to find the release, then confirm the rest by track count and durations.
3. Providers for v1: MusicBrainz, AcoustID and Discogs (user token), plus the Bandcamp release page. Each runs from the user's own connection.
4. Every answer is cached, including misses. Each provider has its own throttle with backoff on 429 and 503, and the queue runs in the background and resumes after a restart.
5. Scoring follows beets: a weighted distance, strong, medium and low tiers, and a required gap to the runner-up. Only strong files itself. Numbers must match exactly (from MiniMediaScanner).
6. beets and MiniMediaScanner are references. Neither is a dependency, and the MiniMediaScanner datasets are not used.
7. The provider interface stays open, so a hosted service or an offline dump can be added later.

## Reasoning

- None of the tools avoids limits; each moves them. Lexicon runs its own database as a paid feature. The datasets moved the cost to one author over months. MusicBee works one album at a time. beets pays the published limits.
- The limits are per IP address, so a desktop app has one budget per user, and querying per release keeps a normal import to a few requests.
- beets needs Python, so it cannot ship in the installer. Its matching model is the part worth having.
- The datasets are 21.7 GB packed and 149 GB unpacked, need PostgreSQL, go stale for new releases, have no stated license, and were gathered from commercial APIs whose terms are unchecked.

## Consequences

- `oc-identify` gets a provider trait and a response cache before any network code.
- Register an AcoustID application key and tell AcoustID before release. The service is free for non-commercial use only.
- Offline lookup (MusicBrainz dumps, the AcoustID database, Discogs dumps) is a later option once sizes are measured.
- Each user's library is read from their own disk, so a scan is local and fast; no catalog or server is needed to index it ([0003](0003-local-library-synced-copy.md)).
