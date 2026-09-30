use super::*;
use crate::writer_tests::disk_fixture;
use keel_core::graph_lock;
use keel_core::store::GraphStore;

#[tokio::test(flavor = "current_thread")]
async fn busy_batch_is_retained_merged_and_retried_without_blocking() {
    let (dir, store, engine, source) = disk_fixture(true);
    let held = graph_lock::try_acquire(&dir.path().join(".keel")).unwrap();
    let (tx, rx) = mpsc::channel(8);
    let task = tokio::spawn(run_batches(engine, dir.path().into(), rx));
    tx.send(WatchBatch {
        changed: vec![source.clone()],
        removed: vec![],
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(store.get_node("seed_hash").is_some());
    // A later deletion supersedes the queued compilation of the same path.
    tx.send(WatchBatch {
        changed: vec![],
        removed: vec![source],
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(store.get_node("seed_hash").is_some());
    drop(held);
    drop(tx);
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap();
    assert!(store.get_nodes_in_file("hello.rs").is_empty());
}

#[tokio::test]
async fn io_failure_drops_batch_and_loop_still_exits() {
    let (dir, store, engine, source) = disk_fixture(true);
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
    assert!(store.get_node("seed_hash").is_some());
}
