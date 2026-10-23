# oc-rekordbox

Lossless Location decoding and a standalone typed XML reader are implemented
under [decision 0011](../../docs/decisions/0011-read-only-xml-parser.md).
Neither resolves paths, matches files, touches a database or connects to the UI.

## Public API

`location::decode_location(&str) -> Result<Vec<u8>, location::LocationError>`
accepts the exact, case-sensitive `file://localhost/` prefix. It percent-decodes
the suffix once and prepends `/`. A synthetic drive-path input
`file://localhost/G:/Synthetic%20Track.aif` therefore returns the bytes
`/G:/Synthetic Track.aif`.

- Both hex cases are accepted. Literal `#`, `+`, punctuation, case and Unicode
  form are preserved; this is not a URL parser or a form decoder.
- Percent-encoded non-UTF-8 bytes are preserved. The input is text; the output
  need not be valid UTF-8.
- `LocationError` has `UnsupportedPrefix`, `InvalidEscape { offset }` and
  `NullByte` variants. Escape offsets are byte offsets into the suffix after the
  prefix. Error messages do not contain the input path.

## Safety and limits

The decoder does no filesystem access, Unicode normalization, case folding,
root remapping or containment checking. Drive, UNC and POSIX path shapes remain
bytes, not a verified OS path or file identity. It does not sanitize traversal
components. Callers must not treat successful decoding as permission to open a
file or as proof of a match.

There is no Location encoder, XML writer, rekordbox mirror, `master.db` reader
or live rekordbox access in the validated baseline. The filesystem identity
contract remains open under review finding R3; see
[decision 0009](../../docs/decisions/0009-reduced-foundation-and-recovery.md).

## XML reader

`xml::read(impl BufRead)` and `xml::read_with_limits(impl BufRead, Limits)`
use `quick-xml`, returning a
complete typed document or an error only after validating the full input.
It retains metadata attributes, original XML-unescaped Locations and decoded
path bytes, fractional-second cue/grid data, playlist order, duplicate
references and explicit unresolved IDs. It performs no file I/O, matching,
database work or UI integration.

Initial limits are 128 MiB of input, 1,000,000 collection tracks, 1,000,000
playlist references and XML depth 128, including unknown subtrees. Malformed
input, invalid Locations, count mismatches, duplicate collection IDs and
unsupported key/entity mechanisms fail. Custom limits can only lower defaults.
`Collection` exposes tracks, a nested `PlaylistNode` tree, preserved attribute
maps, sorted unresolved IDs and sorted skipped extension roots in
`ignored_elements`. Cue/tempo times stay finite nonnegative `f64` seconds.
Unknown subtrees are skipped rather than imported, with text/entity and
structural validation; this is not a general XML schema validator.

## Validation

From the workspace root:

```sh
cargo test -p oc-rekordbox --locked
cargo clippy -p oc-rekordbox --all-targets --locked -- -D warnings
```

The current run on arm64 macOS with Rust 1.99.0 passed 23 crate tests
(seven codec and 16 parser), with strict clippy clean. Inputs are synthetic
strings/readers only, including 10,000 generated collection entries. Tests cover
punctuation, one-pass decoding, raw bytes, case and Unicode preservation, path
shapes, malformed escapes, NUL and unsupported prefixes.

No private export or real-data matching test was run. Windows CI is configured,
not remotely validated. See [research 005](../../docs/research/005-read-only-xml-validation.md).
