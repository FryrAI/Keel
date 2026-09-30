use super::*;

const SEGMENT: &str = "AT TIME ZONE 'Europe/Berlin')::date";
const RUST_HOME: &str =
    r#"fn berlin(x: &str) -> String { format!("({x} AT TIME ZONE 'Europe/Berlin')::date") }"#;

fn rust_homes() -> Vec<TemplateHome> {
    extract_homes("home.rs", RUST_HOME)
}

#[test]
fn rust_tail_return_raw_format_and_concat_homes() {
    let source = r##"
fn a(x: &str) -> String { format!("({x} AT TIME ZONE 'Europe/Berlin')::date") }
fn b() -> &'static str { return r#"AT TIME ZONE 'Europe/Berlin')::date"#; }
fn c() -> &'static str { concat!("AT TIME ZONE ", "'Europe/Berlin')::date") }
fn d() -> &'static str { "Request failed: invalid token" }
fn e() { "Request failed: invalid token"; }
"##;
    let homes = extract_homes("home.rs", source);
    assert_eq!(
        homes.iter().map(|h| h.name.as_str()).collect::<Vec<_>>(),
        vec!["a", "b", "c", "d"]
    );
    assert!(homes[..3].iter().all(|h| h.segments == vec![SEGMENT]));
}

#[test]
fn python_fstring_nested_expression_and_format_spec_do_not_leak() {
    let source = r#"
def home(x):
    return f"{ {'key': x} :{x}} AT TIME ZONE 'Europe/Berlin')::date"
def formatted(x):
    return "({x!r:>{width}} AT TIME ZONE 'Europe/Berlin')::date".format(x=x, width=10)
def adjacent():
    return "AT TIME ZONE " "'Europe/Berlin')::date"
"#;
    let homes = extract_homes("home.py", source);
    assert_eq!(homes.len(), 3);
    assert!(homes.iter().all(|h| h.segments == vec![SEGMENT]));
}

#[test]
fn typescript_javascript_tsx_and_jsx_templates() {
    for extension in ["ts", "js", "tsx", "jsx"] {
        let source = "const home = (x) => `(${({key: x}).key} AT TIME ZONE 'Europe/Berlin')::date`;\nfunction plain() { return \"Request failed: invalid token\"; }";
        let homes = extract_homes(&format!("home.{extension}"), source);
        assert_eq!(homes.len(), 2, "{extension}");
        assert_eq!(homes[0].segments, vec![SEGMENT]);
        assert!(!homes[0].segments.iter().any(|s| s.contains("key")));
    }
}

#[test]
fn go_literal_raw_sprintf_percent_and_indexed_verbs() {
    let source = "package p\nimport \"fmt\"\nfunc home(x string) string { return fmt.Sprintf(\"(%[1]*.[2]*[3]s AT TIME ZONE 'Europe/Berlin')::date\", 1, 2, x) }\nfunc raw() string { return `AT TIME ZONE 'Europe/Berlin')::date` }\nfunc percent() string { return fmt.Sprintf(\"SELECT ratio * 100%% AS percent\") }";
    let homes = extract_homes("home.go", source);
    assert_eq!(homes.len(), 3);
    assert_eq!(homes[0].segments, vec![SEGMENT]);
    assert_eq!(homes[1].segments, vec![SEGMENT]);
    assert_eq!(homes[2].segments, vec!["SELECT ratio * 100% AS percent"]);
}

#[test]
fn go_sprintf_requires_standard_import_and_unshadowed_fmt() {
    for source in [
        "package p\nimport fmt \"unrelated\"\nfunc home(x string) string { return fmt.Sprintf(\"AT TIME ZONE 'Europe/Berlin')::date\", x) }",
        "package p\nimport \"fmt\"\nfunc home(fmt Formatter) string { return fmt.Sprintf(\"AT TIME ZONE 'Europe/Berlin')::date\") }",
        "package p\nfunc home() string { return fmt.Sprintf(\"AT TIME ZONE 'Europe/Berlin')::date\") }",
    ] { assert!(extract_homes("home.go", source).is_empty()); }
}

