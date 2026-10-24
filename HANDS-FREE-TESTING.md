# Letting Claude drive the app on its own

What this gives: Claude can start openCrate and rekordbox on this Mac, click through them, read the
screen, and test with real music, without anyone at the keyboard. Setup is a few minutes once; after
that nothing is needed from you.

## 1. Let the terminal control the screen (one time, by you)

macOS only lets an app click and type for other apps once you allow it. Claude runs inside whichever
app hosts the session, so that app needs two permissions.

1. System Settings, Privacy and Security, Accessibility: turn on **Visual Studio Code** (or
   Terminal, iTerm, or the Claude app, whichever you run Claude Code in).
2. System Settings, Privacy and Security, Screen Recording: the same app. (Screenshots already work
   here, so this may be on already.)
3. The first time Claude controls another app, macOS asks "Visual Studio Code wants access to
   control System Events" (or "control openCrate"). Choose Allow. If you chose Don't Allow earlier,
   turn it on under Privacy and Security, Automation.

Quit and reopen the app after changing these; the permission is read at launch.

How Claude then drives things: `cliclick` (installed with Homebrew) for clicks and typing by screen
position, `screencapture` to see the result, and AppleScript through System Events to read window
titles and buttons. The browser tests still cover the page; this is for the real app, with real
files and the real rekordbox.

## 2. Music Claude may change

Claude never touches your real library on the desktop or the NAS, and keeps track names from your
library out of the repository. For hands-free testing it needs music it is allowed to import,
retag, move and archive. Make a scratch area on this Mac:

```
~/Desktop/ocTest/
  Music/       an empty music folder for openCrate to file into
  Downloads/   copies of a few releases, as they land from Bandcamp or Soulseek
```

Copy, do not move: whatever is in `ocTest` may be rewritten. A dozen releases is plenty, with a mix
of lossless and MP3, a compilation, something with untidy tags ("Artist;") and a vinyl rip with A
and B sides. Tell Claude the folder exists and it will add both folders in Settings itself.

The laptop's rekordbox is already a test instance (you created it for this), so Claude may read and
write it. Every write takes a backup first, listed in Settings; `docs/LOG.md` says how to restore
by hand if ever needed.

## 3. Starting the apps

openCrate: `open /Applications/openCrate.app`, or the latest build at
`target/release/bundle/macos/openCrate.app`. For the page's own dev tools, `npm run tauri dev` from
the repository runs it against the local source.

rekordbox: `env -u ELECTRON_RUN_AS_NODE open "/Applications/rekordbox 7/rekordbox.app"`. The
variable matters: VS Code's terminal sets `ELECTRON_RUN_AS_NODE=1`, rekordbox passes it to its
background agent (an Electron app), and the agent then dies with "Communication with
rekordboxAgent failed". Quit rekordbox before any openCrate read or write of its library.

## 4. What Claude will do with it

- Import the scratch downloads, with and without conversion, and check where files land.
- Check the editor and Clean up on real tags, then that rekordbox sees the changes after a sync.
- Drop tracks on rekordbox playlists, resolve duplicates, tidy folders, and open rekordbox to
  confirm each one by eye.
- Screenshot every screen and fix what looks wrong.

Anything it finds goes in `docs/owner-notes.md`, as now.
