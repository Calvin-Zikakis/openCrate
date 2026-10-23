# Research 001: ways to look up tags

Date: 2026-10-22. Sources are vendor documentation and repository pages read that day. Nothing here was tested against the live services.

## Question

The owner asked how MusicBee and Lexicon find tags "without rate limiting", and asked for beets and MiniMediaScanner to be evaluated (the second came from a Reddit comment by the MiniMediaScanner author, r/musichoarder).

## Official limits

| Service | Limit | Other terms | Source |
| --- | --- | --- | --- |
| MusicBrainz API | About 1 request per second per IP address, on average. Above that, every request from the IP is declined with 503 until the rate drops. 300 per second globally | A User-Agent with contact details is required. No polling for changes. No fixed-time schedules | https://musicbrainz.org/doc/MusicBrainz_API/Rate_Limiting |
| AcoustID web service | No more than 3 requests per second | Free for non-commercial use only. An application API key is required. Tell them before deploying at volume | https://acoustid.org/webservice |
| AcoustID database | None (download) | Daily incremental JSON files. Fingerprint data is CC BY-SA 3.0, the MusicBrainz mapping is CC0 | https://acoustid.org/database |
| Discogs API | 60 per minute authenticated, 25 unauthenticated, measured over a 60-second moving window | A unique User-Agent is required. Monthly data dumps are CC0 | https://www.discogs.com/developers |

Because the limits are per IP address, an app that runs on each DJ's machine has one budget per user. A server that proxies every user would share one budget, which is why centralised services need their own database.

## Findings by tool

### Lexicon, Find Tags & Album Art

- Listed under the Ultimate plan only (the features table shows it as not included in Free or Essential).
- "All information comes from SonoVault, Lexicon's own music database. The source can't be changed: there is no option anymore to choose Beatport, Spotify or another source." A 2022 forum answer from the developer said the data then came mostly from Spotify, with Discogs for genre.
- It matches on the existing artist and title text, not on audio. "If these are not clean then it will be harder to find the correct, or any, match." The developer also said bootlegs and unofficial releases are often not found.
- It fills missing fields only: genre, year, label, album, artwork, energy, danceability, popularity, happiness. It does not identify unknown files.
- Sources: https://www.lexicondj.com/manual/find-tags-and-album-art, https://www.lexicondj.com/features, https://discuss.lexicondj.com/t/lexicon-tag-find-tags-and-album-art/1014

### MusicBee auto-tag

- The wiki describes it as user-triggered: select tracks from one album, press Ctrl+Shift+T, and the tagging window searches for a matching album. The sources are a configurable list of URLs in the preferences.
- Only search-result snippets were readable, so this is thinner than the Lexicon evidence.
- Source: https://musicbee.fandom.com/wiki/Auto-Tagging (snippet), https://getmusicbee.com/forum/index.php?topic=36170.0 (snippet)

### beets

- Python, MIT licensed, 15.8k stars, 620 contributors. Installed with `pip`. Library is SQLite, and `create_backup_before_migrations` defaults to yes.
- Matches whole albums. MusicBrainz is the default source; plugins add Discogs, Beatport (`bpsync`), Deezer, Spotify, Tidal, AcoustID (`chroma`), filename guessing (`fromfilename`) and, from the community, Bandcamp (`beetcamp`).
- Scoring is a distance from 0.0 to 1.0. Default weights include artist 3.0, album 3.0, track title 3.0, track length 2.0, tracks 2.0, album ID 5.0, track ID 5.0, year 1.0, label 0.5, missing tracks 0.9, unmatched tracks 0.6.
- Tiers: a match is "strong" below a distance of 0.04 (`strong_rec_thresh`), and medium or low otherwise. The gap to the next-best candidate (`rec_gap_thresh`) also matters. Missing or unmatched tracks cap a match at "medium". Quiet mode (`quiet`, with `quiet_fallback` of `skip` or `asis`) never asks, which is the same shape as auto-file plus a review queue.
- `duplicate_action: upgrade` replaces a track only when the new one has a higher bitrate. Default duplicate keys are `albumartist album` and `artist title`.
- Default path templates: `$albumartist/$album%aunique{}/$track $title`, and `Non-Album/$artist/$title` for singletons. The default filename replacements remove `/` and `\`, leading dots, control characters, `<>:"?*|`, trailing dots and surrounding whitespace, and the docs say they are enough "to avoid unexpected behavior on all popular platforms".
- Fingerprinting "takes significantly more CPU and memory than ordinary tagging", and the `chroma` plugin only produces candidates when the `musicbrainz` plugin is also enabled.
- The SQLite post (2016) argues that a read-heavy, write-light workload suits SQLite, that a client-server database is "many times more frustrating to use" for a desktop app, and that query tuning should come before a new DBMS.
- Sources: https://beets.io/, https://docs.beets.io/en/stable/reference/config.html, https://docs.beets.io/en/stable/plugins/index.html, https://docs.beets.io/en/stable/plugins/chroma.html, https://beets.io/blog/sqlite-performance.html, https://github.com/beetbox/beets

### MiniMediaScanner and Datasets (MusicMoveArr)

- C# (.NET), GPL-3.0, one contributor, 54 stars, last commit about three months before the read. Needs PostgreSQL. Docker is the documented path. It says it was inspired by beets.
- Tagging: text match against local copies of MusicBrainz, Spotify, Tidal and Deezer data. Tags are written only when artist, album and title match at 80% or more, and anything containing a number must match 100% ("Best of 2020" must not match "Best of 2021"). Fingerprint matches need 98%. It never asks for confirmation.
- Other ideas: `fixversioning` moves remixes to disc 1001 and up so they stay apart from the album; `deduplicate` uses 98% filename and 99% fingerprint accuracy, noting that 98% or lower "can mismatch real fast, think of remixes etc".
- The datasets are bulk copies of Deezer, Tidal, Spotify and MusicBrainz. February 2026: 21.7 GB packed and 149 GB unpacked as SQL (178 GB as CSV). Deezer is about 170 GB in Postgres (about 180 million tracks), Tidal about 110 GB, Spotify about 20 GB, MusicBrainz about 25 GB. Distribution is by torrent, and later updates are SQL files on Mega applied with `psql`.
- The author says the Tidal, Spotify and Deezer data came from months of calling their APIs around the clock, and that Spotify blocks a key for roughly 15 hours if the limit is hit. Spotify pulling is discontinued. The MusicBrainz part comes mostly from its own published dataset.
- The Datasets repository shows no license. MiniMediaScanner shows GPL-3.0.
- Sources: https://github.com/MusicMoveArr/MiniMediaScanner, https://github.com/MusicMoveArr/Datasets

## Reading across the tools

None of them removes the limit. Each moves it:

| Approach | Who pays the limit |
| --- | --- |
| Lexicon's own database | Lexicon, on its server, as a paid feature |
| MiniMediaScanner datasets | The dataset author, once, over months of API calls |
| MusicBee | Nobody: one album at a time on demand |
| beets | The user, on MusicBrainz and the others, at the published rate |

## Verdict

See [decision 0002](../decisions/0002-metadata-lookup-strategy.md). In short: live lookups per release from the user's own connection, cached and throttled; beets' matching model is copied, its code is not used; the datasets are not used.

## Open items

- Sizes of the MusicBrainz database dumps and the AcoustID download, before any offline mode is designed.
- Whether the Datasets' source APIs allow redistribution. Not needed unless the datasets are ever considered again.
- Bandcamp's page structure and any stated limits, when the Bandcamp provider is built. beetcamp is a reference; check its license before reading its code.
