use std::io::{self, BufReader, Cursor};

use super::{read, read_with_limits, Error, Limits, PlaylistNode};

const FIXTURE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<DJ_PLAYLISTS Version="1.0.0">
 <PRODUCT Name="Synthetic" Version="1" Company="Fixture"/>
 <COLLECTION Entries="2">
  <TRACK TrackID="7" Name="Test &amp; Signal" Artist="Synthetic Artist"
         Album="Example" AverageBpm="127.500" Tonality="Am" Rating="255"
         Location="file://localhost/G:/Synthetic/A%20%26%20B%5b1%5d%23+%25.aif"
         Extra="preserved">
   <TEMPO Inizio="0.0255" Bpm="127.500" Metro="4/4" Battito="1"/>
   <TEMPO Inizio="25.750" Bpm="128.0" Metro="4/4" Battito="3"/>
   <POSITION_MARK Name="Memory &quot;cue&quot;" Type="0" Start="0.0255" Num="-1"/>
   <POSITION_MARK Name="Loop" Type="4" Start="12.25" End="16.5" Num="0"
                  Red="255" Green="64" Blue="0"/>
  </TRACK>
  <TRACK TrackID="42" Name="Caf&#233;" Location="file://localhost/Volumes/Test/%ff.aif"/>
 </COLLECTION>
 <PLAYLISTS>
  <NODE Type="0" Name="ROOT" Count="2">
   <NODE Type="0" Name="Nested" Count="1">
    <NODE Type="1" Name="Ordered" KeyType="0" Entries="4">
     <TRACK Key="42"/><TRACK Key="7"/><TRACK Key="42"/><TRACK Key="999"/>
    </NODE>
   </NODE>
   <NODE Type="1" Name="Empty" KeyType="0" Entries="0"/>
  </NODE>
 </PLAYLISTS>
</DJ_PLAYLISTS>"#;

fn parse(source: &str) -> Result<super::Collection, Error> {
    read(Cursor::new(source.as_bytes()))
}

fn collection(body: &str, count: usize) -> String {
    format!("<DJ_PLAYLISTS><COLLECTION Entries=\"{count}\">{body}</COLLECTION></DJ_PLAYLISTS>")
}

fn track(extra: &str) -> String {
    format!("<TRACK TrackID=\"1\" Location=\"file://localhost/G:/Synthetic.aif\">{extra}</TRACK>")
}

#[test]
fn reads_tracks_metadata_timing_and_byte_preserving_locations() {
    let document = parse(FIXTURE).unwrap();
    assert_eq!(document.tracks.len(), 2);
    assert_eq!(document.attributes["Version"], "1.0.0");
    assert_eq!(document.collection_attributes["Entries"], "2");
    assert_eq!(document.product.unwrap()["Name"], "Synthetic");
    let first = &document.tracks[0];
    assert_eq!(first.id, 7);
    assert_eq!(first.attributes["Name"], "Test & Signal");
    assert_eq!(first.attributes["Extra"], "preserved");
    assert_eq!(first.location_bytes, b"/G:/Synthetic/A & B[1]#+%.aif");
    assert_eq!(first.cues.len(), 2);
    assert_eq!(first.cues[0].number, Some(-1));
    assert!((first.cues[0].start_seconds - 0.0255).abs() < f64::EPSILON);
    assert_eq!(first.cues[0].attributes["Name"], "Memory \"cue\"");
    assert_eq!(first.cues[1].end_seconds, Some(16.5));
    assert_eq!(first.cues[1].attributes["Red"], "255");
    assert_eq!(first.tempos.len(), 2);
    assert_eq!(first.tempos[1].bpm, 128.0);
    assert_eq!(first.tempos[0].meter.as_deref(), Some("4/4"));
    assert_eq!(first.tempos[1].beat, Some(3));
    assert_eq!(document.tracks[1].attributes["Name"], "Café");
    assert_eq!(document.tracks[1].location_bytes, b"/Volumes/Test/\xff.aif");
    assert_eq!(document.tracks[1].cues, []);
}

