use std::collections::{BTreeSet, HashSet};
use std::io::{BufRead, BufReader};
use std::str::FromStr;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use super::{Attributes, Collection, Cue, Error, Limits, PlaylistNode, Tempo, Track};
use crate::location::decode_location;

enum Frame {
    Root,
    Product,
    Collection,
    Track(Track),
    Cue,
    Tempo,
    Playlists,
    Folder {
        attributes: Attributes,
        children: Vec<PlaylistNode>,
        expected: Option<usize>,
    },
    Playlist {
        attributes: Attributes,
        track_ids: Vec<u64>,
        expected: Option<usize>,
    },
    Reference,
    Unknown,
}

struct Parser {
    result: Collection,
    stack: Vec<Frame>,
    ids: HashSet<u64>,
    references: usize,
    collection_entries: Option<usize>,
    seen_collection: bool,
    seen_playlists: bool,
    finished: bool,
    ignored_elements: BTreeSet<String>,
    limits: Limits,
}

pub(super) fn read(reader: impl BufRead, limits: Limits) -> Result<Collection, Error> {
    let defaults = Limits::default();
    if limits.input_bytes > defaults.input_bytes
        || limits.depth > defaults.depth
        || limits.tracks > defaults.tracks
        || limits.playlist_references > defaults.playlist_references
    {
        return Err(Error::Limit("configured maximum"));
    }
    let input = reader.take(limits.input_bytes.saturating_add(1));
    let mut reader = Reader::from_reader(BufReader::new(input));
    reader.config_mut().check_end_names = true;
    reader.config_mut().check_comments = true;
    let mut buffer = Vec::new();
    let mut parser = Parser::new(limits);
    let mut declared = false;
    loop {
        let event = reader.read_event_into(&mut buffer);
        if reader.buffer_position() > limits.input_bytes {
            return Err(Error::Limit("input bytes"));
        }
        // quick-xml decodes names and text itself, so bytes that are not UTF-8 surface here as
        // its encoding error rather than as a syntax fault.
        let event = event.map_err(|error| match error {
            quick_xml::Error::Encoding(_) => Error::Encoding,
            other => Error::Syntax(other),
        })?;
        match event {
            Event::Start(element) => parser.start(&element)?,
            Event::Empty(element) => {
                parser.start(&element)?;
                parser.end()?;
            }
            Event::End(_) => parser.end()?,
            Event::DocType(_) => return Err(Error::Doctype),
            Event::Decl(declaration) => {
                if declared || parser.finished || !parser.stack.is_empty() {
                    return Err(Error::Structure("XML declaration inside document"));
                }
                declared = true;
                if declaration.version().map_err(|_| Error::Encoding)? != "1.0" {
                    return Err(Error::Encoding);
                }
                if let Some(encoding) = declaration.encoding() {
                    let encoding = encoding.map_err(|_| Error::Encoding)?;
                    if !encoding.eq_ignore_ascii_case("utf-8")
                        && !encoding.eq_ignore_ascii_case("us-ascii")
                    {
                        return Err(Error::Encoding);
                    }
                }
            }
            Event::Text(text) => {
                let text_value = xml_text(text.as_ref())?;
                if text_value.contains("]]>") {
                    return Err(Error::Structure("CDATA terminator in text"));
                }
                quick_xml::escape::unescape(text_value).map_err(|_| Error::Attributes)?;
                if !matches!(parser.stack.last(), Some(Frame::Unknown))
                    && !text.as_ref().chars().all(|c| c.is_ascii_whitespace())
                {
                    return Err(Error::Structure("text outside metadata attributes"));
                }
            }
            Event::CData(text) => {
                xml_text(text.as_ref())?;
                if !matches!(parser.stack.last(), Some(Frame::Unknown)) {
                    return Err(Error::Structure("text outside metadata attributes"));
                }
            }
            Event::GeneralRef(reference) => {
                let name = xml_text(reference.as_ref())?;
                let escaped = format!("&{name};");
                let value = quick_xml::escape::unescape(&escaped).map_err(|_| Error::Attributes)?;
                xml_text(&value)?;
                if !matches!(parser.stack.last(), Some(Frame::Unknown)) {
                    return Err(Error::Structure("text outside metadata attributes"));
                }
            }
            Event::Eof => return parser.finish(),
            Event::Comment(_) | Event::PI(_) => {}
        }
        buffer.clear();
    }
}

impl Parser {
    fn new(limits: Limits) -> Self {
        Self {
            result: Collection {
                attributes: Attributes::new(),
                product: None,
                collection_attributes: Attributes::new(),
                tracks: Vec::new(),
                playlists: Vec::new(),
                unresolved_track_ids: Vec::new(),
                ignored_elements: Vec::new(),
            },
            stack: Vec::new(),
            ids: HashSet::new(),
            references: 0,
            collection_entries: None,
            seen_collection: false,
            seen_playlists: false,
            finished: false,
            ignored_elements: BTreeSet::new(),
            limits,
        }
    }

