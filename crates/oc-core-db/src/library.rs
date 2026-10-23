//! Windowed, read-only library queries. The shapes mirror the frontend's
//! `LibraryQuery` and `LibraryPage`, so a later IPC layer maps one to the other.
//!
//! Search matches word prefixes through FTS5, ignoring case and accents; every
//! term must match. Dynamic SQL is assembled only from fixed fragments, and all
//! values are bound.

use rusqlite::types::Value;
use rusqlite::{params_from_iter, Connection};

use crate::{Error, Result};

/// The largest window one call returns.
pub const MAX_WINDOW: usize = 200;
const MAX_TERMS: usize = 8;
const MAX_TERM_CHARS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum View {
    Tracks,
    Releases,
    Artists,
}

/// `Bpm` orders tracks only; releases and artists fall back to `Title`. `Playlist` follows a
/// playlist's own order and needs `playlist_id`, `Crate` a crate's and needs `crate_id`; without
/// the id each behaves like `Recent`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sort {
    Recent,
    Title,
    Artist,
    Bpm,
    Playlist,
    Crate,
    /// The owner's own arrangement of the whole track list.
    Custom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Format {
    Aiff,
    Mp3,
    Flac,
}

impl Format {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Aiff => "AIFF",
            Self::Mp3 => "MP3",
            Self::Flac => "FLAC",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LibraryQuery {
    pub view: View,
    pub search: String,
    pub sort: Sort,
    /// Keeps rows with an active file in this container.
    pub format: Option<Format>,
    /// Tracks and releases by this artist; the artist row itself in `Artists`.
    pub artist_id: Option<i64>,
    /// Tracks of this release; the release row itself in `Releases`.
    pub release_id: Option<i64>,
    /// Tracks the rekordbox export lists in this playlist, once matched to files. Tracks only.
    pub playlist_id: Option<i64>,
    /// Tracks in one of the owner's crates. Tracks only.
    pub crate_id: Option<i64>,
}

impl LibraryQuery {
    #[must_use]
    pub fn new(view: View) -> Self {
        Self {
            view,
            search: String::new(),
            sort: Sort::Recent,
            format: None,
            artist_id: None,
            release_id: None,
            playlist_id: None,
            crate_id: None,
        }
    }
}

/// One row of any view. Fields a view does not have are `None`, empty or zero.
#[derive(Clone, Debug, PartialEq)]
pub struct LibraryItem {
    pub id: i64,
    pub kind: View,
    pub title: String,
    pub artist: String,
    pub artist_id: i64,
    pub release: String,
    pub release_id: i64,
    pub bpm: Option<f64>,
    pub musical_key: String,
    pub duration_ms: i64,
    pub format: Option<String>,
    pub track_count: i64,
    /// A track none of whose active files is currently present on disk.
    pub missing: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LibraryPage {
    pub items: Vec<LibraryItem>,
    pub total: u64,
    pub start: usize,
}

enum Search {
    None,
    /// Text was given but holds nothing a word index can match.
    Nothing,
    Match(String),
}

fn parse_search(text: &str) -> Search {
    if text.trim().is_empty() {
        return Search::None;
    }
    let terms: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|term| !term.is_empty())
        .take(MAX_TERMS)
        .map(|term| {
            let kept: String = term.chars().take(MAX_TERM_CHARS).collect();
            format!("\"{kept}\"*")
        })
        .collect();
    if terms.is_empty() {
        Search::Nothing
    } else {
        Search::Match(terms.join(" "))
    }
}

struct Filter {
    clauses: Vec<&'static str>,
    params: Vec<Value>,
}

impl Filter {
    fn new() -> Self {
        Self {
            clauses: Vec::new(),
            params: Vec::new(),
        }
    }

    fn always(&mut self, clause: &'static str) {
        self.clauses.push(clause);
    }

    fn add(&mut self, clause: &'static str, value: impl Into<Value>) {
        self.clauses.push(clause);
        self.params.push(value.into());
    }

    fn where_sql(&self) -> String {
        if self.clauses.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", self.clauses.join(" AND "))
        }
    }
}

