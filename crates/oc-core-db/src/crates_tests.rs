use tempfile::tempdir;

use crate::library::{LibraryQuery, Sort, View};
use crate::service::LibraryService;
use crate::Error;

fn library(tracks: u32) -> (tempfile::TempDir, LibraryService, Vec<i64>) {
    let data = tempdir().unwrap();
    let service = LibraryService::open(data.path(), 2).unwrap();
    service.seed_demo(tracks).unwrap();
    let page = service
        .read_page(&LibraryQuery::new(View::Tracks), 0, 200)
        .unwrap();
    let ids = page.items.iter().map(|item| item.id).collect();
    (data, service, ids)
}

fn in_crate(service: &LibraryService, crate_id: i64) -> Vec<i64> {
    let mut query = LibraryQuery::new(View::Tracks);
    query.crate_id = Some(crate_id);
    query.sort = Sort::Crate;
    service
        .read_page(&query, 0, 200)
        .unwrap()
        .items
        .iter()
        .map(|item| item.id)
        .collect()
}

#[test]
fn a_crate_holds_tracks_in_the_order_they_were_added_without_duplicates() {
    let (_data, service, ids) = library(10);
    let id = service.create_crate("  Warm-up  ").unwrap();
    assert_eq!(
        service.crates().unwrap()[0].name,
        "Warm-up",
        "names are trimmed"
    );
    let picked = vec![ids[3], ids[1], ids[7]];
    assert_eq!(service.add_to_crate(id, picked.clone()).unwrap(), 3);
    assert_eq!(
        service
            .add_to_crate(id, vec![ids[1], 999_999, ids[2]])
            .unwrap(),
        1,
        "a duplicate and an unknown id are skipped"
    );
    assert_eq!(in_crate(&service, id), [ids[3], ids[1], ids[7], ids[2]]);
    assert_eq!(service.crates().unwrap()[0].tracks, 4);
}

#[test]
fn dragging_reorders_and_the_library_query_follows_the_crate_order() {
    let (_data, service, ids) = library(10);
    let id = service.create_crate("Peak").unwrap();
    service.add_to_crate(id, ids[..5].to_vec()).unwrap();
    let order = service
        .move_in_crate(id, vec![ids[4]], Some(ids[0]))
        .unwrap();
    assert_eq!(order, [ids[4], ids[0], ids[1], ids[2], ids[3]]);
    assert_eq!(in_crate(&service, id), order);
    service
        .move_in_crate(id, vec![ids[4], ids[1]], None)
        .unwrap();
    assert_eq!(
        in_crate(&service, id),
        [ids[0], ids[2], ids[3], ids[4], ids[1]]
    );
    // Other sorts still work inside a crate; only the members are shown.
    let mut by_title = LibraryQuery::new(View::Tracks);
    by_title.crate_id = Some(id);
    by_title.sort = Sort::Title;
    assert_eq!(service.read_page(&by_title, 0, 50).unwrap().total, 5);
}

#[test]
fn removing_closes_the_gap_and_deleting_a_crate_keeps_the_tracks() {
    let (_data, service, ids) = library(6);
    let id = service.create_crate("Closers").unwrap();
    service.add_to_crate(id, ids[..4].to_vec()).unwrap();
    assert_eq!(service.remove_from_crate(id, vec![ids[1], 777]).unwrap(), 1);
    assert_eq!(in_crate(&service, id), [ids[0], ids[2], ids[3]]);
    service
        .move_in_crate(id, vec![ids[3]], Some(ids[2]))
        .unwrap();
    assert_eq!(in_crate(&service, id), [ids[0], ids[3], ids[2]]);
    service.delete_crate(id).unwrap();
    assert_eq!(service.crates().unwrap().len(), 0);
    assert_eq!(
        service
            .read_page(&LibraryQuery::new(View::Tracks), 0, 50)
            .unwrap()
            .total,
        6
    );
}

#[test]
fn bad_names_and_unknown_crates_are_refused() {
    let (_data, service, ids) = library(2);
    for bad in ["", "   ", &"x".repeat(101), "tab\there"] {
        assert!(
            matches!(service.create_crate(bad), Err(Error::InvalidCrateName)),
            "{bad:?}"
        );
    }
    let id = service.create_crate("A").unwrap();
    service.rename_crate(id, "B").unwrap();
    assert_eq!(service.crates().unwrap()[0].name, "B");
    assert!(matches!(
        service.rename_crate(id, ""),
        Err(Error::InvalidCrateName)
    ));
    for result in [
        service.rename_crate(999, "x").err(),
        service.delete_crate(999).err(),
        service.add_to_crate(999, ids.clone()).err(),
        service.remove_from_crate(999, ids.clone()).err(),
        service.move_in_crate(999, ids.clone(), None).err(),
    ] {
        assert!(matches!(result, Some(Error::UnknownCrate(999))));
    }
    let second = service.create_crate("C").unwrap();
    let names: Vec<_> = service
        .crates()
        .unwrap()
        .into_iter()
        .map(|c| (c.id, c.name))
        .collect();
    assert_eq!(
        names,
        [(id, "B".to_owned()), (second, "C".to_owned())],
        "creation order"
    );
}

fn ordered(service: &LibraryService, sort: Sort) -> Vec<i64> {
    let mut query = LibraryQuery::new(View::Tracks);
    query.sort = sort;
    service
        .read_page(&query, 0, 200)
        .unwrap()
        .items
        .iter()
        .map(|item| item.id)
        .collect()
}

