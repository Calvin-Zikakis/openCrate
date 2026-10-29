# oc-lookup

Looking a release up in MusicBrainz or Discogs, with no network code: this crate builds the URLs to ask, reads the
answers, and matches the release's tracks to the files of a download. The desktop app does the fetching, at the
pace each service allows.

## Public API

| Item | Purpose |
| --- | --- |
| `search_url`, `release_url`, `recording_search_url` | MusicBrainz web service (`/ws/2`, JSON) queries: a release by artist and title, one release with its tracks, and a recording by artist and title for when the album finds nothing |
| `parse_search`, `parse_recording_search` -> `Vec<Candidate>` | Read search results: id, score, title, artist, year and track count |
| `pick(candidates, files) -> Option<&Candidate>` | The one to fetch: a score of at least 80, preferring the same number of tracks as files, then the highest score, then the earliest year |
| `parse_release(json) -> Release` | A release with its media and tracks |
| `cover_art_url(id)` | The release's front cover in the Cover Art Archive |
| `discogs_search_url`, `discogs_release_url`, `parse_discogs_search`, `parse_discogs_release` | The same for Discogs, with the owner's personal token; a Discogs style (such as Deep House) is preferred over its broader genre |
| `suggest(release, files) -> Vec<Suggestion>` | For each file, the tag values the release gives it, where they differ from what the file has, and how the file was matched |
| `normalized(text)` | Lower case, letters and digits, single spaces, so "Night Drive (Dub Mix)" and "night drive - dub mix" compare equal |
| `USER_AGENT` | `openCrate/<version>` and the repository address, which MusicBrainz asks every client to send |

## Behaviour and limits

- One lookup per release, not per track, and only when the owner asks. MusicBrainz allows about one request a
  second from each address; keeping to that is the caller's job.
- Each track of the release is given to at most one file. A file that matches no track gets an empty suggestion.
- Only fields that would change are suggested; nothing is written here.

## Validation

From the workspace root:

```sh
cargo test -p oc-lookup --locked
cargo clippy -p oc-lookup --all-targets --locked -- -D warnings
```

The tests read answers stored as text in the tests, with synthetic names. No test touches the network.