fn empty_page(start: usize) -> LibraryPage {
    LibraryPage {
        items: Vec::new(),
        total: 0,
        start,
    }
}

/// One window of one view. `start` is a row offset into the ordered, filtered
/// list; `count` is 1 to [`MAX_WINDOW`]. The total is of the whole filtered list.
pub fn read_page(
    connection: &Connection,
    query: &LibraryQuery,
    start: usize,
    count: usize,
) -> Result<LibraryPage> {
    if count == 0 || count > MAX_WINDOW {
        return Err(Error::InvalidWindow);
    }
    let search = parse_search(&query.search);
    if matches!(search, Search::Nothing) {
        return Ok(empty_page(start));
    }
    let filter = build_filter(query, &search);
    let view = view_sql(query);
    let where_sql = filter.where_sql();
    let total: i64 = connection.query_row(
        &format!("SELECT COUNT(*) FROM {}{where_sql}", view.table),
        params_from_iter(&filter.params),
        |row| row.get(0),
    )?;
    let sql = format!(
        "SELECT {} FROM {}{where_sql} ORDER BY {} LIMIT ? OFFSET ?",
        view.columns, view.joined, view.order
    );
    let mut params = filter.params;
    params.extend(view.order_playlist.map(Value::Integer));
    params.push(Value::Integer(
        i64::try_from(count).map_err(|_| Error::InvalidWindow)?,
    ));
    params.push(Value::Integer(
        i64::try_from(start).map_err(|_| Error::InvalidWindow)?,
    ));
    let mut statement = connection.prepare_cached(&sql)?;
    let kind = query.view;
    let items = statement
        .query_map(params_from_iter(&params), |row| item_from_row(kind, row))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(LibraryPage {
        items,
        total: u64::try_from(total).unwrap_or(0),
        start,
    })
}

fn track_filter(filter: &mut Filter, query: &LibraryQuery, search: &Search) {
    if let Search::Match(expression) = search {
        filter.add(
            "t.id IN (SELECT rowid FROM track_fts WHERE track_fts MATCH ?)",
            expression.clone(),
        );
    }
    if let Some(id) = query.artist_id {
        filter.add("t.artist_id = ?", id);
    }
    if let Some(id) = query.release_id {
        filter.add("t.release_id = ?", id);
    }
    if let Some(id) = query.playlist_id {
        filter.add(
            "t.id IN (SELECT f.track_id FROM playlist_item i \
             JOIN playlist p ON p.id = i.playlist_id \
             JOIN rb_track r ON r.rb_source_id = p.rb_source_id AND r.rb_key = i.rb_key \
             JOIN file f ON f.id = r.file_id \
             WHERE i.playlist_id = ? AND f.track_id IS NOT NULL)",
            id,
        );
    }
    if let Some(id) = query.crate_id {
        filter.add(
            "t.id IN (SELECT track_id FROM crate_item WHERE crate_id = ?)",
            id,
        );
    }
    if let Some(format) = query.format {
        filter.add(
            "EXISTS (SELECT 1 FROM file f WHERE f.track_id = t.id \
             AND f.lifecycle = 'active' AND f.container = ?)",
            format.as_str().to_owned(),
        );
    }
}

fn release_filter(filter: &mut Filter, query: &LibraryQuery, search: &Search) {
    filter.always("EXISTS (SELECT 1 FROM track b WHERE b.release_id = r.id)");
    if let Search::Match(expression) = search {
        filter.add(
            "r.id IN (SELECT rowid FROM release_fts WHERE release_fts MATCH ?)",
            expression.clone(),
        );
    }
    if let Some(id) = query.artist_id {
        filter.add("r.artist_id = ?", id);
    }
    if let Some(id) = query.release_id {
        filter.add("r.id = ?", id);
    }
    if let Some(format) = query.format {
        filter.add(
            "EXISTS (SELECT 1 FROM track t JOIN file f ON f.track_id = t.id \
             WHERE t.release_id = r.id AND f.lifecycle = 'active' AND f.container = ?)",
            format.as_str().to_owned(),
        );
    }
}

