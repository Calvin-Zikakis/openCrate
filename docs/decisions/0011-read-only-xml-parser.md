# 0011: Standalone read-only rekordbox XML parser

Date: 2026-10-22.
Status: implemented and validated locally against synthetic fixtures.

## Context

The next slice after the synthetic shell is a standalone XML reader in
`oc-rekordbox`. It implements part of the XML-first approach in
[0008](0008-rekordbox-readback-xml-first.md), not the full read-back milestone.
The implementation and validation are recorded in
[research 005](../research/005-read-only-xml-validation.md). No speed claim.

## API and completion boundary

Expose a public Rust reader that accepts `BufRead` and returns either a complete
typed document or an error. `xml::read` and `xml::read_with_limits` use
`quick-xml` for streaming parsing. Custom limits may only lower the defaults.

The parser performs no path or file I/O. It has no database or UI connection.
Only valid end-of-input permits success; a closed root alone is not sufficient.
Malformed or truncated input must not return a partial document. No partial
mirror is published. Streaming input does not mean constant-memory output:
the complete typed document is retained within the resource bounds below.

## Data fidelity

- Read `DJ_PLAYLISTS`, `PRODUCT` and `COLLECTION` with their metadata attributes.
  Collection `TRACK` records have numeric `TrackID` values scoped to the source
  document, not global application or file identities.
- Keep string metadata and complete attribute maps alongside typed core fields.
  Less common and unknown attributes must not disappear because there is no
  dedicated typed field. Missing optional known attributes are `None`, not
  fabricated empty, zero or other default values.
- Preserve `POSITION_MARK` and `TEMPO` attributes, including cue names, types,
  numbers, colors, loop details and beat-grid data.
- Decimal time fields are finite, nonnegative `f64` seconds. Do not truncate
  them to milliseconds or assume they align to an audio sample. This does not
  settle playback or conversion alignment.
- Preserve each original `Location` value after XML unescaping, before percent
  decoding. Also retain the raw path bytes from the existing
  `location::decode_location` decoder. Literal punctuation, `#`, `+`, case and
  Unicode form must not become path normalization.
- Raw XML attribute text must be valid UTF-8. Percent-escaped path bytes may
  decode to non-UTF-8 bytes and must remain lossless. Invalid or unusable
  `Location` values fail the document; never replace bytes, omit the track or
  fall back to lossy path matching.

## Structure and references

Preserve nested playlist folders and playlists in document order. Playlist
track references use numeric `Key` values with `KeyType="0"` only. Keep
duplicate references. Keep references absent from the collection as explicit
unresolved source IDs; they are not evidence that a file is missing.

Reject duplicate collection `TrackID` values, declared count mismatches,
malformed numerics, unsupported `KeyType`, nested `TRACK` elements, invalid
document shape and truncation. Absence of an optional count does not justify
inventing a declared value.

Unknown metadata attributes are retained. Skipped extension roots are explicitly
reported in `Collection.ignored_elements`. Unknown elements may be skipped only
as validated subtrees under the same input and depth bounds. Skipping is not a
catch-all that converts malformed or unsupported known structures into success.

## Resource and XML safety

Initial maximums:

| Resource | Maximum |
| --- | --- |
| Input | 128 MiB |
| Collection tracks | 1,000,000 |
| Playlist track references, including duplicates and unresolved IDs | 1,000,000 |
| XML nesting depth, including unknown subtrees | 128 |

These are initial contract limits, not measured capacity or peak-memory claims.
The input bound also covers skipped content. All XML nesting counts toward the
depth limit, not only the playlist tree.

Reject DTDs and unsupported XML entity mechanisms. Permit only standard XML
unescaping: predefined XML entities and numeric character references. Do not
resolve external entities or fetch external resources. Invalid UTF-8,
malformed entities and malformed skipped subtrees must not be accepted through
a permissive fallback.

## Planned validation

Use synthetic strings and readers only. Cover:

- Windows, POSIX and UNC path shapes; punctuation, `#`, `+`, percent escapes,
  Unicode/XML entities and percent-escaped non-text path bytes.
- Collection metadata, optional fields, attribute retention, cues, loops and
  beat grids, including fractional seconds without millisecond truncation.
- Nested playlist order, duplicate references, unresolved IDs, duplicate
  collection IDs, declared counts and reference validation.
- Invalid numerics and Locations, unsupported keys, nested tracks, malformed
  shape, truncation and invalid trailing input.
- DTD/entity rejection and resource limits, including unknown subtrees.

Previously recorded private-owner research 002 counts inform capacity planning
only. Do not reopen that export or use real XML, library names or audio as
fixtures. No real-library access is authorized.

After the parent supplies tested code and commands, inspect public items,
tests and manifests, reconcile the crate README and this contract, and create
research 005 from actual evidence. Until then, no parser implementation,
test-pass or measurement claim belongs in the docs. The parent may report
necessary contract adjustments; record them explicitly rather than silently
weakening validation.

## Deferred work

No matching, root mapping, case folding, filesystem scanning, mirror database,
database transaction, UI picker, import, playback, XML writer or `master.db`
reader belongs to this slice. R3 path identity and R7 availability states remain
open. Existing product policy and milestone scope are unchanged.

Stop at the reader contract if broader integration is needed. Report the
blocker and split that work into a later slice rather than hiding it inside a
successful parse. No new schedule or performance estimate is established here.
