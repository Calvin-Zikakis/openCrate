# Research 007: How a real volume decides that two names are the same file

Date: 2026-10-22. Synthetic names only, in a throwaway directory. One machine:
arm64 macOS 27.0.1, local APFS data volume (`/dev/disk3s5`, mounted `apfs, local,
journaled`). Supports the proposed contract in
[the path identity design](../design/path-identity-and-availability.md) and
[decision 0013](../decisions/0013-filesystem-identity-and-availability.md).

## Method

A short Python script (`python3 -I`, standard library only, kept outside the
repository) creates synthetic files in a fresh directory, then asks whether other
spellings of the name reach the same file. The question is answered with
`os.lstat` on the variant and a comparison of `(st_dev, st_ino)` with the
original. Looking a variant up writes nothing, so the same technique could probe a
volume without touching a library folder. The files are created only to have
something to look up.

The first version mixed up two cases: its ASCII swap also changed a non-ASCII
letter. The script was corrected so the ASCII swap changes ASCII letters only, two
more cases were added, and it was rerun. Only the second run is reported.

## Result

| Stored name | Variant looked up | Same file? |
| --- | --- | --- |
| `Café Track.aif` (precomposed e-acute) | Decomposed spelling (`e` plus combining acute) | Yes |
| `Café Track.aif` | ASCII letters case-swapped, accent unchanged | Yes |
| `Café Track.aif` | Capital E-acute, rest unchanged | Yes |
| `Café Track.aif` | Create the decomposed spelling | Refused: the name already exists |
| `Straße.aif` | `STRASSE.AIF` | Yes |
| `Straße.aif` | `STRAßE.AIF` | Yes |
| `k.aif` | Kelvin sign U+212A in place of `k` | Yes |
| `fi.aif` | Ligature U+FB01 in place of `fi` | Yes |
| `A-B.aif` | `AB.aif` | No: two files |

The directory kept the name as it was created (precomposed bytes `43 61 66 c3 a9`).

## What it shows

- This volume treats names as equal under full Unicode case folding (sharp s to
  `ss`, the `fi` ligature to `fi`) and canonical normalization. The Kelvin sign
  row does not separate the two: it decomposes canonically to `K` and also folds
  to `k`. That is much coarser than ASCII-only or simple case mapping, so a lookup
  key built from either would miss aliases the volume resolves.
- Punctuation is never folded: `A-B.aif` and `AB.aif` stay distinct, which is the
  distinction R3 asked a path key to preserve.
- Stored names are preserved, so the raw bytes from a directory listing are the
  record of what is on disk; the volume does the equating at lookup time.
- Existing-name variants can be tested with a read-only `lstat` and compared by
  file identity. That makes the volume an authority for what a Location refers
  to, when the volume is online.

## Limits

- One volume of one filesystem family on one OS release. NTFS, exFAT, ext4, SMB
  and case-sensitive APFS were not run; their behavior in the design is from
  general knowledge and labelled as an assumption (A77).
- Whether the volume is case-sensitive was inferred from behavior. `diskutil`
  output did not state it.
- This does not show how rekordbox compares names. Whether rekordbox matches a
  Location differing in case or normalization is unknown and needs the owner's
  rekordbox.
- No timing was measured.
