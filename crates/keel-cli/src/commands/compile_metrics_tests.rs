use super::*;

#[test]
fn build_language_mix_empty() {
    let files: Vec<String> = vec![];
    let mix = build_language_mix(&files);
    assert!(mix.is_empty());
}

#[test]
fn build_language_mix_single_language() {
    let files = vec![
        "src/main.rs".to_string(),
        "src/lib.rs".to_string(),
        "src/utils.rs".to_string(),
    ];
    let mix = build_language_mix(&files);
    assert_eq!(mix.get("rust"), Some(&100));
    assert_eq!(mix.len(), 1);
}

#[test]
fn build_language_mix_multiple_languages() {
    let files = vec![
        "src/main.ts".to_string(),
        "src/app.ts".to_string(),
        "src/lib.py".to_string(),
        "src/go_mod.go".to_string(),
    ];
    let mix = build_language_mix(&files);
    // 2 ts, 1 py, 1 go = 50%, 25%, 25%
    assert_eq!(mix.get("typescript"), Some(&50));
    assert_eq!(mix.get("python"), Some(&25));
    assert_eq!(mix.get("go"), Some(&25));
}

#[test]
fn build_language_mix_unknown_extensions_ignored() {
    let files = vec![
        "README.md".to_string(),
        "Dockerfile".to_string(),
        "src/main.rs".to_string(),
    ];
    let mix = build_language_mix(&files);
    // Only .rs is recognized
    assert_eq!(mix.get("rust"), Some(&100));
    assert_eq!(mix.len(), 1);
}

#[test]
fn build_language_mix_all_unknown() {
    let files = vec![
        "README.md".to_string(),
        "config.yaml".to_string(),
    ];
    let mix = build_language_mix(&files);
    assert!(mix.is_empty());
}