fn artist_filter(filter: &mut Filter, query: &LibraryQuery, search: &Search) {
    filter.always("EXISTS (SELECT 1 FROM track b WHERE b.artist_id = a.id)");
    if let Search::Match(expression) = search {
        filter.add(
            "a.id IN (SELECT rowid FROM artist_fts WHERE artist_fts MATCH ?)",
            expression.clone(),
        );
    }
    if let Some(id) = query.artist_id {
        filter.add("a.id = ?", id);
    }
    if let Some(format) = query.format {
        filter.add(
            "EXISTS (SELECT 1 FROM track t JOIN file f ON f.track_id = t.id \
             WHERE t.artist_id = a.id AND f.lifecycle = 'active' AND f.container = ?)",
            format.as_str().to_owned(),
        );
    }
}

struct ViewSql {
    /// The filtered table alone, for counting and id lists.
    table: &'static str,
    /// The table plus what a row needs to show.
    joined: &'static str,
    columns: &'static str,
    order: &'static str,
    /// The playlist id bound in `order`, when it follows a playlist.
    order_playlist: Option<i64>,
    alias: &'static str,
}

/// Arranged tracks by their position, then the rest newest first.
const CUSTOM_ORDER: &str =
    "(SELECT o.position FROM library_order o WHERE o.track_id = t.id) IS NULL, \
     (SELECT o.position FROM library_order o WHERE o.track_id = t.id), t.added_ms DESC, t.id DESC";

/// Every track id in the order `sort` shows them, unfiltered. Only the library's own sorts:
/// playlist and crate orders need an id and are not a whole-library order.
pub fn library_order_ids(connection: &Connection, sort: Sort) -> Result<Vec<i64>> {
    if matches!(sort, Sort::Playlist | Sort::Crate) {
        return Err(Error::InvalidArrangeSort);
    }
    let mut query = LibraryQuery::new(View::Tracks);
    query.sort = sort;
    let order = view_sql(&query).order;
    let mut statement =
        connection.prepare(&format!("SELECT t.id FROM track t ORDER BY {order}"))?;
    let ids = statement
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<i64>>>()?;
    Ok(ids)
}

fn view_sql(query: &LibraryQuery) -> ViewSql {
    match query.view {
        View::Tracks => ViewSql {
            table: "track t",
            joined:
                "track t JOIN artist a ON a.id = t.artist_id JOIN release r ON r.id = t.release_id",
            columns:
                "t.id, t.title, a.name, t.artist_id, r.title, t.release_id, t.bpm, t.musical_key, \
                      t.duration_ms, (SELECT f.container FROM file f WHERE f.track_id = t.id \
                      AND f.lifecycle = 'active' ORDER BY f.id LIMIT 1), 1, \
                      (EXISTS (SELECT 1 FROM file f WHERE f.track_id = t.id AND f.lifecycle = 'active') \
                       AND NOT EXISTS (SELECT 1 FROM file f WHERE f.track_id = t.id \
                                       AND f.lifecycle = 'active' AND f.presence = 'present'))",
            order: match (query.sort, query.playlist_id) {
                (Sort::Playlist, Some(_)) => {
                    "(SELECT MIN(i.position) FROM playlist_item i \
                     JOIN playlist p ON p.id = i.playlist_id \
                     JOIN rb_track r ON r.rb_source_id = p.rb_source_id AND r.rb_key = i.rb_key \
                     JOIN file f ON f.id = r.file_id \
                     WHERE i.playlist_id = ? AND f.track_id = t.id), t.id"
                }
                (Sort::Crate, _) if query.crate_id.is_some() => {
                    "(SELECT c.position FROM crate_item c WHERE c.crate_id = ? \
                     AND c.track_id = t.id), t.id"
                }
                (Sort::Custom, _) => CUSTOM_ORDER,
                (Sort::Recent | Sort::Playlist | Sort::Crate, _) => "t.added_ms DESC, t.id DESC",
                (Sort::Title, _) => "t.title_key, t.id",
                (Sort::Artist, _) => "t.artist_key, t.release_id, t.disc_no, t.track_no, t.id",
                (Sort::Bpm, _) => "t.bpm IS NULL, t.bpm, t.id",
            },
            order_playlist: match query.sort {
                Sort::Playlist => query.playlist_id,
                Sort::Crate => query.crate_id,
                _ => None,
            },
            alias: "t",
        },
        View::Releases => ViewSql {
            table: "release r",
            joined: "release r JOIN artist a ON a.id = r.artist_id",
            columns: "r.id, r.title, a.name, r.artist_id, r.title, r.id, NULL, '', 0, NULL, \
                      (SELECT COUNT(*) FROM track t WHERE t.release_id = r.id), 0",
            order: match query.sort {
                Sort::Recent | Sort::Playlist | Sort::Crate | Sort::Custom => {
                    "r.added_ms DESC, r.id DESC"
                }
                Sort::Artist => "r.artist_key, r.sort_key, r.id",
                Sort::Title | Sort::Bpm => "r.sort_key, r.id",
            },
            order_playlist: None,
            alias: "r",
        },
        View::Artists => ViewSql {
            table: "artist a",
            joined: "artist a",
            columns: "a.id, a.name, a.name, a.id, '', 0, NULL, '', 0, NULL, \
                      (SELECT COUNT(*) FROM track t WHERE t.artist_id = a.id), 0",
            order: match query.sort {
                Sort::Recent | Sort::Playlist | Sort::Crate | Sort::Custom => "a.id DESC",
                Sort::Title | Sort::Artist | Sort::Bpm => "a.sort_key, a.id",
            },
            order_playlist: None,
            alias: "a",
        },
    }
}

