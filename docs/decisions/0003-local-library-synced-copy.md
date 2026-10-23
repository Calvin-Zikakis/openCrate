# 0003: The local folder is the source of truth, the NAS holds a synced copy

Status: proposed 2026-10-22 and recorded in the Game Plan. The setup it describes (library on the desktop, a synced copy on the NAS) is the owner's own statement; the design that follows from it is a proposal. The sync tool is QNAP QSync, two-way, with the desktop owning the files (assumption A33). It replaces an earlier draft that assumed the library lived on the NAS.

## Context

The owner's audio files are on the desktop. That folder is shared with a NAS, so the desktop is the source of truth, and Navidrome serves the NAS copy to a phone. Nothing needs to be streamed from the NAS. Most other DJs will have a local or external drive.

## Decision

- openCrate works on the desktop folder only. It never writes to the NAS, never talks to the sync tool, and never streams from the NAS.
- A library root plus a path relative to it are stored separately, so moving the folder or remapping a drive letter is one setting.
- A root that is not reachable (an unplugged drive, a network folder away from home) is offline, not deleted. No row is marked missing or removed while a root is offline.
- The database, the staging folder and the archive folder all live outside the synced folder, on the same volume as the library where possible. The final step of an import is then an atomic rename, and the sync tool only ever sees finished files.
- When staging and library are on different volumes, copy to a temporary name beside the destination, verify size and checksum, rename, and only then delete the staging copy.
- Replaced files are archived, never deleted, and nothing moves in the existing library, because the sync tool mirrors every move and delete to the NAS.
- Local folders use file-system events. A root on a network drive is polled.
- File names are sanitized to the strictest common rules and written as NFC.

## Reasoning

- The sync tool copies whatever it finds. Half-written files, archived old versions and files waiting in the review queue must stay out of the folder it watches, and out of any media server that indexes the NAS copy.
- SQLite's WAL mode does not work over a network filesystem, and a sync tool can capture a database mid-write, so the database cannot sit in the synced folder.
- An unmounted root looks like "every file deleted" to a naive missing-file check, which would empty the library.

## Consequences

- `oc-core-db` has `library_root` and `file.rel_path`, with a unique `(root_id, rel_path)`.
- `oc-organize` provides an atomic rename for the same volume and a copy-then-verify move for different volumes.
- `oc-ingest` needs a polling mode for roots on network drives. Its stability check uses size and modified time only.
- The sync tool is QNAP QSync, which syncs both ways (assumption A33). A change made on the NAS side could come back to the desktop. The desktop owns the files and Navidrome does not write to the music folder (its documentation), so the risk is small, and the owner does not consider it important.
