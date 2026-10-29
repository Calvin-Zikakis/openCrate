# oc-organize

The naming rules for the library: folder layouts (path templates), file and folder names that are valid on every
system, Windows' path length limit, and reading a title's version and credits. It has no file system access and no
database; the desktop app uses it to decide where a file belongs.

## Public API

| Item | Purpose |
| --- | --- |
| `Template::parse(text)`, `Template::render(fields, extension)` | A folder layout such as `{folder_artist}/{release}/{track:02} - {artist} - {title}`; each `/` starts a folder |
| `DEFAULT_TEMPLATE` | The layout above: artist, release, then the numbered track |
| `Field` | What a template can use: folder artist, release, artist, album artist, remixer, title, year, track and disc |
| `destination(...) -> Destination` | Render the template for one track and fit it inside a library root |
| `sanitize_component`, `sanitize_file_name`, `sanitize_extension` | Names valid on Windows, macOS and Linux at once |
| `fit_destination(...)`, `PathBudget::classic()`, `PathBudget::long_paths()` | Keep a whole path within Windows' limit: 259 UTF-16 units, or 32,000 with long paths on |
| `parse_title(raw) -> ParsedTitle` | Split `Night Drive (Studio 001 Remix) (feat. Guest)` into title, version and credits |
| `folder_artist(parsed, album_artist, artist)` | Whose folder a track goes in: the remixer's for a remix, edit, dub, flip, bootleg or rework; the artist's own for a VIP |
| `same_version`, `version_key` | Compare versions, where no version means "Original Mix" |

## Behaviour and limits

- Forbidden characters are replaced with `_`, never deleted, so words are not run together. Windows device names
  (`CON`, `COM1` and the rest) are defused, nothing ends in a dot or a space, and a name is cut to at most 100
  characters and 240 bytes. Applying the rules twice changes nothing more.
- Only the file name can give way to fit the path limit, and 16 units are kept spare for a temporary suffix. When
  the root or the folders alone are too long, the answer is an error rather than a broken path.
- A character outside the Basic Multilingual Plane counts twice toward the Windows limit, as Windows counts it.

## Validation

From the workspace root:

```sh
cargo test -p oc-organize --locked
cargo clippy -p oc-organize --all-targets --locked -- -D warnings
```

The tests use synthetic names only.