#[test]
fn preserves_playlist_nesting_order_duplicates_and_unresolved_references() {
    let document = parse(FIXTURE).unwrap();
    assert_eq!(document.unresolved_track_ids, [999]);
    let PlaylistNode::Folder { children, .. } = &document.playlists[0] else {
        panic!("expected root folder")
    };
    assert_eq!(children.len(), 2);
    let PlaylistNode::Folder { children, .. } = &children[0] else {
        panic!("expected nested folder")
    };
    let PlaylistNode::Playlist {
        track_ids,
        attributes,
    } = &children[0]
    else {
        panic!("expected playlist")
    };
    assert_eq!(attributes["Name"], "Ordered");
    assert_eq!(track_ids, &[42, 7, 42, 999]);
}

#[test]
fn accepts_bom_empty_collection_and_optional_sections() {
    let xml = collection("", 0);
    assert_eq!(parse(&xml).unwrap().playlists, []);
    let mut bom = vec![0xef, 0xbb, 0xbf];
    bom.extend_from_slice(xml.as_bytes());
    assert_eq!(read(Cursor::new(bom)).unwrap().tracks, []);
    assert_eq!(
        parse("<DJ_PLAYLISTS><COLLECTION Entries=\"0\"/></DJ_PLAYLISTS>")
            .unwrap()
            .tracks,
        []
    );
}

#[test]
fn tiny_input_buffers_do_not_change_results() {
    let reference = parse(FIXTURE).unwrap();
    for size in [1, 2, 7, 31] {
        let reader = BufReader::with_capacity(size, Cursor::new(FIXTURE.as_bytes()));
        assert_eq!(read(reader).unwrap(), reference);
    }
}

#[test]
fn decodes_xml_entities_once_then_percent_escapes_once() {
    let xml = collection(
        "<TRACK TrackID=\"1\" Name=\"&amp;lt;\" Location=\"file://localhost//server/share/A&amp;B#C+%2520.aif\"/>",
        1,
    );
    let document = parse(&xml).unwrap();
    assert_eq!(document.tracks[0].attributes["Name"], "&lt;");
    assert_eq!(
        document.tracks[0].location_bytes,
        b"//server/share/A&B#C+%20.aif"
    );
}

#[test]
fn unknown_extensions_are_skipped_without_mistaking_their_tracks_for_collection() {
    let xml = "<DJ_PLAYLISTS><EXTENSION><TRACK arbitrary=\"yes\"/></EXTENSION><COLLECTION Entries=\"0\"/></DJ_PLAYLISTS>";
    let parsed = parse(xml).unwrap();
    assert_eq!(parsed.tracks, []);
    assert_eq!(parsed.ignored_elements, ["EXTENSION"]);
    assert!(parse(&collection(&track("<EXTRA><VALUE>data</VALUE></EXTRA>"), 1)).is_ok());
}

#[test]
fn malformed_truncated_and_misplaced_known_elements_are_refused() {
    for xml in [
        "",
        "<DJ_PLAYLISTS>",
        "<DJ_PLAYLISTS><COLLECTION Entries=\"0\"/></different>",
        "<DJ_PLAYLISTS><COLLECTION Entries=\"0\"/></DJ_PLAYLISTS><DJ_PLAYLISTS/>",
        "<DJ_PLAYLISTS><COLLECTION Entries=\"0\"/><COLLECTION Entries=\"0\"/></DJ_PLAYLISTS>",
        "<DJ_PLAYLISTS><TRACK TrackID=\"1\"/></DJ_PLAYLISTS>",
        "<DJ_PLAYLISTS><COLLECTION Entries=\"0\"/><NODE Type=\"0\" Name=\"Wrong\"/></DJ_PLAYLISTS>",
        "<DJ_PLAYLISTS>unexpected<COLLECTION Entries=\"0\"/></DJ_PLAYLISTS>",
    ] {
        assert!(parse(xml).is_err(), "{xml}");
    }
    assert!(parse(&collection(&track(&track("")), 1)).is_err());
}

