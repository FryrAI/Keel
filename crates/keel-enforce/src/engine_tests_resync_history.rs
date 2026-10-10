use super::*;
use crate::test_fixtures::{file_index, node_for_definition};

#[test]
fn ordinal_salted_unchanged_method_keeps_base_history() {
    let file = "src/lib.rs";
    let mut definitions = Vec::new();
    for (idx, body) in ["1", "2", "3"].iter().enumerate() {
        let mut def = make_definition("same", "pub fn same(&self) -> u32", body, file);
        def.line_start = 4 + idx as u32 * 6;
        def.line_end = def.line_start + 2;
        def.is_associated = true;
        definitions.push(def);
    }
    let nodes: Vec<_> = definitions
        .iter()
        .enumerate()
        .map(|(idx, def)| node_for_definition(idx as u64 + 1, def))
        .collect();
    let old_hash = nodes[2].hash.clone();
    let mut store = SqliteGraphStore::in_memory().unwrap();
    store
        .update_nodes(
            nodes
                .into_iter()
                .map(keel_core::types::NodeChange::Add)
                .collect(),
        )
        .unwrap();
    let mut engine = EnforcementEngine::new(Box::new(store));
    for def in &mut definitions {
        def.body_text = "1".into();
    }
    let parsed = file_index(file, definitions);
    engine.compile(std::slice::from_ref(&parsed));
    let before = engine.store.get_node_by_id(3).unwrap();
    assert_eq!(before.previous_hashes, vec![old_hash.clone()]);
    engine.compile(&[parsed]);
    let after = engine.store.get_node_by_id(3).unwrap();
    println!(
        "third method history: {:?}; current: {}",
        after.previous_hashes, after.hash
    );
    assert_eq!(
        after.previous_hashes,
        vec![old_hash, before.hash],
        "ordinary compile must retain the base engine's self-history entry"
    );
}
