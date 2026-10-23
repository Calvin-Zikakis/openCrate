# Filesystem identity and availability contract

Status: proposed 2026-10-22 under [decision 0013](../decisions/0013-filesystem-identity-and-availability.md).
Not yet reviewed by the owner. It answers review findings R3 and R7 and is the
gate before library migrations, the scanner and rekordbox matching. Nothing here
is implemented. The platform facts it relies on are labelled assumptions
(A67 to A78 in [ASSUMPTIONS](../ASSUMPTIONS.md)); only macOS APFS was measured
([research 007](../research/007-volume-name-equivalence-probe.md)).

## 1. Principles

1. **Raw bytes are the truth.** The path stored for a file is exactly what the
   operating system returned, never rewritten, folded or normalized.
2. **A key may miss a match; it may never invent one.** A false match attaches
   the wrong cues and playlist membership to a file. A missed match leaves an
   entry visibly unmatched and recoverable. Every rule below prefers the miss.
3. **Fuzzy metadata keys are not identity.** `artist_key`, `title_key` and the
   like never apply to paths. A path key never strips punctuation or diacritics.
4. **The volume decides what "the same name" means.** The app approximates that
   with a key that is never coarser than the volume (section 3). When the volume
   is online it can be asked directly (section 5, tier 2).
5. **Never write into a library root to learn about it.** Probing is read-only.
6. **Absence is a claim that needs evidence.** Offline, unmounted, unreadable and
   unindexed are not missing. Rows are never deleted by availability changes.

## 2. Stored path form

