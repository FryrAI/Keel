/// Compute the (plain, disambiguated) hash pair for a definition — the two
/// identities `keel map` may have stored for it (map salts with the file
/// path on collision). Single source of truth for "does this def match its
/// stored node", shared by the engine's hash sync and progressive adoption.
pub fn definition_hashes(
    def: &keel_parsers::resolver::Definition,
    file_path: &str,
) -> (String, String) {
    // Normalize the body once and reuse it for both identities — routing
    // through `Definition::{hash, hash_disambiguated}` would normalize twice.
    let doc = def.docstring.as_deref().unwrap_or("");
    let body = def.body_for_hash();
    (
        keel_core::hash::compute_hash(&def.signature, &body, doc),
        keel_core::hash::compute_hash_disambiguated(&def.signature, &body, doc, file_path),
    )
}

/// Compute one disambiguated hash for a definition under an arbitrary salt.
///
/// [`definition_hashes`] covers the two identities `keel map` assigns (plain,
/// and file-path-salted). A file holding three or more identical same-named
/// definitions needs more than two distinct identities, so the engine's
/// re-baseline walks an ordinal (`"<file>#2"`, `"<file>#3"`, …) through this.
/// Off the hot path — it re-normalizes the body — and only reached once a
/// collision is already proven.
pub fn definition_hash_salted(def: &keel_parsers::resolver::Definition, salt: &str) -> String {
    keel_core::hash::compute_hash_disambiguated(
        &def.signature,
        &def.body_for_hash(),
        def.docstring.as_deref().unwrap_or(""),
        salt,
    )
}

/// Whether a stored node's hash matches the definition under either of the
/// two identities from [`definition_hashes`].
pub fn node_hash_matches(
    node: &keel_core::types::GraphNode,
    def: &keel_parsers::resolver::Definition,
    file_path: &str,
) -> bool {
    let (hash, hash_d) = definition_hashes(def, file_path);
    node.hash == hash || node.hash == hash_d
}

/// Pair a freshly parsed definition with the stored node that IS it, or
/// `None` when the pairing is genuinely undecidable.
///
/// Every check that compares a parse against the graph needs this, and each
/// one used to inline its own version. The strategy, in order:
///
/// 1. **Hash evidence wins.** A same-named node whose hash matches under
///    either identity from [`definition_hashes`] is this definition, however
///    many same-named siblings surround it.
/// 2. **Otherwise the name must be unique on BOTH sides** — exactly one
///    definition in the parse and exactly one node in the store. A repeated
///    name with no hash evidence is a coin flip (a free `search_graph` beside
///    a `search_graph` method), and comparing the wrong pair manufactures
///    findings out of nothing.
/// 3. **Otherwise refuse.** Undecidable is a real answer; guessing is not.
///
/// A signature change moves the hash, so a *changed* definition never has
/// evidence under rule 1 — for repeated names that is precisely the case rule
/// 2 must refuse.
pub fn bind_to_node<'a>(
    def: &keel_parsers::resolver::Definition,
    file: &keel_parsers::resolver::FileIndex,
    nodes: &'a [keel_core::types::GraphNode],
) -> Option<&'a keel_core::types::GraphNode> {
    let candidates: Vec<&keel_core::types::GraphNode> =
        nodes.iter().filter(|n| n.name == def.name).collect();
    if let Some(matched) = candidates
        .iter()
        .find(|n| node_hash_matches(n, def, &file.file_path))
    {
        return Some(matched);
    }
    let unique_in_parse = file
        .definitions
        .iter()
        .filter(|d| d.name == def.name)
        .count()
        == 1;
    (candidates.len() == 1 && unique_in_parse).then(|| candidates[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::{definition, file_index, function_node, node_for_definition};

    #[test]
    fn bind_to_node_prefers_hash_evidence_over_the_name() {
        // Two stored `run`s; only one carries this def's body. The name alone
        // would be a coin flip, the hash is decisive.
        let def = definition("run", "src/a.rs", false);
        let file = file_index("src/a.rs", vec![def.clone()]);
        let nodes = vec![
            function_node(1, "otherhash11", "run", "src/a.rs"),
            node_for_definition(2, &def),
        ];
        assert_eq!(bind_to_node(&def, &file, &nodes).map(|n| n.id), Some(2));
    }

    #[test]
    fn bind_to_node_falls_back_only_when_the_name_is_unique_on_both_sides() {
        // No hash evidence (the stored signature/body moved on), one node and
        // one definition with the name: the pairing is still forced.
        let def = definition("run", "src/a.rs", false);
        let file = file_index("src/a.rs", vec![def.clone()]);
        let nodes = vec![function_node(1, "otherhash11", "run", "src/a.rs")];
        assert_eq!(bind_to_node(&def, &file, &nodes).map(|n| n.id), Some(1));
    }

    #[test]
    fn bind_to_node_refuses_repeats_on_either_side() {
        let def = definition("run", "src/a.rs", false);
        // Repeated in the parse (a free fn beside a method of the same name).
        let two_defs = file_index("src/a.rs", vec![def.clone(), def.clone()]);
        let one_node = vec![function_node(1, "otherhash11", "run", "src/a.rs")];
        assert!(bind_to_node(&def, &two_defs, &one_node).is_none());

        // Repeated in the store (one of the pair was just deleted).
        let one_def = file_index("src/a.rs", vec![def.clone()]);
        let two_nodes = vec![
            function_node(1, "otherhash11", "run", "src/a.rs"),
            function_node(2, "otherhash22", "run", "src/a.rs"),
        ];
        assert!(bind_to_node(&def, &one_def, &two_nodes).is_none());
    }
}
