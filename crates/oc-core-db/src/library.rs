//! Windowed, read-only library queries. The shapes mirror the frontend's
//! `LibraryQuery` and `LibraryPage`, so the desktop app's IPC layer maps one to the other.
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
    /// Release by release, each in disc and track order: how a release or an artist reads.
    Track,
    /// By the file's container (AIFF, FLAC, MP3...), then title. Tracks only; releases and
    /// artists fall back to `Title`.
    Format,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LibraryQuery {
    pub view: View,
    pub search: String,
    pub sort: Sort,
    /// Keeps rows with an active file in this container, named as the file table names it
    /// ("AIFF", "MP3", "M4A"...).
    pub format: Option<String>,
    /// Tracks and releases by this artist; the artist row itself in `Artists`.
    pub artist_id: Option<i64>,
    /// Tracks of this release; the release row itself in `Releases`.
    pub release_id: Option<i64>,
    /// Tracks the rekordbox export lists in this playlist, once matched to files. Tracks only.
    pub playlist_id: Option<i64>,
    /// Tracks in one of the owner's crates. Tracks only.
    pub crate_id: Option<i64>,
    /// Tracks filed by one import batch. Tracks only.
    pub batch_id: Option<i64>,
    /// Tracks whose genre is spelled exactly so (releases and artists with such a track).
    pub genre: Option<String>,
    /// Tracks whose year lies in this range, both ends included.
    pub years: Option<(i32, i32)>,
    /// Tracks added to the library in this span of milliseconds since the epoch, both ends
    /// included.
    pub added: Option<(i64, i64)>,
    /// The sort the other way round: oldest first, Z to A, BPM high to low. Rows without a
    /// value (no BPM) stay last either way. Playlist, crate and custom orders are never
    /// reversed: they are the owner's own arrangement, and moving a row relies on it.
    pub reversed: bool,
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
            batch_id: None,
            genre: None,
            years: None,
            added: None,
            reversed: false,
        }
    }

    /// Whether this query reads the other way round: see [`Self::reversed`].
    #[must_use]
    pub const fn reads_reversed(&self) -> bool {
        let arranged = match self.sort {
            Sort::Custom => true,
            Sort::Playlist => self.playlist_id.is_some(),
            Sort::Crate => self.crate_id.is_some(),
            _ => false,
        };
        self.reversed && !arranged
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
    /// rekordbox's BPM once it has analysed the track, else the file's tag, if any.
    pub bpm: Option<f64>,
    /// rekordbox's key once it has one, else the file's tag.
    pub musical_key: String,
    pub duration_ms: i64,
    pub format: Option<String>,
    pub track_count: i64,
    /// A track none of whose active files is currently present on disk.
    pub missing: bool,
    /// rekordbox has analysed the track: `bpm` is its own, not the tag's.
    pub analysed: bool,
    /// `musical_key` is rekordbox's, not the tag's.
    pub rekordbox_key: bool,
    /// Tracks only; empty or `None` for a release or an artist.
    pub genre: String,
    pub label: String,
    pub year: Option<i32>,
    /// When the track joined the library.
    pub added_ms: i64,
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

    /// A clause with the same value in each of its places.
    fn add_each(&mut self, clause: &'static str, value: &str) {
        self.clauses.push(clause);
        for _ in 0..clause.matches('?').count() {
            self.params.push(value.to_owned().into());
        }
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
        view.columns,
        view.joined,
        order_sql(&view, query)
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

/// The genre, year and date-added conditions on a track alias, for every view.
fn tag_clauses(filter: &mut Filter, alias: &'static str, query: &LibraryQuery) {
    if let Some(genre) = &query.genre {
        filter.add(
            match alias {
                "t" => "t.genre = ?",
                "r" => "r.id IN (SELECT g.release_id FROM track g WHERE g.genre = ?)",
                _ => "a.id IN (SELECT g.artist_id FROM track g WHERE g.genre = ?)",
            },
            genre.clone(),
        );
    }
    if let Some((from, to)) = query.years {
        let clause = match alias {
            "t" => "t.year BETWEEN ? AND ?",
            "r" => "r.id IN (SELECT y.release_id FROM track y WHERE y.year BETWEEN ? AND ?)",
            _ => "a.id IN (SELECT y.artist_id FROM track y WHERE y.year BETWEEN ? AND ?)",
        };
        filter.clauses.push(clause);
        filter.params.push(from.into());
        filter.params.push(to.into());
    }
    if let Some((from, to)) = query.added {
        let clause = match alias {
            "t" => "t.added_ms BETWEEN ? AND ?",
            "r" => "r.id IN (SELECT d.release_id FROM track d WHERE d.added_ms BETWEEN ? AND ?)",
            _ => "a.id IN (SELECT d.artist_id FROM track d WHERE d.added_ms BETWEEN ? AND ?)",
        };
        filter.clauses.push(clause);
        filter.params.push(from.into());
        filter.params.push(to.into());
    }
}

fn track_filter(filter: &mut Filter, query: &LibraryQuery, search: &Search) {
    tag_clauses(filter, "t", query);
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
    if let Some(id) = query.batch_id {
        filter.add(
            "t.id IN (SELECT f.track_id FROM import_file b JOIN file f \
             ON f.root_id = b.root_id AND f.path_key = b.path_key \
             WHERE b.batch_id = ? AND f.lifecycle = 'active' AND f.track_id IS NOT NULL)",
            id,
        );
    }
    if let Some(format) = &query.format {
        filter.add(
            "t.id IN (SELECT f.track_id FROM file f \
             WHERE f.lifecycle = 'active' AND f.container = ?)",
            format.clone(),
        );
    }
}

fn release_filter(filter: &mut Filter, query: &LibraryQuery, search: &Search) {
    // `IN`, not a correlated `EXISTS`: SQLCipher's SQLite 3.51.3 applies OFFSET wrongly to the
    // latter.
    filter.always("r.id IN (SELECT b.release_id FROM track b)");
    tag_clauses(filter, "r", query);
    if let Search::Match(expression) = search {
        // A release shows for a track of its that matches, as well as for its own name.
        filter.add_each(
            "(r.id IN (SELECT rowid FROM release_fts WHERE release_fts MATCH ?) \
             OR r.id IN (SELECT m.release_id FROM track m \
             WHERE m.id IN (SELECT rowid FROM track_fts WHERE track_fts MATCH ?)))",
            expression,
        );
    }
    if let Some(id) = query.artist_id {
        filter.add("r.artist_id = ?", id);
    }
    if let Some(id) = query.release_id {
        filter.add("r.id = ?", id);
    }
    if let Some(format) = &query.format {
        filter.add(
            "r.id IN (SELECT t.release_id FROM track t JOIN file f ON f.track_id = t.id \
             WHERE f.lifecycle = 'active' AND f.container = ?)",
            format.clone(),
        );
    }
}

fn artist_filter(filter: &mut Filter, query: &LibraryQuery, search: &Search) {
    filter.always("a.id IN (SELECT b.artist_id FROM track b)");
    tag_clauses(filter, "a", query);
    if let Search::Match(expression) = search {
        // An artist shows for a track or release of theirs that matches, as well as for the name.
        filter.add_each(
            "(a.id IN (SELECT rowid FROM artist_fts WHERE artist_fts MATCH ?) \
             OR a.id IN (SELECT m.artist_id FROM track m \
             WHERE m.id IN (SELECT rowid FROM track_fts WHERE track_fts MATCH ?)))",
            expression,
        );
    }
    if let Some(id) = query.artist_id {
        filter.add("a.id = ?", id);
    }
    if let Some(format) = &query.format {
        filter.add(
            "a.id IN (SELECT t.artist_id FROM track t JOIN file f ON f.track_id = t.id \
             WHERE f.lifecycle = 'active' AND f.container = ?)",
            format.clone(),
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
                "t.id, t.title, a.name, t.artist_id, r.title, t.release_id, COALESCE(t.bpm, t.tag_bpm), \
                      CASE WHEN t.musical_key <> '' THEN t.musical_key ELSE t.tag_key END, \
                      t.duration_ms, (SELECT f.container FROM file f WHERE f.track_id = t.id \
                      AND f.lifecycle = 'active' ORDER BY f.id LIMIT 1), 1, \
                      (EXISTS (SELECT 1 FROM file f WHERE f.track_id = t.id AND f.lifecycle = 'active') \
                       AND NOT EXISTS (SELECT 1 FROM file f WHERE f.track_id = t.id \
                                       AND f.lifecycle = 'active' AND f.presence = 'present')), \
                      t.bpm IS NOT NULL, t.musical_key <> '', t.genre, t.label, t.year, t.added_ms",
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
                // The release by subquery: some callers order `track t` alone, with no join.
                (Sort::Track, _) => {
                    "(SELECT o.sort_key FROM release o WHERE o.id = t.release_id), t.release_id, \
                     t.disc_no, t.track_no, t.title_key, t.id"
                }
                (Sort::Bpm, _) => {
                    "COALESCE(t.bpm, t.tag_bpm) IS NULL, COALESCE(t.bpm, t.tag_bpm), t.id"
                }
                (Sort::Format, _) => {
                    "(SELECT f.container FROM file f WHERE f.track_id = t.id \
                     AND f.lifecycle = 'active' ORDER BY f.id LIMIT 1), t.title_key, t.id"
                }
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
                      (SELECT COUNT(*) FROM track t WHERE t.release_id = r.id), 0, 0, 0, '', '', NULL, \
                      r.added_ms",
            order: match query.sort {
                Sort::Recent | Sort::Playlist | Sort::Crate | Sort::Custom => {
                    "r.added_ms DESC, r.id DESC"
                }
                Sort::Artist => "r.artist_key, r.sort_key, r.id",
                Sort::Title | Sort::Bpm | Sort::Track | Sort::Format => "r.sort_key, r.id",
            },
            order_playlist: None,
            alias: "r",
        },
        View::Artists => ViewSql {
            table: "artist a",
            joined: "artist a",
            columns: "a.id, a.name, a.name, a.id, '', 0, NULL, '', 0, NULL, \
                      (SELECT COUNT(*) FROM track t WHERE t.artist_id = a.id), 0, 0, 0, '', '', NULL, 0",
            order: match query.sort {
                Sort::Recent | Sort::Playlist | Sort::Crate | Sort::Custom => "a.id DESC",
                Sort::Title | Sort::Artist | Sort::Bpm | Sort::Track | Sort::Format => {
                    "a.sort_key, a.id"
                }
            },
            order_playlist: None,
            alias: "a",
        },
    }
}

/// The ORDER BY terms of a view, turned round when the query asks for it.
fn order_sql(view: &ViewSql, query: &LibraryQuery) -> std::borrow::Cow<'static, str> {
    if query.reads_reversed() {
        std::borrow::Cow::Owned(reversed_order(view.order))
    } else {
        std::borrow::Cow::Borrowed(view.order)
    }
}

/// `order` with each term's direction flipped. Terms are split at the commas outside
/// parentheses, since some are subqueries. A term that only puts rows without a value last
/// ("... IS NULL") keeps its direction, so those rows stay last either way round.
fn reversed_order(order: &str) -> String {
    let mut terms = Vec::new();
    let mut depth = 0_u32;
    let mut start = 0;
    for (at, character) in order.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                terms.push(&order[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    terms.push(&order[start..]);
    terms
        .into_iter()
        .map(|term| {
            let term = term.split_whitespace().collect::<Vec<_>>().join(" ");
            if term.ends_with(" IS NULL") {
                term
            } else if let Some(ascending) = term.strip_suffix(" DESC") {
                ascending.to_owned()
            } else {
                format!("{term} DESC")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
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
        analysed: row.get::<_, i64>(12)? != 0,
        rekordbox_key: row.get::<_, i64>(13)? != 0,
        genre: row.get(14)?,
        label: row.get(15)?,
        year: row.get(16)?,
        added_ms: row.get(17)?,
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
        order_sql(&view, query),
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

    /// Every id of the filtered list, in the order it reads: what "select all" selects.
    pub fn ids(&mut self, query: &LibraryQuery) -> Result<Vec<i64>> {
        let search = parse_search(&query.search);
        if matches!(search, Search::Nothing) {
            return Ok(Vec::new());
        }
        let version = Self::version(&self.connection)?;
        if version != self.data_version {
            self.lists.clear();
            self.data_version = version;
        }
        if let Some((_, ids)) = self.lists.iter().find(|(cached, _)| cached == query) {
            return Ok(ids.clone());
        }
        let ids = ordered_ids(&self.connection, query, &search)?;
        self.lists.push_front((query.clone(), ids.clone()));
        self.lists.truncate(CACHED_LISTS);
        Ok(ids)
    }
}

/// Every container with an active file behind a track, with the number of such tracks, most
/// common first and then by name.
pub fn formats(connection: &Connection) -> Result<Vec<(String, i64)>> {
    let mut statement = connection.prepare(
        "SELECT f.container, COUNT(DISTINCT f.track_id) AS tracks FROM file f \
         WHERE f.lifecycle = 'active' AND f.track_id IS NOT NULL \
         GROUP BY f.container ORDER BY tracks DESC, f.container",
    )?;
    let rows = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Every label tracks with a present file carry, as spelled, with how many tracks, most first.
pub fn labels(connection: &Connection) -> Result<Vec<(String, i64)>> {
    let mut statement = connection.prepare(
        "SELECT t.label, COUNT(DISTINCT t.id) FROM track t \
         WHERE t.label <> '' AND EXISTS (SELECT 1 FROM file f WHERE f.track_id = t.id \
         AND f.presence = 'present' AND f.lifecycle = 'active') \
         GROUP BY t.label ORDER BY COUNT(DISTINCT t.id) DESC, t.label",
    )?;
    let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// A present file that is the same size as one about to be imported and within a second of
/// its length: the same recording already in the library, most likely.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LikeFile {
    pub title: String,
    pub artist: String,
    /// The music folder's label.
    pub folder: String,
    /// Inside the music folder, with "/" between the parts (lossy where not text).
    pub path: String,
}

pub fn files_like(connection: &Connection, size: i64, duration_ms: i64) -> Result<Vec<LikeFile>> {
    let mut statement = connection.prepare_cached(
        "SELECT t.title, a.name, r.label, f.path_raw FROM file f \
         JOIN track t ON t.id = f.track_id JOIN artist a ON a.id = t.artist_id \
         JOIN library_root r ON r.id = f.root_id \
         WHERE f.size = ?1 AND f.presence = 'present' AND f.lifecycle = 'active' \
         AND f.duration_ms IS NOT NULL AND ABS(f.duration_ms - ?2) <= 1000 \
         ORDER BY t.id LIMIT 5",
    )?;
    let rows = statement.query_map(rusqlite::params![size, duration_ms], |row| {
        let path: Vec<u8> = row.get(3)?;
        Ok(LikeFile {
            title: row.get(0)?,
            artist: row.get(1)?,
            folder: row.get(2)?,
            path: String::from_utf8_lossy(&path).replace('\\', "/"),
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
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
