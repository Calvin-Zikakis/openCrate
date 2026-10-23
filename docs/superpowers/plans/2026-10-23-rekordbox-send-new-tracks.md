# Send new tracks to rekordbox (XML route): implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracks.

**Goal:** After an import, the rekordbox screen lists the tracks openCrate filed that rekordbox does not have yet; "Send to rekordbox" writes them into one stable rekordbox XML file, in a playlist the owner adds to their rolling "Imported" playlist, and openCrate remembers them as offered until a later read shows rekordbox has them.

**Architecture:** `oc-rekordbox::xml` gains a writer whose `Location` spelling follows research 002. `oc-core-db` finds candidates from the publish log (files openCrate filed), drops any the current mirror already matches, and records offers (schema v14). The host writes the XML atomically into the app data folder. The page lists, ticks and sends.

**Tech Stack:** Rust, quick-xml (already used for reading), Tauri 2, React, Playwright.

**Spec:** [docs/design/rekordbox-sync.md](../../design/rekordbox-sync.md), "New music" steps 1-2, and the anti-goals. Review finding R4 (offered versus confirmed) in [review-findings.md](../../review-findings.md).

## Global Constraints

- Never write rekordbox's own files on this route: openCrate writes only its own XML file; the owner imports it in rekordbox.
- Only tracks rekordbox does not already have are offered, judged from the current mirror; if there is no mirror, or it is older than the newest filing, the page says so before sending.
- `Location` spelling: `file://localhost/` then the absolute path with forward slashes; percent-encode space, `'`, `&`, `[`, `]`, `%`, `"`, `<`, `>`, `\`, `^`, `` ` ``, `{`, `|`, `}`, control characters and every non-ASCII byte, with lowercase hex; leave letters, digits, `/`, `:`, `,`, `#`, `+`, `$`, `-`, `.`, `_`, `(`, `)`, `!`, `~`, `;`, `=`, `@`, `*` raw.
- The XML playlist is named "openCrate new tracks" (never "Imported"), so importing it can never replace the owner's Imported playlist.
- No track names from the owner's library in the repository; synthetic fixtures only.

## Review Focus

1. A track rekordbox already has (matched by the mirror) is never offered, even if openCrate filed it.
2. A filed file later deleted or archived is not offered.
3. Names with `&`, quotes, `<`, non-ASCII, `#`, `+`, `;` survive the XML round trip exactly (attribute escaping and Location encoding).
4. A failed write leaves the previous XML file whole (temporary file, then rename).
5. Sending an empty selection is refused; sending twice overwrites the file with the newest selection and keeps both offers.

---

### Task 1: XML writer and Location encoding

**Files:** Create `crates/oc-rekordbox/src/xml/writer.rs`; modify `crates/oc-rekordbox/src/xml/mod.rs`, `crates/oc-rekordbox/src/location.rs`; tests in `crates/oc-rekordbox/src/xml/tests.rs`

