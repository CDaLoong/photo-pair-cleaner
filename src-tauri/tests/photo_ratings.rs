#[allow(dead_code)]
#[path = "../src/formats.rs"]
mod formats;
#[path = "../src/ratings.rs"]
mod ratings;

use std::fs;

#[test]
fn ratings_persist_per_root_and_zero_removes_the_entry() {
    let temp = tempfile::tempdir().expect("temp directory");
    let root = temp.path().join("photos");
    let day = root.join("day");
    let database = temp.path().join("app-data/photo-ratings.json");
    fs::create_dir_all(&day).expect("photo directory");
    fs::write(day.join("A.NEF"), b"raw").expect("raw photo");

    assert!(
        ratings::load_ratings(&database, &root)
            .expect("empty ratings")
            .is_empty()
    );
    let update = ratings::set_rating(&database, &root, "day/A.NEF", 5).expect("set rating");
    assert_eq!(update.asset_id, "day/a");
    assert_eq!(update.rating, 5);
    assert_eq!(
        ratings::load_ratings(&database, &root).expect("saved ratings")["day/a"],
        5
    );

    ratings::set_rating(&database, &root, "day/A.NEF", 0).expect("clear rating");
    assert!(
        ratings::load_ratings(&database, &root)
            .expect("cleared ratings")
            .is_empty()
    );
}

#[test]
fn ratings_reject_unsupported_or_escaping_paths() {
    let temp = tempfile::tempdir().expect("temp directory");
    let root = temp.path().join("photos");
    let database = temp.path().join("ratings.json");
    fs::create_dir_all(&root).expect("photo directory");
    fs::write(root.join("notes.txt"), b"notes").expect("unsupported file");

    assert!(ratings::set_rating(&database, &root, "notes.txt", 4).is_err());
    assert!(ratings::set_rating(&database, &root, "../outside.NEF", 4).is_err());
    assert!(ratings::set_rating(&database, &root, "missing.NEF", 4).is_err());
    assert!(ratings::set_rating(&database, &root, "notes.txt", 6).is_err());
}

#[test]
fn ratings_are_isolated_between_photo_roots() {
    let temp = tempfile::tempdir().expect("temp directory");
    let database = temp.path().join("ratings.json");
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    fs::create_dir_all(&first).expect("first root");
    fs::create_dir_all(&second).expect("second root");
    fs::write(first.join("same.JPG"), b"jpg").expect("first photo");
    fs::write(second.join("same.JPG"), b"jpg").expect("second photo");

    ratings::set_rating(&database, &first, "same.JPG", 4).expect("first rating");
    assert_eq!(
        ratings::load_ratings(&database, &first).expect("first ratings")["same"],
        4
    );
    assert!(
        ratings::load_ratings(&database, &second)
            .expect("second ratings")
            .is_empty()
    );
}
