use rbl_db::fixture::{self, Shape};
use rbl_db::{Library, OpenMode};

#[test]
fn rbl_db_opens_a_fixture() {
    let temp = tempfile::tempdir().unwrap();
    let location = fixture::build(temp.path(), Shape::default()).unwrap();
    let library = Library::open(location, OpenMode::ReadOnly).unwrap();
    assert_eq!(library.live_track_count().unwrap(), 40);
}