A file is `(root, relative path)`. The relative path is an ordered list of
**components**: byte strings that are non-empty, contain no NUL byte, are not `.`
or `..`, and contain no `/` (and no `\` when the volume is Windows-like).

- **Encoding.** On Unix a component is the raw bytes the OS gave. On Windows it is
  the UTF-16 name encoded as WTF-8 (UTF-8 that also admits unpaired surrogates),
  so well-formed names are ordinary UTF-8 on every platform. Use `encode_wide`
  and `from_wide` with an in-repo WTF-8 codec. Do not persist
  `OsStr::as_encoded_bytes`, which Rust documents as unspecified across versions.
- **Storage.** One BLOB holds the components joined by a single `0x00` byte. NUL
  cannot appear in a component on any supported system, so the form is
  unambiguous, does not depend on the OS separator, and compares bytewise in
  SQLite. A forward-slash string is never the stored form.
- **Display** text is derived from the bytes with lossy conversion and is never
  used for comparison. A name that is not valid UTF-8 is kept, shown with
  replacement characters and counted in the scan report, never skipped (A41).
- The root path is stored the same way (`path_raw`), not as text.

## 3. Name profiles and lookup keys

Each root has a **name profile**: the rule for deriving a lookup key from a
relative path. The key is stored beside the raw path and is unique per root.

| Profile | Key | Used when |
| --- | --- | --- |
| `exact` | The raw bytes | Default. Case-sensitive volumes, anything not established as insensitive, or after a collision |
| `fold_case` | Raw bytes with ASCII `A-Z` folded to lowercase | The volume is case-insensitive but does not normalize Unicode (assumed NTFS, FAT, exFAT) |
| `fold_case_normalized` | `fold_case` applied after NFC, on components that are valid UTF-8; other components stay raw bytes | The volume is also normalization-insensitive (APFS, HFS+) |

**The safety rule.** A profile must never be coarser than the volume: it may fail
to unify two spellings the volume equates, which only costs a missed match, but it
must never unify two names the volume keeps apart. Research 007 shows the stricter
side matters: APFS equated sharp s with `ss` and the Kelvin sign with `k`, which no
ASCII or simple fold reproduces. Therefore:

- The profiles above are deliberately narrow. Widening a fold (non-ASCII case,
  full case folding) requires fixtures from a real volume of that family first.
- Punctuation, accents on distinct letters and word spacing are never folded.
  Synthetic `A-B.aif` and `AB.aif` have different keys under every profile.
- Drive letters are compared ASCII case-insensitively regardless of profile.

**Collisions.** If two entries in one root get the same key during a scan, the
profile was coarser than that directory's reality (for example a case-sensitive
directory inside a case-insensitive root). The root's profile is demoted to
`exact`, every key in the root is recomputed in one transaction, and the demotion
is reported. No winner is picked and no row is dropped.

**Rekeying.** A profile or key-scheme change recomputes all keys for the root in
one transaction. The key scheme has a version number stored on the root.

## 4. Choosing a profile: read-only probes

At registration the profile is chosen from evidence, never assumed:

1. Ask the operating system for the volume's case and normalization flags when it
   can answer (Windows volume information; macOS volume capabilities). Linux has
   no portable answer, so it falls through.
2. Otherwise probe with `lstat` on variants of **entries that already exist**:
   a case-swapped spelling, and a differently normalized spelling of a name that
   has non-ASCII characters. If the variant reaches the same file identity, the
   volume is insensitive in that way; if it does not exist, it is sensitive. No
   file is created in the root. A root with no letters or no non-ASCII names
   gives no answer for that axis, and the answer stays `exact`.
3. The result is stored with its source (`os_query`, `stat_probe`, `user_set`,
   `default`). A user may set a profile; a profile coarser than the probe result
   is allowed only with a warning and is recorded.

The scanner may rerun the probe when new entries make an axis testable, and may
only move the profile toward `exact` without asking.

## 5. rekordbox Location to a file

`oc-rekordbox` supplies decoded Location bytes such as `/G:/Folder/File.aif`.
Nothing in this section changes that decoder.

**Shape.** The decoded bytes are classified, and anything else is
`unrecognized` and never guessed:

| Shape | Form | Notes |
| --- | --- | --- |
| `drive` | `/` + ASCII letter + `:` + `/` ... | Seen in the owner's export (research 002) |
| `unc` | `//` + server + `/` + share ... | How rekordbox writes UNC is unverified (A77) |
| `posix` | `/` + anything else | macOS and Linux paths |

**Components.** The rest is split on `/`. The match is refused for an empty
component after the first, `.` or `..`, or a `\` inside a `drive` or `unc`
component (Windows would read it as another separator, so it could escape the
intended folder). A `\` inside a `posix` component is a legal byte.

**Aliases.** A root has one or more `root_alias` entries: the prefix, in the same
shape and component form, that a Location may start with to belong to this root.

- The root's own current path is always an alias.
- Changing the root's path keeps the old aliases, so an older export that still
  says `D:` after the library moved to `G:` keeps matching.
- Other aliases are added only by the owner. The app may **suggest** one by
  counting the common prefixes of unmatched Locations ("8,977 entries start with
  `/D:/Library`"), but never accepts a suggestion on its own.
- The longest alias that is a whole-component prefix wins. `/G:/Music` does not
  match `/G:/MusicOld/x`. Two roots matching at equal length is `ambiguous` and
  matches neither.

**Two tiers of match.**

1. **Key match (database only, works offline).** Remainder components give a key
   under the root's profile; a row with that key in that root is the file. The
   result records whether the stored raw bytes equal the remainder (`exact_bytes`)
   or only the key does (`key_equal`).
2. **Volume-resolved match (online only, optional).** For an entry that maps into
   a root but has no key match, `lstat` the mapped path component by component
   without following links, read the file identity (inode and device, or the
   Windows file index and volume serial), and compare it with the stored
   `file_token` of indexed rows in that root. A hit is `volume_resolved`. This is
   how aliases the volume understands but the narrow key does not (research 007)
   are found without trusting an approximation. A stored token is a hint that is
   always re-verified, because copying or restoring a file changes it.

## 6. Containment and links

- A directory entry that is a symbolic link, a Windows junction or mount point, or
  anything with the reparse-point attribute is **not followed and not indexed**.
  It is counted and listed in the scan report. Entries are classified with
  `symlink_metadata`, never `metadata`. The implementation must prove junction
  detection on Windows before the scanner is accepted.
- Mapped paths are walked from the root one component at a time with
  `symlink_metadata`; a link at any step stops the walk. `canonicalize` is not
  used on candidate paths because it follows links.
- Registering a root records its canonical path and a directory token. Containment
  is checked again whenever a root path changes, not only when a download folder
  is first added.
- Paths longer than the classic Windows limit are indexed (extended-length
  prefix) and flagged `long_path`, never skipped. Whether rekordbox or the sync
  tool can open them is a separate question (A15, question 7).

## 7. Roots: registration, identity, availability

**Registration** (all read-only). The path exists and is a directory; it is
canonicalized; it is not inside, not containing and not the same directory (by
directory token, so subst drives, mapped drives and symlinks are caught) as any
other root; it is not inside the app-data directory or a staging or download
folder. A volume token is stored when the OS can provide one (Windows volume
serial or GUID; macOS volume UUID). Tokens are opaque and compared for equality
only.

**Availability** is a property of the root, evaluated on demand and stored with a
timestamp and a reason. It is **not** stored on file rows.

| Availability | Meaning | Effect |
| --- | --- | --- |
| `available` | The root path is a directory, its directory token and any recorded volume token match | Scans and existence checks may run |
| `unavailable` | Not found, not a directory, permission denied, network timeout, I/O error | Rows untouched; library stays browsable; UI shows offline |
| `unverified` | Reachable but the volume token or directory token differs (another drive now holds the letter, an empty mount point) | Treated like unavailable until the owner confirms it is the same library |

I/O on a root that may be a network share is bounded by a timeout and runs off
the UI thread; a call that does not return counts as `unavailable`.

## 8. File states

A file row carries two independent facts. "Offline" is derived from the root and
is never a third value.

| Field | Values | Set by |
| --- | --- | --- |
| `lifecycle` | `staged`, `active`, `archived` | openCrate operations (import, filing, archive). The scanner creates rows as `active` and never changes lifecycle |
| `presence` | `present`, `missing` | The scanner and targeted checks, under the rules below. With `last_seen_ms` and `missing_since_ms` |

Rules:

1. A row becomes `missing` only if its parent directory was enumerated
   **successfully** in a pass during which the root was `available`. A directory
   that could not be read (permission, I/O error, cancelled) leaves its whole
   subtree unchanged and is reported.
2. **Mass-disappearance hold.** If one pass would mark at least 25% of a root's
   present files, and at least 20 files, as missing, none of them change. The
   owner is asked to confirm. This catches an empty mount point or a moved
   folder. The numbers are a starting choice and have not been measured (A74).
3. A `missing` row that reappears at the same key becomes `present` again as the
   same row, so its track link and mirror matches survive. A file that appears at
   a different path is a new row. Move detection is later work.
4. Nothing deletes rows. Library views list `active` rows whether present,
   missing or offline, with an indicator. Operations that need the bytes (play,
   convert, verify for export) require `present` and an `available` root.

## 9. rekordbox entries: match status and mirror replacement

Each mirrored entry has a **match status** computed from the database only. It
never says "missing".

| Status | Meaning |
| --- | --- |
| `matched` | A file row was found (kind: `exact_bytes`, `key_equal`, `volume_resolved`) |
| `unmapped` | The Location is in no root's alias set, so it lies outside the configured library |
| `not_indexed` | It maps into a root but no row has that key: not scanned yet, deleted, or an alias the key misses |
| `ambiguous` | Two roots or rows claim it equally |
| `unrecognized` | The shape or components failed section 5 |

A separate **disk check** (`unchecked`, `exists`, `absent`, `root_unavailable`) is
an on-demand, read-only walk of the mapped path used for the Sync screen. Only
`absent` on an `available` root may be worded as "not on disk". A `matched` file
whose `presence` is `missing` is listed as gone through the file, not the entry.
rekordbox itself lists deleted files (research 002), so these are normal.

**Replacing the mirror.** The XML reader already returns a whole document or an
error (decision 0011). Per `rb_source`, a read is applied in one transaction that
replaces that source's entries, cues, tempo points and playlists and increments
its `generation`. A failed or partial read changes nothing and records the error.
Matching is recomputed in the same transaction.

**Derived values.** `bpm` and `musical_key` that openCrate copies onto a track
record the source and generation they came from. When a successful read no longer
contains the entry, or no longer matches the file, the value is cleared in the
same transaction. A value from an older generation than the current mirror is
shown as stale, never as current. Re-reading an unchanged export changes nothing.

## 10. First implementation slice (planned, not started)

A pure crate with no database and no UI, tested on synthetic data and temporary
directories only, covering: the stored path form and its codec on both platforms,
the three profiles and their keys, shape parsing and alias matching, containment
checks, the probe functions, and the state enums. The migration that creates
`library_root`, `root_alias` and `file` follows in the read-only library slice.

Required fixtures, from R3, R7 and the Opus review:

- Punctuation: `A-B.aif` and `AB.aif` distinct under every profile.
- Accents: precomposed and decomposed spellings, distinct under `exact` and
  `fold_case`, equal under `fold_case_normalized`.
- Case: `Track.aif` and `track.aif` distinct under `exact`, equal under
  `fold_case`; non-ASCII case and sharp s not folded.
- Raw names: invalid UTF-8 bytes (Unix) and an unpaired surrogate (Windows)
  round-trip; NUL-joined storage round-trips; names containing `#`, `+`, `%`.
- Locations: `drive` with upper and lower case letters, `unc`, `posix`, malformed
  shapes, `.`/`..`/empty components, `\` in `drive` and `posix`.
- Aliases: component boundary (`/G:/Music` against `/G:/MusicOld`), longest
  prefix, an equal-length tie, an old prefix kept after the root path changes.
- Links: a symbolic link to a directory and to a file are reported and not
  followed; a link in the middle of a mapped path stops the walk. Windows junction
  fixtures need Windows CI and stay open until then.
- Roots: overlapping, nested, duplicate and aliased roots are refused.
- Probes: on the host volume, a profile is never coarser than what `lstat` shows.
- States: an unavailable root causes no `missing`; an unreadable directory leaves
  its subtree untouched; the mass-disappearance hold triggers and releases; a
  reappearing file is the same row; a token mismatch gives `unverified`.
- Mirror: a failed read keeps the old generation; a successful read clears
  derived values whose match vanished.

## 11. Not decided here

- Move detection, hashing and duplicate handling (milestone 8).
- Atomic publication, staging and recovery for file changes (R1).
- How rekordbox itself compares names, and how it writes UNC and macOS paths.
  These need the owner's rekordbox and real exports.
- Whether library items reached through links should ever be indexed.
- Windows path-length limits for generated names (planned helpers).