#[test]
fn declared_counts_and_duplicate_ids_are_checked() {
    assert!(matches!(
        parse(&collection(&track(""), 2)),
        Err(Error::Count(_))
    ));
    assert!(matches!(
        parse(&collection(&format!("{}{}", track(""), track("")), 2)),
        Err(Error::DuplicateTrack)
    ));
    assert!(matches!(
        parse(&FIXTURE.replace("Count=\"2\"", "Count=\"3\"")),
        Err(Error::Count("folder Count"))
    ));
    assert!(matches!(
        parse(&FIXTURE.replace("Entries=\"4\"", "Entries=\"3\"")),
        Err(Error::Count("playlist Entries"))
    ));
}

#[test]
fn non_id_playlist_keys_and_missing_required_attributes_are_refused() {
    assert!(matches!(
        parse(&FIXTURE.replace("KeyType=\"0\"", "KeyType=\"1\"")),
        Err(Error::KeyType)
    ));
    for xml in [
        collection("<TRACK Location=\"file://localhost/x\"/>", 1),
        collection("<TRACK TrackID=\"1\"/>", 1),
        collection(
            "<TRACK TrackID=\"not-an-id\" Location=\"file://localhost/x\"/>",
            1,
        ),
        collection(
            "<TRACK TrackID=\"18446744073709551616\" Location=\"file://localhost/x\"/>",
            1,
        ),
    ] {
        assert!(parse(&xml).is_err());
    }
}

#[test]
fn invalid_times_bpm_and_reversed_loops_are_refused() {
    for value in ["NaN", "inf", "-1", "no-number"] {
        assert!(parse(&collection(
            &track(&format!("<POSITION_MARK Type=\"0\" Start=\"{value}\"/>")),
            1
        ))
        .is_err());
        assert!(parse(&collection(
            &track(&format!("<TEMPO Inizio=\"0\" Bpm=\"{value}\"/>")),
            1
        ))
        .is_err());
    }
    assert!(parse(&collection(&track("<TEMPO Inizio=\"0\" Bpm=\"0\"/>"), 1)).is_err());
    assert!(parse(&collection(
        &track("<POSITION_MARK Type=\"4\" Start=\"2\" End=\"1\"/>"),
        1
    ))
    .is_err());
    let document = parse(&collection(
        &track("<POSITION_MARK Type=\"0\" Start=\"0\"/>"),
        1,
    ))
    .unwrap();
    assert_eq!(document.tracks[0].cues[0].number, None);
}

#[test]
fn invalid_location_entities_encoding_and_attributes_are_refused() {
    for xml in [
        collection(
            "<TRACK TrackID=\"1\" Location=\"file://localhost/%gg\"/>",
            1,
        ),
        collection(
            "<TRACK TrackID=\"1\" Location=\"file://localhost/%00\"/>",
            1,
        ),
        collection(
            "<TRACK TrackID=\"1\" Location=\"https://example.invalid\"/>",
            1,
        ),
        collection(
            "<TRACK TrackID=\"1\" Location=\"file://localhost/x\" Name=\"&unknown;\"/>",
            1,
        ),
        collection(
            "<TRACK TrackID=\"1\" TrackID=\"2\" Location=\"file://localhost/x\"/>",
            1,
        ),
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-16\"?>{}",
            collection("", 0)
        ),
    ] {
        assert!(parse(&xml).is_err());
    }
    let xml = b"<DJ_PLAYLISTS><COLLECTION Entries=\"1\"><TRACK TrackID=\"1\" Location=\"file://localhost/x\" Name=\"\xff\"/></COLLECTION></DJ_PLAYLISTS>";
    assert!(matches!(read(Cursor::new(xml)), Err(Error::Encoding)));
}

