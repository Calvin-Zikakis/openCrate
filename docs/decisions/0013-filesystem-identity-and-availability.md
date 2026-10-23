# 0013: Filesystem identity and availability

Date: 2026-10-22.
Status: proposed. Implemented provisionally under the owner's overnight
instruction to keep working; not reviewed by the owner. Revisable.

## Context

Review findings R3 and R7 block library migrations, the scanner and rekordbox
matching. R3: search normalization must not define filesystem identity, and the
proposed `(root_id, rel_path)` uniqueness and `location_key` do not model
case-insensitive volumes, raw non-text names or old drive prefixes in an export.
R7: offline, missing and unmatched are different states. The contract is in
[the design](../design/path-identity-and-availability.md). The key measurement is
[research 007](../research/007-volume-name-equivalence-probe.md): on APFS the
volume equated sharp s with `ss`, the Kelvin sign with `k` and decomposed with
precomposed accents, so no simple fold reproduces a volume's equality.

## Decision

- Store each path as raw bytes, as root plus NUL-joined components, never as a
  separator string. Keep the display text separate and lossy.
- Derive a lookup key per root from a small set of profiles (`exact`,
  `fold_case`, `fold_case_normalized`) that may never be coarser than the volume.
  A missed match is acceptable; a false match is not. Unknown means `exact`.
- Choose the profile from read-only evidence: an OS volume query, else `lstat`
  on variants of existing entries. Never create probe files in a library root.
  A key collision demotes the root to `exact`.
- Map a rekordbox Location to a root through explicit aliases, matched by whole
  components and longest prefix. Changing a root's path keeps old aliases. Aliases
  are suggested from unmatched prefixes but accepted only by the owner.
- Match in two tiers: a database-only key match, then an optional online
  `lstat` resolution compared by file identity.
- Do not follow or index symbolic links, junctions or reparse points.
- Derive offline from the root. A file row carries `lifecycle` and `presence`
  only. Presence becomes `missing` only under directories enumerated successfully
  while the root was available, with a hold on mass disappearance.
- Match status is a database computation and never says "missing". A separate
  on-demand disk check supports the Sync screen.
- Mirror replacement is one transaction per source with a generation. Derived
  BPM and key record their source and generation and are cleared or marked stale.

## Alternatives rejected

- **One normalized `location_key` for all paths** (the original sketch). It
  collapses distinct names and conflates identity with search.
- **Full Unicode case folding and NFC in the key.** It can be coarser than a
  volume that is case-sensitive or does not normalize, which creates false matches.
- **Probe by creating files in the root.** Writes into a synced library.
- **Store `offline` on every file row.** Unplugging a drive would rewrite thousands
  of rows and the two copies would drift (Opus review).
- **Mark entries missing when no file matches.** rekordbox lists deleted files and
  roots can be unplugged; the app would report false losses.

## Consequences

- The `file`, `library_root` and `rb_track` sketches in the data model change when
  the migrations are written: raw path BLOBs, a per-root profile, aliases, and
  presence and match fields replacing `state` and `file_missing`.
- A pure crate `oc-paths` is proposed for the codec, keys, Location mapping and
  probes, so `oc-core-db` and `oc-rekordbox` share one definition without a
  dependency on each other. It is a tenth boundary beyond the Game Plan's nine
  and needs the owner's confirmation; the Game Plan is not changed yet.
- Library views stay browsable offline. Stale cached data is marked.
- Windows, UNC, junctions and rekordbox's own name comparison remain unverified.