fn build_filter(query: &LibraryQuery, search: &Search) -> Filter {
    let mut filter = Filter::new();
    match query.view {
        View::Tracks => track_filter(&mut filter, query, search),
        View::Releases => release_filter(&mut filter, query, search),
        View::Artists => artist_filter(&mut filter, query, search),
    }
    filter
}

fn item_from_row(kind: View, row: &rusqlite::Row<'_>) -> rusqlite::Result<LibraryItem> {
    Ok(LibraryItem {
        id: row.get(0)?,
        kind,
        title: row.get(1)?,
        artist: row.get(2)?,
        artist_id: row.get(3)?,
        release: row.get(4)?,
        release_id: row.get(5)?,
        bpm: row.get(6)?,
        musical_key: row.get(7)?,
        duration_ms: row.get(8)?,
        format: row.get(9)?,
        track_count: row.get(10)?,
        missing: row.get::<_, i64>(11)? != 0,
    })
}

/// Ids of the whole filtered list, in display order.
fn ordered_ids(connection: &Connection, query: &LibraryQuery, search: &Search) -> Result<Vec<i64>> {
    let filter = build_filter(query, search);
    let view = view_sql(query);
    let sql = format!(
        "SELECT {alias}.id FROM {}{} ORDER BY {}",
        view.table,
        filter.where_sql(),
        view.order,
        alias = view.alias
    );
    let mut params = filter.params;
    params.extend(view.order_playlist.map(Value::Integer));
    let mut statement = connection.prepare_cached(&sql)?;
    let ids = statement
        .query_map(params_from_iter(&params), |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<i64>>>()?;
    Ok(ids)
}

fn fetch_items(
    connection: &Connection,
    query: &LibraryQuery,
    ids: &[i64],
) -> Result<Vec<LibraryItem>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let view = view_sql(query);
    let placeholders = vec!["?"; ids.len()].join(", ");
    let sql = format!(
        "SELECT {} FROM {} WHERE {}.id IN ({placeholders})",
        view.columns, view.joined, view.alias
    );
    let mut statement = connection.prepare_cached(&sql)?;
    let kind = query.view;
    let mut found: std::collections::HashMap<i64, LibraryItem> = statement
        .query_map(params_from_iter(ids), |row| item_from_row(kind, row))?
        .map(|item| item.map(|item| (item.id, item)))
        .collect::<rusqlite::Result<_>>()?;
    Ok(ids.iter().filter_map(|id| found.remove(id)).collect())
}

