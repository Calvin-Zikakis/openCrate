# openCrate user guide

openCrate keeps a DJ's music library tidy: it brings new downloads in cleanly, shows your whole library, plays it,
and works alongside rekordbox without breaking anything you have done there.

Three promises hold everywhere in the app:

- **Your download folders are never changed.** Importing copies files; the originals stay where they are.
- **Your music folder is only read**, except for files openCrate itself files there during an import.
- **rekordbox's own library is never written** by openCrate, except when you restore one of its backups yourself.

## First start

The first time openCrate opens, a welcome panel offers three steps:

1. **Your music folder.** Pick the folder your library lives in. openCrate reads every file's tags and shows the
   progress. Nothing is moved or renamed.
2. **Download folders.** Pick the folders Bandcamp, Soulseek and other downloads land in. Releases already there are
   counted as old; new ones show on the Import screen.
3. **rekordbox.** Opens the rekordbox screen, described below.

"Done for now" closes the panel. Every step can be done or changed later in Settings.

openCrate keeps its own data (its database, rekordbox backups and its rekordbox file) in the computer's application
data folder, never in your music folder.

## Library: Tracks, Releases and Artists

- **Search** with the box at the top, or press `/`. It matches titles, artists and releases, from the first letters of
  each word.
- **Filters** at the top of Tracks show only AIFF, MP3 or FLAC files. **Sort** by recently added, title, artist or BPM.
- **Compact** fits more rows on screen.
- Open a release or an artist to see its tracks. A track whose file cannot be found is marked **Missing**; one on a
  drive that is unplugged is shown as offline, not missing.
- **Custom order:** drag tracks to arrange the list by hand, or move the focused track with Alt+Up and Alt+Down.

## Playing

- Double-click a track, or select it and press Enter or Space. Space pauses and plays again.
- **Previous** goes back to the start of the track, or to the track before when pressed near the start. **Next** plays
  the following track in the list; at the end of a track the next one starts by itself, and tracks that cannot be
  played are skipped.
- **The waveform.** The strip across the top of the player scrolls past the playhead in the middle and shows ten
  seconds of the track coloured by sound: lows in blue, mids in orange, highs in white. The bar under the transport
  shows the whole track; click or drag it to move, or use the arrow keys once it has focus. Cue points from rekordbox are
  marked on both: green for hot cues, white for memory cues.
- **Cues and grid** lists the cues and beat grid rekordbox has for the selected track.
- The volume is remembered.

## Crates

Crates are your own lists, kept in openCrate. Make one with the + beside CRATES, then drag tracks onto its name.
Reorder by dragging or with Alt+Up and Alt+Down; rename or delete from its menu. Deleting a crate leaves the tracks in
your library. A crate can be sent to rekordbox as a playlist (see below).

## Import

The Import screen lists the releases in your download folders and whether each has finished downloading.

- **Preview...** shows, for every file, where it would be filed, which tags it would get, and whether it would be
  converted. Nothing is copied by a preview.
- **Import** copies the release into your music folder, in the folder layout chosen in Settings, and reads it into the
  library. The release then shows as Imported.
- **Import all** imports every finished release. It first lists the tags it is unsure about (guessed from file names,
  or missing) so you can check them, and can **look them up** on MusicBrainz. A lookup happens only when you press the
  button, and only the values you tick are used.
- **Conversion:** "Convert lossless files to" and "Convert lossy files to" choose AIFF, WAV, FLAC or MP3, or leave files
  as they are. A file already in the chosen format is never converted. Converting needs ffmpeg on the computer; without
  it, files go in as they are. A lossless file that looks like it was made from an MP3 is flagged in the preview.
- **Tidy names** removes stray separators such as the ";" in "Artist;" and splits "A;B" into "A, B" before folders and
  file names are made. On by default.
- Tags are written as ID3v2.3, which rekordbox reads.
- A remix or an edit credited to another artist is filed under that artist's folder; a VIP stays with the original
  artist.

## rekordbox

### Reading rekordbox

openCrate can learn about your rekordbox library in two ways.

- **An XML export.** In rekordbox choose File, then Export Collection in xml format, then pick the file with
  **Choose export...**. openCrate reads tempo, key, cues, beat grids and your playlists, and shows the playlists in the
  sidebar. **Read again** picks up a newer export; the screen says when the file has changed since it was read.
- **Directly**, once switched on in Settings (see below). **Read from rekordbox** takes a backup of rekordbox's library
  and reads the copy. rekordbox must be closed, including its background agent. This also brings in My Tags.

If entries point at your music under another drive letter or folder, openCrate suggests the folder they belong to; **Use
this** accepts a suggestion.

### New in your library

Tracks openCrate filed that rekordbox does not have yet. Tick the ones to send and press **Send to rekordbox**. They go
into openCrate's own rekordbox file in a playlist named "openCrate new tracks". Then, in rekordbox:

1. The first time only: Preferences, Advanced, rekordbox xml: choose openCrate's file (its path is shown) as the
   Imported Library.
2. In rekordbox's tree, open rekordbox xml, then "openCrate new tracks". Refresh it if it was open.
3. Select its tracks and drag them onto your Imported playlist.

Sent tracks are marked "Sent, waiting for rekordbox" until the next read shows rekordbox has them. **Hide** keeps a
track out of rekordbox for good; "Show hidden tracks" and **Bring back** undo it. Read rekordbox before sending, so
tracks it already has are left out; the card says when the last read is older than the newest import.

### Crates for rekordbox

Switch a crate on and it goes into the same openCrate file as a playlist inside a folder named "openCrate crates". Tracks
rekordbox already has are written exactly as rekordbox exported them, so their cues, grids and tags stay as they are.
After changing a crate, press **Update file**. In rekordbox, open rekordbox xml, then "openCrate crates", and drag a
playlist into your own playlists.

Tracks rekordbox knows only from a direct read are left out until an XML export has been read; tracks whose files cannot
be reached are left out too. The card says how many.

These rekordbox steps are still to be confirmed on a real rekordbox.

## Settings

- **Appearance:** theme (match this computer, dark or light), compact rows, reduced motion.
- **Music folders** and **Download folders:** add, read again, switch off or remove. Removing a folder removes it from
  openCrate only; no file is touched.
- **Folder layout:** how imported music is arranged, Artist / Album / Track by default, or your own pattern. Music
  already in your folder is not moved.
- **Library check:** lists folder names made from untidy tags and recordings filed more than once. It only reads.
- **rekordbox:** "Let openCrate read rekordbox's library directly", off by default. Turning it on shows a warning first;
  if rekordbox changes version, it asks again. **Backups of rekordbox's library** lists every backup openCrate took (the
  last 20 are kept, plus any you pin). **Restore...** puts one back after asking; the library it replaces is saved first,
  so a restore can itself be undone. rekordbox must be closed.
