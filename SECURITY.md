# Security

## Reporting

If you find a way for openCrate to read, change or send something it should not, report it privately through
GitHub's "Report a vulnerability" form on this repository's Security tab, which only the maintainer can read.
Say which version you used and how to reproduce it; a synthetic file or a description of its tag layout is
enough. Do not send real audio or a real library.

## What the app touches

openCrate is a desktop app with no server and no account. Knowing what it does helps judge a report:

- It reads the music folders and watch folders you choose, and writes only when you choose: imports (as copies),
  tags you edit or accept, and files you tidy, re-file or reorganize.
- It keeps its database, rekordbox backups and its own rekordbox XML file in the local app data folder.
- It reads rekordbox's library only when you turn that on, and writes only behind a second switch, after a backup.
- It goes online only for lookups you ask for: MusicBrainz, the Cover Art Archive and, with your own token,
  Discogs. A Discogs or AcoustID key is stored in the library database on your computer and sent only to that
  service.
- The window is a web view that can reach only the app's own commands. It has no file system, shell or network
  access of its own, and the content security policy allows no remote resources.

## Supported versions

Only the newest version is supported. Fixes land on `main` and in the next installer.
