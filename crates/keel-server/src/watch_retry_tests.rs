use super::*;
use crate::writer_test_support::state;
use crate::writer_tests::disk_fixture;
use keel_core::graph_lock;
use keel_core::store::GraphStore;
use keel_core::types::NodeChange;
use std::time::Instant;

#[tokio::test(flavor = "current_thread")]
async fn busy_batch_is_retained_merged_and_retried_without_blocking() {
    let (dir, store, engine, source) = disk_fixture(true);
    let second = dir.path().join("second.rs");
    std::fs::write(&second, "fn second() -> i32 { 99 }\n").unwrap();
    let mut node = store.get_node_by_id(1).unwrap();
    node.id = 2;
    node.name = "second".into();
    node.file_path = "second.rs".into();
    node.signature = "fn second() -> i32".into();
    node.hash = "second_seed".into();
    store.insert_node(&node).unwrap();
    let before = state(dir.path());
    let held = graph_lock::try_acquire(&dir.path().join(".keel")).unwrap();
    let (tx, rx) = mpsc::channel(8);
    let task = tokio::spawn(run_batches(engine, dir.path().into(), rx));
    let start = Instant::now();
    tx.send(WatchBatch {
        changed: vec![source.clone(), second],
        removed: vec![],
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "retry stalled runtime"
    );
    assert_eq!(state(dir.path()), before);
    // A later deletion supersedes only this path; the second file must survive
    // merging and still compile even though it appears only in the first batch.
    std::fs::remove_file(&source).unwrap();
    tx.send(WatchBatch {
        changed: vec![],
        removed: vec![source],
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "merge stalled runtime"
    );
    assert_eq!(state(dir.path()), before);
    drop(held);
    drop(tx);
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap();
    assert!(store.get_nodes_in_file("hello.rs").is_empty());
    assert_ne!(store.get_node_by_id(2).unwrap().hash, "second_seed");
}

#[tokio::test(flavor = "current_thread")]
async fn retained_removal_recreated_and_mapped_during_contention_survives_retry() {
    let (dir, mut store, engine, source) = disk_fixture(true);
    let held = graph_lock::try_acquire(&dir.path().join(".keel")).unwrap();
    std::fs::remove_file(&source).unwrap();
    let (tx, rx) = mpsc::channel(8);
    let task = tokio::spawn(run_batches(engine, dir.path().into(), rx));
    tx.send(WatchBatch {
        changed: vec![],
        removed: vec![source.clone()],
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    // Simulate map completing with the recreated file. No recreation event is
    // delivered before retry, matching an event still inside debounce.
    std::fs::write(&source, "fn hello() -> i32 { 73 }\n").unwrap();
    let index = crate::parse_shared::FileParser::new()
        .parse(source.to_str().unwrap())
        .unwrap();
    let mut node = store.get_node_by_id(1).unwrap();
    node.hash = index.definitions[0].hash();
    let mapped_hash = node.hash.clone();
    store.update_nodes(vec![NodeChange::Update(node)]).unwrap();
    drop(held);
    drop(tx);
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(store.get_nodes_in_file("hello.rs").len(), 1);
    assert_eq!(store.get_node_by_id(1).unwrap().hash, mapped_hash);
}

#[test]
fn stale_changed_path_missing_on_disk_is_pruned() {
    let (dir, store, engine, source) = disk_fixture(true);
    std::fs::remove_file(&source).unwrap();
    let outcome = crate::watcher::apply_batch(
        &engine,
        dir.path(),
        &WatchBatch {
            changed: vec![source],
            removed: vec![],
        },
    )
    .unwrap();
    assert_eq!(outcome.pruned, 1);
    assert_eq!(outcome.compiled, 0);
    assert!(store.get_nodes_in_file("hello.rs").is_empty());
}

#[tokio::test]
async fn io_failure_drops_batch_and_loop_still_exits() {
    let (dir, _store, engine, source) = disk_fixture(true);
    let before = state(dir.path());
    std::fs::remove_file(dir.path().join(".keel/compile.lock")).unwrap();
    std::fs::create_dir(dir.path().join(".keel/compile.lock")).unwrap();
    let (tx, rx) = mpsc::channel(8);
    tx.send(WatchBatch {
        changed: vec![],
        removed: vec![source],
    })
    .await
    .unwrap();
    drop(tx);
    tokio::time::timeout(
        Duration::from_secs(1),
        run_batches(engine, dir.path().into(), rx),
    )
    .await
    .unwrap();
    assert_eq!(state(dir.path()), before);
}
