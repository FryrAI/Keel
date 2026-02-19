use super::{count_graph_nodes, ensure_repo, run_keel};

const REPO_NAME: &str = "ky";
const REPO_URL: &str = "https://github.com/sindresorhus/ky.git";
const REPO_TAG: &str = "v1.7.5";

#[test]
#[ignore] // requires network + clone
fn corpus_ky_map() {
    let dir = match ensure_repo(REPO_NAME, REPO_URL, REPO_TAG) {
        Some(d) => d,
        None => {
            eprintln!("skipping {REPO_NAME}: clone failed (no network?)");
            return;
        }
    };

    // keel init
    let (code, _out, err) = run_keel(&dir, &["init", "--yes"]);
    assert!(code == 0, "keel init failed (exit {code}): {err}");

    // keel map
    let (code, _out, err) = run_keel(&dir, &["map"]);
    assert!(code == 0, "keel map failed (exit {code}): {err}");

    // Verify graph has nodes
    let nodes = count_graph_nodes(&dir);
    assert!(
        nodes > 10,
        "expected >10 graph nodes for {REPO_NAME}, got {nodes}"
    );
    eprintln!("{REPO_NAME}: {nodes} nodes");
}