**Interfaces (produces):**
- `pub fn encode_location(path: &[u8]) -> String` in `location.rs` (absolute path bytes, `/` or `\` separators, Windows drive form `C:/...` or POSIX `/...`).
- `pub struct NewTrack { pub path: Vec<u8>, pub name: String, pub artist: String, pub album: String, pub year: Option<i32>, pub total_seconds: u32, pub size: u64, pub kind: String }`
- `pub fn write_offer(tracks: &[NewTrack], playlist: &str, product_version: &str) -> String`

- [ ] Tests: `locations_are_spelled_like_rekordbox` (`G:/Music/A & B/It's [VIP] 100% Café, #1 + $.aif` gives `file://localhost/G:/Music/A%20%26%20B/It%27s%20%5bVIP%5d%20100%25%20Caf%c3%a9,%20#1%20+%20$.aif`; `/Users/dj/Music/x y.mp3` gives `file://localhost/Users/dj/Music/x%20y.mp3`; `C:\Music\a.wav` gives `file://localhost/C:/Music/a.wav`); `encoded_locations_decode_back` (for a set of awkward byte paths, `decode_location(encode_location(p)) == p`); `an_offer_reads_back_through_the_reader` (two tracks with `&`, `"`, `<`, non-ASCII in names: `read` gives 2 tracks with the same names and location bytes, one playlist "openCrate new tracks" under ROOT holding both ids in order, COLLECTION Entries 2).

### Task 2: Candidates and offers in the database

**Files:** Create `crates/oc-core-db/src/schema/014_rekordbox_offer.sql`, `crates/oc-core-db/src/offers.rs`, tests `crates/oc-core-db/src/offers_tests.rs`; modify `lib.rs` (migration list), `service.rs`

**Interfaces (produces):**
- Table `rekordbox_offer(file_id INTEGER PRIMARY KEY REFERENCES file(id) ON DELETE CASCADE, offered_ms INTEGER NOT NULL) STRICT`
- `pub struct Candidate { pub file_id: i64, pub path: Vec<u8>, pub title: String, pub artist: String, pub album: String, pub year: Option<i32>, pub duration_ms: i64, pub size: i64, pub container: String, pub filed_ms: i64, pub offered_ms: Option<i64> }`
- `pub fn candidates(connection: &Connection) -> Result<Vec<Candidate>>` (newest filing first): destinations of publish operations in phase `recorded` or `cleaned`, resolved to files with `Matcher`, active and present, with no `rb_track` matched to the file.
- `LibraryService::rekordbox_candidates() -> Result<(Vec<Candidate>, Option<i64>)>` (the second value is the newest mirror `last_read_ms`), `LibraryService::record_offers(file_ids: Vec<i64>, now_ms) -> Result<()>`

- [ ] Tests: `a_filed_file_is_a_candidate_until_rekordbox_has_it`, `deleted_and_unfiled_files_are_not_candidates`, `offers_are_remembered_and_cleared_with_the_file`.

### Task 3: Host commands

**Files:** Create `src-tauri/src/rekordbox_send.rs`; modify `src-tauri/src/main.rs`

**Interfaces (produces):**
- `rekordbox_new_tracks() -> NewTracksDto { tracks: Vec<CandidateDto>, mirrorReadMs: Option<i64>, newestFiledMs: Option<i64>, xmlPath: String }`
- `rekordbox_send(fileIds: Vec<i64>) -> SentDto { xmlPath: String, tracks: usize }`: refuses an empty list and ids that are not current candidates; writes `<app data>/rekordbox/opencrate.xml` through a temporary file and rename; records offers.

- [ ] Tests: `sending_writes_one_stable_file_and_records_offers` (filed fixture tracks, send two: the file reads back with both, offers recorded), `unknown_or_empty_selections_are_refused`.

### Task 4: Interface

**Files:** Modify `src/library/types.ts`, `src/library/tauri-api.ts`, `src/components/Sync.tsx`; create `src/components/SendToRekordbox.tsx`; tests in `src/library/tauri-api.test.ts` and `tests/browser/desktop-bridge.spec.ts`

- [ ] A card "New in your library" on the rekordbox screen when there are candidates: each track with a tick (on by default), title, artist and "sent, waiting for rekordbox" for offered ones; a notice when there is no rekordbox read or it is older than the newest filing ("Read rekordbox first so tracks it already has are left out"); "Send to rekordbox".
- [ ] After sending: the file path, and the steps: in rekordbox Preferences, Advanced, rekordbox xml, set Imported Library to this file (once); in the tree, open rekordbox xml, then the playlist "openCrate new tracks"; select its tracks and drag them onto your Imported playlist. Marked on the page as steps to check on the first use.
- [ ] Browser tests: `new tracks are listed, ticked and sent`, `a stale read is pointed out`.

### Task 5: Docs

- [ ] LOG, ASSUMPTIONS (Location set for characters research 002 did not see; playlist name; offered state), HANDOFF; owner check list: import the XML on the laptop's rekordbox and confirm the drag adds to Imported without duplicates.