#[test]
fn segment_threshold_character_count_expression_filter_and_shared_owners() {
    assert!(!eligible_segment("a long ordinary message", 16));
    assert!(!eligible_segment("a_long_identifier_name", 16));
    assert!(!eligible_segment("x: tiny", 16));
    assert!(eligible_segment("SELECT value::date", 16));
    assert!(!eligible_segment("SELECT value::date", 24));
    assert!(eligible_segment("ééééééééééééééé:", 16));
    let mut homes = rust_homes();
    homes.extend(extract_homes(
        "other.rs",
        &RUST_HOME.replace("berlin", "other"),
    ));
    let matches = occurrences(
        "caller.rs",
        r#"const SQL: &str = "SELECT (now() AT TIME ZONE 'Europe/Berlin')::date";"#,
        &homes,
    );
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].homes.len(), 2);
    assert!(occurrences("home.rs", RUST_HOME, &homes).is_empty());
    assert!(occurrences("other.rs", &RUST_HOME.replace("berlin", "other"), &homes).is_empty());
}

#[test]
fn module_literals_concat_and_unicode_escape_match_at_start_line() {
    let source = r#"// "AT TIME ZONE 'Europe/Berlin')::date"
const SQL: &str = "SELECT (now() AT TIME ZONE\u{20}'Europe/Berlin')::date";
const JOINED: &str = concat!("SELECT (now() AT TIME ZONE ", "'Europe/Berlin')::date");
static MULTILINE: &str = "SELECT (
now() AT TIME ZONE 'Europe/Berlin')::date";
"#;
    let matches = occurrences("caller.rs", source, &rust_homes());
    assert_eq!(matches.len(), 3);
    assert_eq!(
        matches.iter().map(|m| m.line).collect::<Vec<_>>(),
        vec![2, 3, 4]
    );
    assert!(matches[0].literal.contains(SEGMENT));
}

#[test]
fn decoded_and_adjacent_literals_match_in_each_language() {
    let homes = rust_homes();
    for (file, source) in [
        (
            "caller.py",
            r#"SQL = "SELECT (now() AT TIME ZONE\u0020" "'Europe/Berlin')::date""#,
        ),
        (
            "caller.ts",
            r#"const SQL = "SELECT (now() AT TIME ZONE\u0020'Europe/Berlin')::date";"#,
        ),
        (
            "caller.go",
            "package p\nconst SQL = \"SELECT (now() AT TIME ZONE\\u0020'Europe/Berlin')::date\"",
        ),
    ] {
        assert_eq!(occurrences(file, source, &homes).len(), 1, "{file}");
    }
}

#[test]
fn side_effects_branches_multiple_returns_and_nested_functions_reject_homes() {
    for (file, source) in [
        ("home.py", "def failure(user):\n    revoke(user)\n    return \"Request failed: invalid token\"\n"),
        ("home.rs", "fn berlin(x: &str) -> String { if unavailable { return fallback(); } format!(\"({x} AT TIME ZONE 'Europe/Berlin')::date\") }"),
        ("home.ts", "function home(x) { if (x) return 'Request failed: invalid token'; return 'Request failed: invalid token'; }"),
        ("home.go", "package p\nfunc home() string { log(); return \"Request failed: invalid token\" }"),
    ] { assert!(extract_homes(file, source).is_empty(), "{file}"); }
    let homes = extract_homes("nested.py", "def outer():\n    def inner():\n        return \"Request failed: invalid token\"\n    return inner()\n");
    assert_eq!(homes.len(), 1);
    assert_eq!(homes[0].name, "inner");
}