#[test]
fn doctype_and_resource_exhaustion_are_refused() {
    let xml = format!(
        "<!DOCTYPE DJ_PLAYLISTS [<!ENTITY x \"expanded\">]>{}",
        collection("", 0)
    );
    assert!(matches!(parse(&xml), Err(Error::Doctype)));
    let xml = collection(&track(""), 1);
    let limits = Limits {
        input_bytes: u64::try_from(xml.len() - 1).unwrap(),
        ..Limits::default()
    };
    assert!(read_with_limits(Cursor::new(&xml), limits).is_err());
    let limits = Limits {
        input_bytes: u64::try_from(xml.len()).unwrap(),
        ..Limits::default()
    };
    assert!(read_with_limits(Cursor::new(&xml), limits).is_ok());
    assert!(matches!(
        read_with_limits(
            Cursor::new(&xml),
            Limits {
                tracks: 0,
                ..Limits::default()
            }
        ),
        Err(Error::Limit("collection tracks"))
    ));
    assert!(matches!(
        read_with_limits(
            Cursor::new(FIXTURE),
            Limits {
                playlist_references: 3,
                ..Limits::default()
            }
        ),
        Err(Error::Limit("playlist references"))
    ));
    let xml = "<DJ_PLAYLISTS><UNKNOWN><UNKNOWN><UNKNOWN/></UNKNOWN></UNKNOWN><COLLECTION Entries=\"0\"/></DJ_PLAYLISTS>";
    assert!(matches!(
        read_with_limits(
            Cursor::new(xml),
            Limits {
                depth: 3,
                ..Limits::default()
            }
        ),
        Err(Error::Limit("element depth"))
    ));
}

struct FailingReader;
impl io::Read for FailingReader {
    fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "synthetic read error",
        ))
    }
}

#[test]
fn io_errors_do_not_return_a_partial_document() {
    assert!(matches!(
        read(BufReader::new(FailingReader)),
        Err(Error::Syntax(_))
    ));
}

#[test]
fn skipped_extensions_still_require_valid_xml_text_and_entities() {
    for text in [
        "&unknown;",
        "&#0;",
        "\u{1}",
        "<![CDATA[\u{1}]]>",
        "]]>",
        "&unterminated",
    ] {
        let xml = format!(
            "<DJ_PLAYLISTS><EXTENSION>{text}</EXTENSION><COLLECTION Entries=\"0\"/></DJ_PLAYLISTS>"
        );
        assert!(parse(&xml).is_err(), "{text:?}");
    }
    let xml = "<DJ_PLAYLISTS><EXTENSION>A &amp; B <![CDATA[<literal>]]></EXTENSION><COLLECTION Entries=\"0\"/></DJ_PLAYLISTS>";
    assert_eq!(parse(xml).unwrap().ignored_elements, ["EXTENSION"]);
    for value in ["&#0;", "\u{1}", "<unescaped"] {
        let xml = collection(
            &format!("<TRACK TrackID=\"1\" Name=\"{value}\" Location=\"file://localhost/x\"/>"),
            1,
        );
        assert!(parse(&xml).is_err());
    }
}

#[test]
fn declarations_and_comments_are_not_silently_accepted_when_invalid() {
    for prefix in [
        "<?xml version=\"1.0\"?><?xml version=\"1.0\"?>",
        "<?xml version=\"1.1\"?>",
        "<!-- embedded -- is invalid -->",
    ] {
        assert!(parse(&format!("{prefix}{}", collection("", 0))).is_err());
    }
    let limits = Limits {
        depth: 129,
        ..Limits::default()
    };
    assert!(matches!(
        read_with_limits(Cursor::new(collection("", 0)), limits),
        Err(Error::Limit("configured maximum"))
    ));
}

#[test]
fn parses_large_synthetic_collection_with_exact_order_and_count() {
    use std::fmt::Write;
    let mut xml = String::from("<DJ_PLAYLISTS><COLLECTION Entries=\"10000\">");
    for id in 0..10_000 {
        write!(
            xml,
            "<TRACK TrackID=\"{id}\" Location=\"file://localhost/G:/Synthetic/{id}.aif\"/>"
        )
        .unwrap();
    }
    xml.push_str("</COLLECTION></DJ_PLAYLISTS>");
    let parsed = parse(&xml).unwrap();
    assert_eq!(parsed.tracks.len(), 10_000);
    assert_eq!(parsed.tracks[0].id, 0);
    assert_eq!(parsed.tracks[9_999].id, 9_999);
}