#[test]
fn the_first_drag_keeps_everything_else_where_the_current_sort_had_it() {
    let (_data, service, _) = library(8);
    let by_title = ordered(&service, Sort::Title);
    // Drag the last title to the top, starting from title order.
    service
        .move_in_library(vec![by_title[7]], Some(by_title[0]), Sort::Title)
        .unwrap();
    let mut expected = vec![by_title[7]];
    expected.extend(&by_title[..7]);
    assert_eq!(ordered(&service, Sort::Custom), expected);
    // Later moves build on the arrangement, and other sorts are unaffected.
    service
        .move_in_library(vec![expected[0]], None, Sort::Custom)
        .unwrap();
    assert_eq!(ordered(&service, Sort::Custom), by_title);
    assert_eq!(ordered(&service, Sort::Title), by_title);
    assert!(matches!(
        service.move_in_library(vec![1], None, Sort::Playlist),
        Err(Error::InvalidArrangeSort)
    ));
}

#[test]
fn arranging_a_large_library_is_quick_enough_for_a_drag() {
    let (_data, service, _) = library(20_000);
    let recent = ordered(&service, Sort::Recent);
    let start = std::time::Instant::now();
    service
        .move_in_library(vec![recent[150]], Some(recent[0]), Sort::Recent)
        .unwrap();
    let first = start.elapsed();
    let start = std::time::Instant::now();
    service
        .move_in_library(vec![recent[3]], Some(recent[0]), Sort::Custom)
        .unwrap();
    let second = start.elapsed();
    eprintln!("arranging 20,000 tracks: first {first:?}, then {second:?}");
    assert_eq!(
        ordered(&service, Sort::Custom)[..3],
        [recent[150], recent[3], recent[0]]
    );
}

#[test]
fn merging_appends_the_other_crates_tracks_once_and_can_remove_them() {
    let (_data, service, ids) = library(10);
    let a = service.create_crate("A").unwrap();
    let b = service.create_crate("B").unwrap();
    service
        .add_to_crate(a, vec![ids[0], ids[1], ids[2]])
        .unwrap();
    service
        .add_to_crate(b, vec![ids[2], ids[5], ids[4]])
        .unwrap();
    let added = service.merge_crates(a, vec![b], true).unwrap();
    assert_eq!(added, 2, "the track both held is not added again");
    assert_eq!(
        in_crate(&service, a),
        [ids[0], ids[1], ids[2], ids[5], ids[4]]
    );
    assert_eq!(
        service.crates().unwrap().len(),
        1,
        "the merged crate is removed"
    );
    assert!(
        service.merge_crates(a, vec![a], false).is_err(),
        "a crate is not merged into itself"
    );
}

#[test]
fn a_crate_can_be_sorted_in_place_by_any_library_order() {
    let (_data, service, ids) = library(12);
    let a = service.create_crate("A").unwrap();
    service
        .add_to_crate(a, vec![ids[9], ids[2], ids[7], ids[4]])
        .unwrap();
    for sort in [Sort::Title, Sort::Artist, Sort::Bpm, Sort::Recent] {
        service.sort_crate(a, sort).unwrap();
        let mut query = LibraryQuery::new(View::Tracks);
        query.crate_id = Some(a);
        query.sort = sort;
        let expected: Vec<i64> = service
            .read_page(&query, 0, 200)
            .unwrap()
            .items
            .iter()
            .map(|item| item.id)
            .collect();
        assert_eq!(in_crate(&service, a), expected, "{sort:?}");
    }
    assert!(
        service.sort_crate(a, Sort::Crate).is_err(),
        "a crate's own order is not a sort"
    );
}

#[test]
fn comparing_two_crates_says_what_they_share_and_what_only_one_holds() {
    let (_data, service, ids) = library(10);
    let a = service.create_crate("A").unwrap();
    let b = service.create_crate("B").unwrap();
    service
        .add_to_crate(a, vec![ids[0], ids[1], ids[2]])
        .unwrap();
    service
        .add_to_crate(b, vec![ids[2], ids[1], ids[5]])
        .unwrap();
    let found = service.compare_crates(a, b).unwrap();
    assert_eq!(found.both, [ids[1], ids[2]], "in A's order");
    assert_eq!(found.only_first, [ids[0]]);
    assert_eq!(found.only_second, [ids[5]]);
}

#[test]
fn custom_fields_are_kept_per_track_in_the_library_and_an_empty_value_removes_one() {
    let (_data, service, ids) = library(6);
    assert_eq!(
        service.custom_fields(&[ids[0], ids[1]]).unwrap(),
        Vec::<(i64, String, String)>::new()
    );
    service
        .set_custom_field(&[ids[0], ids[1]], " Energy ", " 7 ")
        .unwrap();
    service.set_custom_field(&[ids[0]], "Mood", "late").unwrap();
    let mut found = service.custom_fields(&[ids[0], ids[1]]).unwrap();
    found.sort();
    let mut expected = vec![
        (ids[0], "Energy".to_owned(), "7".to_owned()),
        (ids[0], "Mood".to_owned(), "late".to_owned()),
        (ids[1], "Energy".to_owned(), "7".to_owned()),
    ];
    expected.sort();
    assert_eq!(found, expected);
    service
        .set_custom_field(&[ids[0], ids[1]], "Energy", "  ")
        .unwrap();
    assert_eq!(
        service.custom_fields(&[ids[0], ids[1]]).unwrap(),
        [(ids[0], "Mood".to_owned(), "late".to_owned())]
    );
    assert!(
        service.set_custom_field(&[ids[0]], " ", "x").is_err(),
        "a field needs a name"
    );
    assert_eq!(service.custom_field_names().unwrap(), ["Mood"]);
}
