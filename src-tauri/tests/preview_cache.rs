#![allow(dead_code)]

#[path = "../src/preview_cache.rs"]
mod preview_cache;

use preview_cache::{PreviewCache, cache_for};
use std::fs;
use std::path::Path;

fn write_cache_file(root: &Path, relative: &str, size: usize) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("cache parent")).expect("create cache shard");
    fs::write(path, vec![0x5a; size]).expect("write cache file");
}

#[test]
fn sqlite_cache_persists_totals_without_discovering_orphan_files() {
    let root = tempfile::tempdir().expect("cache root");
    write_cache_file(root.path(), "aa/bb/one.jpg", 400);
    write_cache_file(root.path(), "orphan.jpg", 900);

    {
        let cache = PreviewCache::open(root.path(), 10_000, 100).expect("open cache");
        cache
            .record_generated("aa/bb/one.jpg", 400, 512, 10)
            .expect("record generated");
        assert_eq!(cache.stats().expect("stats").entry_count, 1);
        assert_eq!(cache.stats().expect("stats").size_bytes, 400);
    }

    let reopened = PreviewCache::open(root.path(), 10_000, 100).expect("reopen cache");
    let stats = reopened.stats().expect("reopened stats");
    assert_eq!(stats.entry_count, 1);
    assert_eq!(stats.size_bytes, 400);
}

#[test]
fn access_timestamps_flush_as_one_persistent_batch() {
    let root = tempfile::tempdir().expect("cache root");
    write_cache_file(root.path(), "aa/one.jpg", 300);
    write_cache_file(root.path(), "bb/two.jpg", 500);
    let cache = PreviewCache::open(root.path(), 10_000, 100).expect("open cache");
    cache
        .record_generated("aa/one.jpg", 300, 512, 10)
        .expect("record one");
    cache
        .record_generated("bb/two.jpg", 500, 1600, 20)
        .expect("record two");

    cache
        .record_access("aa/one.jpg", 300, 512, 50)
        .expect("touch one");
    cache
        .record_access("bb/two.jpg", 500, 1600, 60)
        .expect("touch two");
    assert_eq!(cache.pending_access_count(), 2);
    cache.flush_accesses().expect("flush accesses");
    assert_eq!(cache.pending_access_count(), 0);

    let reopened = PreviewCache::open(root.path(), 10_000, 100).expect("reopen cache");
    assert_eq!(reopened.last_access_ms("aa/one.jpg").unwrap(), Some(50));
    assert_eq!(reopened.last_access_ms("bb/two.jpg").unwrap(), Some(60));
}

#[test]
fn cache_pruning_removes_the_oldest_file_and_keeps_the_new_entry() {
    let root = tempfile::tempdir().expect("cache root");
    write_cache_file(root.path(), "aa/old.jpg", 400);
    write_cache_file(root.path(), "bb/new.jpg", 400);
    let cache = PreviewCache::open(root.path(), 700, 100).expect("open cache");
    cache
        .record_generated("aa/old.jpg", 400, 512, 10)
        .expect("record old");
    cache
        .record_generated("bb/new.jpg", 400, 1600, 20)
        .expect("record new");

    assert!(!root.path().join("aa/old.jpg").exists());
    assert!(root.path().join("bb/new.jpg").is_file());
    let stats = cache.stats().expect("stats");
    assert_eq!(stats.entry_count, 1);
    assert_eq!(stats.size_bytes, 400);
}

#[test]
fn missing_cache_files_are_removed_from_metadata() {
    let root = tempfile::tempdir().expect("cache root");
    write_cache_file(root.path(), "aa/missing.jpg", 400);
    let cache = PreviewCache::open(root.path(), 10_000, 100).expect("open cache");
    cache
        .record_generated("aa/missing.jpg", 400, 512, 10)
        .expect("record generated");
    fs::remove_file(root.path().join("aa/missing.jpg")).expect("remove cache file");

    cache
        .remove_missing("aa/missing.jpg")
        .expect("remove missing metadata");
    assert_eq!(cache.stats().expect("stats").entry_count, 0);
    assert_eq!(cache.stats().expect("stats").size_bytes, 0);
}

#[test]
fn legacy_json_index_is_imported_once() {
    let root = tempfile::tempdir().expect("cache root");
    write_cache_file(root.path(), "legacy.jpg", 250);
    fs::write(
        root.path().join("preview-cache-index-v1.json"),
        r#"{"schemaVersion":1,"entries":{"legacy.jpg":{"sizeBytes":250,"lastAccessMs":42,"maxEdge":512}}}"#,
    )
    .expect("write legacy index");

    let cache = PreviewCache::open(root.path(), 10_000, 100).expect("migrate cache");
    assert_eq!(cache.stats().expect("stats").entry_count, 1);
    assert_eq!(cache.last_access_ms("legacy.jpg").unwrap(), Some(42));
    assert!(!root.path().join("preview-cache-index-v1.json").exists());
    assert!(
        root.path()
            .join("preview-cache-index-v1.json.migrated")
            .is_file()
    );
}

#[test]
fn damaged_legacy_index_is_quarantined_without_blocking_sqlite_cache() {
    let root = tempfile::tempdir().expect("cache root");
    fs::write(
        root.path().join("preview-cache-index-v1.json"),
        b"{not valid json",
    )
    .expect("write damaged legacy index");

    let cache = PreviewCache::open(root.path(), 10_000, 100).expect("open new cache");

    assert_eq!(cache.stats().expect("stats").entry_count, 0);
    assert!(!root.path().join("preview-cache-index-v1.json").exists());
    assert!(
        root.path()
            .join("preview-cache-index-v1.json.invalid")
            .is_file()
    );
}

#[test]
fn process_cache_registry_keeps_pending_accesses_between_calls() {
    let root = tempfile::tempdir().expect("cache root");
    write_cache_file(root.path(), "aa/one.jpg", 300);
    let cache = cache_for(root.path(), 10_000, 100).expect("first cache handle");
    cache
        .record_generated("aa/one.jpg", 300, 512, 10)
        .expect("record generated");
    cache
        .record_access("aa/one.jpg", 300, 512, 50)
        .expect("queue access");
    assert_eq!(cache.pending_access_count(), 1);
    drop(cache);

    let reused = cache_for(root.path(), 10_000, 100).expect("reused cache handle");
    assert_eq!(reused.pending_access_count(), 1);
}

#[test]
fn batched_access_updates_cached_size_totals() {
    let root = tempfile::tempdir().expect("cache root");
    write_cache_file(root.path(), "aa/one.jpg", 300);
    let cache = PreviewCache::open(root.path(), 10_000, 100).expect("open cache");
    cache
        .record_generated("aa/one.jpg", 300, 512, 10)
        .expect("record generated");
    write_cache_file(root.path(), "aa/one.jpg", 450);
    cache
        .record_access("aa/one.jpg", 450, 512, 50)
        .expect("queue changed size");
    cache.flush_accesses().expect("flush accesses");

    assert_eq!(cache.stats().expect("stats").size_bytes, 450);
}
