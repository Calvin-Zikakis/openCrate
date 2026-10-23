# Research 005: Standalone XML reader validation

Date: 2026-10-22. Synthetic data only, arm64 macOS, pinned Rust 1.99.0.

## Commands and results

From the repository root, using `~/.cargo/bin/cargo` where Cargo is not on PATH:

| Command | Result |
| --- | --- |
| `cargo fmt --all` and `cargo fmt --all --check` | Passed |
| `cargo test -p oc-rekordbox --locked` | 23 passed: seven Location tests, 16 parser tests; zero doctests |
| `cargo clippy -p oc-rekordbox --all-targets --locked -- -D warnings` | Passed |
| `cargo test --workspace --locked` | 41 passed: 18 database, 23 rekordbox; Tauri has no tests |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `git diff --check` | Passed before final documentation edits |

Dataset: inline synthetic XML with two tracks, fractional cues/loops, two tempo
points, nested/empty playlists, repeated and unresolved references, entities,
raw percent-escaped bytes and drive/UNC/POSIX paths. A generated 10,000-track
document checks exact count and ordering; this is not a benchmark.

Failure tests cover truncation, malformed structure, duplicate IDs/attributes,
declared counts, unsupported playlist keys, invalid timing/Locations/encoding,
DTD/entity rejection, invalid text even in skipped extensions, I/O errors,
byte/depth/entry limits and invalid declarations/comments. Tiny input buffers
produce identical results. Unknown subtree roots are reported, not silently
represented as imported metadata.

First run passed 21 tests but strict clippy found an unused import and five
test assertion lints. Those were fixed; two extra XML rejection tests brought
the total to 23. An interrupted validation invocation was rerun successfully.

## Limits

Final repeated workspace run: 16 database tests passed, two failed (unexpected
`AlreadyOpen` on drop/reopen and unexpected error instead of `InvalidState`).
The preceding 41-test passing result above is historical, not stable acceptance.
XML-targeted tests passed 23; no evidence connects the failure to XML parsing.
The cause was later found and fixed; see [research 006](006-owner-lock-inheritance.md).
Final strict clippy, format/diff checks and 290 local Markdown targets passed.

No real exports, audio, directories or databases were opened by parser tests.
No Windows/hosted CI, XML timing, memory or real-export parity claim. The API
retains a complete typed document in memory; limits bound input and structure,
not measured peak memory. It is not a complete general-purpose XML validator.
Unknown extension subtrees are skipped with their root names reported.
No filesystem identity/matching, mirror transaction, native picker, IPC,
XML writing, live rekordbox access or playback is implemented by this slice.

The prior shell's 12 frontend unit and nine Chromium tests were not rerun for
Rust-only XML changes. Their separate evidence remains in research 004.
