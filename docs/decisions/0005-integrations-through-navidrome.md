# 0005: Integrations go through Navidrome

Status: withdrawn 2026-10-22. The owner chose to skip Navidrome and AudioMuse-AI integration, and the Game Plan no longer mentions it. Kept as a record of what was found, in case the idea returns. The playlist-file idea that was meant to stay is withdrawn too ([0006](0006-playlists-m3u8-and-rekordbox.md), [0008](0008-rekordbox-readback-xml-first.md)), so nothing Navidrome-related remains in the plan.

## Context

The owner asked whether AudioMuse-AI and Navidrome are worth integrating. At that point the library was thought to be on a NAS, with Navidrome serving it and AudioMuse-AI analysing it. The library is in fact on the desktop and the NAS holds a synced copy ([0003](0003-local-library-synced-copy.md)). Sources: the Navidrome pages on Subsonic compatibility and on AudioMuse-AI, and AudioMuse-AI's README and architecture notes, all read on 2026-10-22.

## Findings

- Navidrome implements Subsonic API 1.16.1 with OpenSubsonic extensions. It exposes `getArtists`, `getAlbumList2`, `getAlbum`, `getSong`, `getCoverArt`, `search3`, `startScan` (with a `fullScan` option), `getScanStatus`, and playlist create and update.
- AudioMuse-AI has a Navidrome plugin. With it, Navidrome answers `getSimilarSongs` and `getArtistInfo` from AudioMuse-AI's sonic analysis, and advertises the OpenSubsonic `sonicSimilarity` extension: `getSonicSimilarTracks` (a song, with a score per match) and `findSonicPath` (a chain from a start song to an end song). Without the plugin, those two return 404.
- AudioMuse-AI fetches audio through the media server, analyses it with Librosa and ONNX models, and keeps results and embeddings in PostgreSQL. It is AGPL-3.0, and it needs Navidrome, the core app and the plugin kept at matching versions.

## Decision

1. Navidrome is the single integration. openCrate speaks Subsonic and OpenSubsonic to it, and detects features with `getOpenSubsonicExtensions`.
2. AudioMuse-AI is used only through Navidrome's `sonicSimilarity` extension: "similar tracks" and "path to" in the track menu, hidden when the extension is absent. openCrate does not talk to AudioMuse-AI or its database directly.
3. Navidrome adds three optional things, in this order of value: an "Import from Navidrome" source (tags, durations, bitrates and cover art from its API instead of reading 500 GB over the network), `startScan` after filing new releases, and publishing crates as playlists.
4. None of it is in the foundation or in the first import pipeline. Everything sits behind a library-source interface, and the folder scan stays the universal path for DJs without Navidrome.

## Reasoning

- It is the cheapest route to the "never laggy" requirement on the owner's setup: Navidrome has already read every tag and cover.
- One client covers import and similarity, and no code from an AGPL project is copied.
- Runtime feature detection survives version drift in a third-party plugin.

## Consequences

- A `LibrarySource` interface with two implementations (folder scan, Navidrome) is part of the "index in place" milestone design.
- Credentials go in the OS keychain, never in the database or the repository.
- Unverified, so test before building UI: how useful the similarity is on dance music (assumption A29, withdrawn with this decision), and whether moving files disturbs Navidrome's IDs (A26, still open as a sync-tool risk).
- Not worth doing: replacing openCrate's own identification or rekordbox read-back with AudioMuse-AI output. Its tempo and key are not a beat grid, and the plan already says rekordbox analyzes.