    fn start(&mut self, element: &BytesStart<'_>) -> Result<(), Error> {
        if self.stack.len() >= self.limits.depth {
            return Err(Error::Limit("element depth"));
        }
        let name = element.name();
        let name = name.as_ref();
        let attributes = attributes(element)?;
        let frame = match self.stack.last() {
            None => {
                if self.finished || name != "DJ_PLAYLISTS" {
                    return Err(Error::Structure("expected one DJ_PLAYLISTS root"));
                }
                self.result.attributes = attributes;
                Frame::Root
            }
            Some(Frame::Unknown) => Frame::Unknown,
            Some(Frame::Root) => self.root_child(name, attributes)?,
            Some(Frame::Collection) if name == "TRACK" => {
                if self.ids.len() >= self.limits.tracks {
                    return Err(Error::Limit("collection tracks"));
                }
                let id = number(&attributes, "TrackID")?;
                if !self.ids.insert(id) {
                    return Err(Error::DuplicateTrack);
                }
                let location = required(&attributes, "Location")?.to_owned();
                let location_bytes = decode_location(&location)?;
                Frame::Track(Track {
                    id,
                    location,
                    location_bytes,
                    attributes,
                    cues: Vec::new(),
                    tempos: Vec::new(),
                })
            }
            Some(Frame::Track(_)) if name == "POSITION_MARK" => {
                let cue = cue(attributes)?;
                if let Some(Frame::Track(track)) = self.stack.last_mut() {
                    track.cues.push(cue);
                }
                Frame::Cue
            }
            Some(Frame::Track(_)) if name == "TEMPO" => {
                let tempo = tempo(attributes)?;
                if let Some(Frame::Track(track)) = self.stack.last_mut() {
                    track.tempos.push(tempo);
                }
                Frame::Tempo
            }
            Some(Frame::Playlists | Frame::Folder { .. }) if name == "NODE" => node(attributes)?,
            Some(Frame::Playlist { .. }) if name == "TRACK" => {
                if self.references >= self.limits.playlist_references {
                    return Err(Error::Limit("playlist references"));
                }
                let id = number(&attributes, "Key")?;
                self.references += 1;
                if let Some(Frame::Playlist { track_ids, .. }) = self.stack.last_mut() {
                    track_ids.push(id);
                }
                Frame::Reference
            }
            _ => unknown(name)?,
        };
        if matches!(frame, Frame::Unknown) && !matches!(self.stack.last(), Some(Frame::Unknown)) {
            self.ignored_elements.insert(xml_text(name)?.to_owned());
        }
        self.stack.push(frame);
        Ok(())
    }

    fn root_child(&mut self, name: &str, attributes: Attributes) -> Result<Frame, Error> {
        match name {
            "PRODUCT" => {
                if self.result.product.is_some() {
                    return Err(Error::Structure("duplicate PRODUCT"));
                }
                self.result.product = Some(attributes);
                Ok(Frame::Product)
            }
            "COLLECTION" => {
                if self.seen_collection {
                    return Err(Error::Structure("duplicate COLLECTION"));
                }
                self.seen_collection = true;
                self.collection_entries = Some(number(&attributes, "Entries")?);
                self.result.collection_attributes = attributes;
                Ok(Frame::Collection)
            }
            "PLAYLISTS" => {
                if self.seen_playlists {
                    return Err(Error::Structure("duplicate PLAYLISTS"));
                }
                self.seen_playlists = true;
                Ok(Frame::Playlists)
            }
            _ => unknown(name),
        }
    }

    fn end(&mut self) -> Result<(), Error> {
        let frame = self
            .stack
            .pop()
            .ok_or(Error::Structure("unexpected closing element"))?;
        match frame {
            Frame::Root => self.finished = true,
            Frame::Track(track) => self.result.tracks.push(track),
            Frame::Folder {
                attributes,
                children,
                expected,
            } => {
                check_count(expected, children.len(), "folder Count")?;
                self.append_node(PlaylistNode::Folder {
                    attributes,
                    children,
                })?;
            }
            Frame::Playlist {
                attributes,
                track_ids,
                expected,
            } => {
                check_count(expected, track_ids.len(), "playlist Entries")?;
                self.append_node(PlaylistNode::Playlist {
                    attributes,
                    track_ids,
                })?;
            }
            _ => {}
        }
        Ok(())
    }

    fn append_node(&mut self, node: PlaylistNode) -> Result<(), Error> {
        match self.stack.last_mut() {
            Some(Frame::Folder { children, .. }) => children.push(node),
            Some(Frame::Playlists) => self.result.playlists.push(node),
            _ => return Err(Error::Structure("NODE outside PLAYLISTS or folder")),
        }
        Ok(())
    }