#[test]
fn tests_comments_and_unsupported_whole_file_languages_are_excluded() {
    let source = format!("{RUST_HOME}\n#[cfg(test)] mod tests {{ fn expected() -> &'static str {{ \"{SEGMENT}\" }} const SQL: &str = \"{SEGMENT}\"; }}\n#[test] fn expected() -> &'static str {{ \"{SEGMENT}\" }}");
    let homes = extract_homes("home.rs", &source);
    assert_eq!(homes.len(), 1);
    assert!(occurrences("home.rs", &source, &homes).is_empty());
    assert!(extract_homes("tests/home.rs", RUST_HOME).is_empty());
    assert!(occurrences(
        "tests/caller.rs",
        &format!("const SQL: &str = \"{SEGMENT}\";"),
        &homes
    )
    .is_empty());
    assert!(extract_homes(
        "home.py",
        "def test_expected():\n    return \"Request failed: invalid token\"\n"
    )
    .is_empty());
    assert!(occurrences(
        "home.ts",
        &format!("test('case', () => {{ const sql = \"{SEGMENT}\"; }});"),
        &homes
    )
    .is_empty());
    for ext in ["svelte", "astro", "sql", "typ", "baml"] {
        assert!(extract_homes(&format!("home.{ext}"), RUST_HOME).is_empty());
    }
}

#[test]
fn escaped_braces_and_template_substitutions_keep_literal_text() {
    assert_eq!(
        decode::brace_segments("{{literal}} {value:{width}} suffix::expression").unwrap(),
        vec!["{literal} ", " suffix::expression"]
    );
    let homes = extract_homes(
        "home.py",
        "def home(x):\n    return f\"{{literal}}:{x} AT TIME ZONE 'Europe/Berlin')::date\"\n",
    );
    assert_eq!(homes[0].segments, vec![SEGMENT]);
    let homes = extract_homes(
        "home.ts",
        r#"const home = (x) => `\${literal} AT TIME ZONE 'Europe/Berlin')::date`;"#,
    );
    assert_eq!(
        homes[0].segments,
        vec!["${literal} AT TIME ZONE 'Europe/Berlin')::date"]
    );
}

#[test]
fn decoding_standard_escapes_raw_bytes_and_unicode() {
    assert_eq!(
        decode::decode(r"a\n\t\x41\u{1f642}", "rust", false).unwrap(),
        "a\n\tA🙂"
    );
    assert_eq!(
        decode::decode(r"\uD83D\uDE42", "typescript", false).unwrap(),
        "🙂"
    );
    assert_eq!(
        decode::decode(r"\101\u0042\U00000043", "python", false).unwrap(),
        "ABC"
    );
    assert_eq!(
        decode::decode(r"\101\u0042\U00000043", "go", false).unwrap(),
        "ABC"
    );
    assert_eq!(decode::decode(r"\n", "rust", true).unwrap(), r"\n");
    let homes = extract_homes(
        "bytes.rs",
        r#"fn raw() -> &'static [u8] { b"AT TIME ZONE\n'Europe/Berlin')::date" }"#,
    );
    assert_eq!(
        homes[0].segments,
        vec![r"AT TIME ZONE\n'Europe/Berlin')::date"]
    );
}

#[test]
fn multiset_identity_survives_hash_changes_whitespace_and_moves() {
    let one = occurrences(
        "caller.rs",
        &format!("const SQL: &str = \"{SEGMENT}\";"),
        &rust_homes(),
    )
    .remove(0);
    let mut moved = one.clone();
    moved.file = "moved.rs".into();
    moved.line = 99;
    moved.homes[0].hash = "changed".into();
    moved.literal = moved.literal.replace(' ', "   ");
    for part in &mut moved.literal_parts {
        *part = part.replace(' ', "   ");
    }
    assert!(git::subtract(vec![moved.clone()], vec![one.clone()]).is_empty());
    assert_eq!(
        git::subtract(vec![moved.clone(), moved], vec![one]).len(),
        1
    );
}