/// Lists kept per reader; each holds one id per row, about 800 KB for 100,000 tracks.
const CACHED_LISTS: usize = 4;

/// A read-only connection that remembers the ordered id list of its recent queries.
/// The first window of a query pays one pass over the filtered list; later windows
/// (scrolling) cost only the rows in the window. A commit by the writer is noticed
/// through SQLite's `data_version` and drops the lists.
pub struct LibraryReader {
    connection: Connection,
    data_version: i64,
    lists: std::collections::VecDeque<(LibraryQuery, Vec<i64>)>,
}

impl LibraryReader {
    pub fn new(connection: Connection) -> Result<Self> {
        let data_version = Self::version(&connection)?;
        Ok(Self {
            connection,
            data_version,
            lists: std::collections::VecDeque::new(),
        })
    }

    fn version(connection: &Connection) -> Result<i64> {
        Ok(connection.pragma_query_value(None, "data_version", |row| row.get(0))?)
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn clear_cache(&mut self) {
        self.lists.clear();
    }

    pub fn read_page(
        &mut self,
        query: &LibraryQuery,
        start: usize,
        count: usize,
    ) -> Result<LibraryPage> {
        if count == 0 || count > MAX_WINDOW {
            return Err(Error::InvalidWindow);
        }
        let search = parse_search(&query.search);
        if matches!(search, Search::Nothing) {
            return Ok(empty_page(start));
        }
        let version = Self::version(&self.connection)?;
        if version != self.data_version {
            self.lists.clear();
            self.data_version = version;
        }
        let position = self.lists.iter().position(|(cached, _)| cached == query);
        let entry = match position {
            Some(index) => self.lists.remove(index),
            None => None,
        };
        let (cached, ids) = match entry {
            Some(entry) => entry,
            None => (
                query.clone(),
                ordered_ids(&self.connection, query, &search)?,
            ),
        };
        let total = ids.len();
        let window = ids.get(start..).unwrap_or_default();
        let window = &window[..window.len().min(count)];
        let items = fetch_items(&self.connection, query, window)?;
        self.lists.push_front((cached, ids));
        self.lists.truncate(CACHED_LISTS);
        Ok(LibraryPage {
            items,
            total: u64::try_from(total).unwrap_or(0),
            start,
        })
    }
}

/// Case-folded sort text for titles and names. Display values are stored as given;
/// this is a metadata key and is never used for file paths.
#[must_use]
pub fn sort_key(text: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    text.trim().nfc().collect::<String>().to_lowercase()
}

/// One folder or playlist from a rekordbox export, with how many of its entries are in the library.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaylistRow {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub is_folder: bool,
    pub name: String,
    pub position: i64,
    /// Entries in the playlist, duplicates included.
    pub entries: i64,
    /// Distinct tracks of this playlist that match a file in the library.
    pub in_library: i64,
}

/// Every playlist and folder of every mirrored export, parents before children is not
/// guaranteed; use `parent_id` to nest. Ordered by export, parent and position.
pub fn read_playlists(connection: &Connection) -> Result<Vec<PlaylistRow>> {
    let mut statement = connection.prepare(
        "SELECT p.id, p.parent_id, p.kind, p.name, p.position, \
                (SELECT COUNT(*) FROM playlist_item i WHERE i.playlist_id = p.id), \
                (SELECT COUNT(DISTINCT f.track_id) FROM playlist_item i \
                   JOIN rb_track r ON r.rb_source_id = p.rb_source_id AND r.rb_key = i.rb_key \
                   JOIN file f ON f.id = r.file_id \
                  WHERE i.playlist_id = p.id AND f.track_id IS NOT NULL) \
         FROM playlist p ORDER BY p.rb_source_id, p.parent_id, p.position",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(PlaylistRow {
            id: row.get(0)?,
            parent_id: row.get(1)?,
            is_folder: row.get::<_, String>(2)? == "folder",
            name: row.get(3)?,
            position: row.get(4)?,
            entries: row.get(5)?,
            in_library: row.get(6)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

#[cfg(test)]
mod tests;