    fn finish(mut self) -> Result<Collection, Error> {
        if !self.finished || !self.stack.is_empty() || !self.seen_collection {
            return Err(Error::Structure("incomplete document"));
        }
        check_count(
            self.collection_entries,
            self.result.tracks.len(),
            "COLLECTION Entries",
        )?;
        let mut unresolved = BTreeSet::new();
        for node in &self.result.playlists {
            unresolved_ids(node, &self.ids, &mut unresolved);
        }
        self.result.unresolved_track_ids = unresolved.into_iter().collect();
        self.result.ignored_elements = self.ignored_elements.into_iter().collect();
        Ok(self.result)
    }
}

fn attributes(element: &BytesStart<'_>) -> Result<Attributes, Error> {
    let mut values = Attributes::new();
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| Error::Attributes)?;
        let name = xml_text(attribute.key.as_ref())?;
        let raw_value = xml_text(attribute.value.as_ref())?;
        if raw_value.contains('<') {
            return Err(Error::Attributes);
        }
        let value = quick_xml::escape::unescape(raw_value).map_err(|_| Error::Attributes)?;
        xml_text(&value)?;
        values.insert(name.to_owned(), value.into_owned());
    }
    Ok(values)
}

fn xml_text(value: &str) -> Result<&str, Error> {
    if value
        .chars()
        .any(|value| !matches!(value, '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}'))
    {
        return Err(Error::Encoding);
    }
    Ok(value)
}

fn required<'a>(attributes: &'a Attributes, field: &'static str) -> Result<&'a str, Error> {
    attributes
        .get(field)
        .map(String::as_str)
        .ok_or(Error::Missing(field))
}

fn number<T: FromStr>(attributes: &Attributes, field: &'static str) -> Result<T, Error> {
    required(attributes, field)?
        .parse()
        .map_err(|_| Error::Number(field))
}

fn optional_number<T: FromStr>(
    attributes: &Attributes,
    field: &'static str,
) -> Result<Option<T>, Error> {
    attributes
        .get(field)
        .map(|value| value.parse().map_err(|_| Error::Number(field)))
        .transpose()
}

fn seconds(attributes: &Attributes, field: &'static str) -> Result<f64, Error> {
    let value: f64 = number(attributes, field)?;
    if !value.is_finite() || value < 0.0 {
        return Err(Error::Number(field));
    }
    Ok(value)
}

fn cue(attributes: Attributes) -> Result<Cue, Error> {
    let start_seconds = seconds(&attributes, "Start")?;
    let end_seconds = attributes
        .contains_key("End")
        .then(|| seconds(&attributes, "End"))
        .transpose()?;
    if end_seconds.is_some_and(|end| end < start_seconds) {
        return Err(Error::Number("End"));
    }
    Ok(Cue {
        kind: number(&attributes, "Type")?,
        number: optional_number(&attributes, "Num")?,
        start_seconds,
        end_seconds,
        attributes,
    })
}

fn tempo(attributes: Attributes) -> Result<Tempo, Error> {
    let bpm = seconds(&attributes, "Bpm")?;
    if bpm == 0.0 {
        return Err(Error::Number("Bpm"));
    }
    Ok(Tempo {
        start_seconds: seconds(&attributes, "Inizio")?,
        bpm,
        meter: attributes.get("Metro").cloned(),
        beat: optional_number(&attributes, "Battito")?,
        attributes,
    })
}

fn node(attributes: Attributes) -> Result<Frame, Error> {
    required(&attributes, "Name")?;
    match required(&attributes, "Type")? {
        "0" => Ok(Frame::Folder {
            expected: optional_number(&attributes, "Count")?,
            attributes,
            children: Vec::new(),
        }),
        "1" => {
            if required(&attributes, "KeyType")? != "0" {
                return Err(Error::KeyType);
            }
            Ok(Frame::Playlist {
                expected: optional_number(&attributes, "Entries")?,
                attributes,
                track_ids: Vec::new(),
            })
        }
        _ => Err(Error::Number("Type")),
    }
}

fn unknown(name: &str) -> Result<Frame, Error> {
    if matches!(
        name,
        "DJ_PLAYLISTS"
            | "PRODUCT"
            | "COLLECTION"
            | "PLAYLISTS"
            | "TRACK"
            | "NODE"
            | "POSITION_MARK"
            | "TEMPO"
    ) {
        return Err(Error::Structure("known element in unexpected context"));
    }
    Ok(Frame::Unknown)
}

fn check_count(expected: Option<usize>, found: usize, name: &'static str) -> Result<(), Error> {
    if expected.is_some_and(|expected| expected != found) {
        return Err(Error::Count(name));
    }
    Ok(())
}

fn unresolved_ids(node: &PlaylistNode, ids: &HashSet<u64>, unresolved: &mut BTreeSet<u64>) {
    match node {
        PlaylistNode::Folder { children, .. } => {
            for node in children {
                unresolved_ids(node, ids, unresolved);
            }
        }
        PlaylistNode::Playlist { track_ids, .. } => {
            unresolved.extend(track_ids.iter().filter(|id| !ids.contains(id)));
        }
    }
}