#[test]
fn interpolation_holes_and_plus_chains_do_not_invent_contiguous_text() {
    let source = "const SQL = `AT TIME ZONE '${zone}/Berlin')::date`;\nconst joined = \"AT TIME ZONE \" + \"'Europe/Berlin')::date\";";
    assert!(occurrences("caller.ts", source, &rust_homes()).is_empty());
}

#[test]
fn same_line_nested_homes_keep_their_own_names_and_parentheses_are_transparent() {
    let homes = extract_homes(
        "home.rs",
        r#"fn outer() { fn inner() -> &'static str { ("Request failed: invalid token") } }"#,
    );
    assert_eq!(homes.len(), 1);
    assert_eq!(homes[0].name, "inner");
    let homes = extract_homes(
        "home.ts",
        "function outer() { const inner = () => 'Request failed: invalid token'; return inner; }",
    );
    assert_eq!(homes.len(), 1);
    assert_eq!(homes[0].name, "inner");
}

#[test]
fn go_multi_name_and_variadic_fmt_bindings_disqualify_sprintf() {
    for params in ["a, fmt Formatter", "fmt ...Formatter"] {
        let source = format!("package p\nimport \"fmt\"\nfunc home({params}) string {{ return fmt.Sprintf(\"Request failed: invalid token\") }}");
        assert!(extract_homes("home.go", &source).is_empty());
    }
    assert_eq!(decode::decode(r"a\0b", "rust", false).unwrap(), "a\0b");
}

#[test]
fn parenthesized_arrow_and_unsupported_named_unicode_escape_are_sound() {
    let homes = extract_homes(
        "home.ts",
        "const home = () => ('Request failed: invalid token');",
    );
    assert_eq!(homes.len(), 1);
    assert_eq!(homes[0].name, "home");
    // No Unicode-name database is bundled. Refusing the literal is preferable
    // to treating its source escape as decoded runtime text in a precision study.
    assert!(decode::decode(r"\N{SPACE}", "python", false).is_none());
    assert_eq!(
        decode::decode(r"\N{SPACE}", "python", true).unwrap(),
        r"\N{SPACE}"
    );
}

#[test]
fn non_test_generated_paths_follow_the_same_whole_file_grammar_population() {
    let homes = extract_homes("baml_client/home.rs", RUST_HOME);
    assert_eq!(homes.len(), 1);
    assert_eq!(
        occurrences(
            "baml_client/caller.rs",
            &format!("const SQL: &str = \"{SEGMENT}\";"),
            &homes
        )
        .len(),
        1
    );
    assert!(extract_homes("baml_client/tests/home.rs", RUST_HOME).is_empty());
}

#[test]
fn fstring_hex_braces_remain_literal_and_nul_never_bridges_interpolations() {
    let homes = extract_homes(
        "home.py",
        r#"def home():
    return f"\x7b\x7bRequest failed: invalid token"
"#,
    );
    assert_eq!(homes[0].segments, vec!["{{Request failed: invalid token"]);
    let homes = extract_homes(
        "home.rs",
        r#"fn home() -> &'static str { "Long prefix expression\0 with suffix" }"#,
    );
    assert!(occurrences(
        "caller.py",
        "SQL = f\"Long prefix expression{x} with suffix\"",
        &homes
    )
    .is_empty());
    assert_eq!(
        occurrences(
            "caller.rs",
            r#"const SQL: &str = "Long prefix expression\0 with suffix";"#,
            &homes
        )
        .len(),
        1
    );
    let homes = rust_homes();
    let literal = format!("const SQL: &str = \"{SEGMENT}\\0suffix\";");
    let dynamic = format!("SQL = f\"{SEGMENT}{{x}}suffix\"");
    let base = occurrences("base.rs", &literal, &homes);
    let head = occurrences("head.py", &dynamic, &homes);
    assert_eq!(git::subtract(head, base).len(), 1);
}
