use keel_parsers::resolver::ParseCache;
use keel_parsers::treesitter::TreeSitterParser;
use std::path::Path;

#[test]
fn parse_cache_discards_syntax_tree_without_changing_live_parse() {
    let path = Path::new("home.rs");
    let source = "fn home() -> &'static str { \"DOMAIN fixed::segment with punctuation\" }";
    let parsed = TreeSitterParser::new()
        .parse_file("rust", path, source)
        .expect("source should parse");
    let cache = ParseCache::default();
    cache.insert(path, parsed.clone());

    let cached = cache.get(path).expect("parse should be cached");
    assert!(cached.syntax_tree.is_none());
    assert!(cache.lock().get(path).unwrap().syntax_tree.is_none());
    assert_eq!(cached.definitions.len(), parsed.definitions.len());
    assert_eq!(cached.definitions[0].hash(), parsed.definitions[0].hash());
    assert_eq!(cache.definitions_for(path)[0].name, "home");
    assert!(parsed.syntax_tree.is_some());
    assert_eq!(
        parsed.syntax_tree.unwrap().root_node().end_byte(),
        source.len()
    );
}
