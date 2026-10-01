//! Round-one review counter-cases.
use super::{extract_homes, occurrences};
const SEG: &str = "DOMAIN fixed::segment with punctuation";
fn homes() -> Vec<keel_core::template_homes::TemplateHome> {
    extract_homes(
        "home.rs",
        &format!("fn home() -> &'static str {{ \"{SEG}\" }}"),
    )
}
#[test]
fn edited_owner_body_is_excluded_even_when_ineligible() {
    for source in [
        format!("\n\nfn home() -> &'static str {{ \"{SEG} SUFFIX\" }}"),
        format!("fn home() -> &'static str {{ log(); \"{SEG}\" }}"),
    ] {
        assert!(occurrences("home.rs", &source, &homes()).is_empty());
    }
}
#[test]
fn renamed_and_moved_home_bodies_are_excluded_without_remap() {
    for file in ["home.rs", "moved.rs"] {
        assert!(occurrences(
            file,
            &format!("fn renamed() -> &'static str {{ \"{SEG}\" }}"),
            &homes()
        )
        .is_empty());
    }
}
#[test]
fn crlf_literals_match_runtime_newlines() {
    for (file, source, copy) in [
        (
            "home.rs",
            "fn home() -> &'static str { \"DOMAIN fixed::segment\r\nwith punctuation\" }",
            "const COPY: &str = \"DOMAIN fixed::segment\\nwith punctuation\";",
        ),
        (
            "home.py",
            "def home():\n    return \"\"\"DOMAIN fixed::segment\r\nwith punctuation\"\"\"",
            "COPY = \"DOMAIN fixed::segment\\nwith punctuation\"",
        ),
        (
            "home.ts",
            "const home = () => `DOMAIN fixed::segment\r\nwith punctuation`;",
            "const COPY = \"DOMAIN fixed::segment\\nwith punctuation\";",
        ),
        (
            "home.go",
            "package p\nfunc home() string { return `DOMAIN fixed::segment\r\nwith punctuation` }",
            "package p\nconst COPY = \"DOMAIN fixed::segment\\nwith punctuation\"",
        ),
    ] {
        let mapped = extract_homes(file, source);
        assert_eq!(mapped.len(), 1, "{file}");
        assert_eq!(
            occurrences(&file.replace("home", "caller"), copy, &mapped).len(),
            1,
            "{file}"
        );
    }
}
#[test]
fn rust_crlf_continuation_matches_runtime_string() {
    let mapped = extract_homes(
        "home.rs",
        "fn home() -> &'static str { \"DOMAIN fixed::segment\\\r\n  with punctuation\" }",
    );
    assert_eq!(
        occurrences(
            "caller.rs",
            "const COPY: &str = \"DOMAIN fixed::segmentwith punctuation\";",
            &mapped
        )
        .len(),
        1
    );
}
#[test]
fn go_hex_and_octal_escapes_decode_as_utf8_bytes() {
    let mapped = extract_homes(
        "home.go",
        "package p\nfunc home() string { return \"DOMAIN é::segment with punctuation\" }",
    );
    for escape in [r"\xc3\xa9", r"\303\251"] {
        let copy = format!("package p\nconst COPY = \"DOMAIN {escape}::segment with punctuation\"");
        assert_eq!(occurrences("caller.go", &copy, &mapped).len(), 1);
    }
}
#[test]
fn javascript_uppercase_u_is_an_identity_escape() {
    let mapped = extract_homes(
        "home.ts",
        "const home = () => 'DOMAIN A::segment with punctuation';",
    );
    assert!(occurrences(
        "caller.ts",
        r"const COPY = 'DOMAIN \U00000041::segment with punctuation';",
        &mapped
    )
    .is_empty());
    let mapped = extract_homes(
        "home.ts",
        "const home = () => 'DOMAIN U00000041::segment with punctuation';",
    );
    assert_eq!(
        occurrences(
            "caller.ts",
            r"const COPY = 'DOMAIN \U00000041::segment with punctuation';",
            &mapped
        )
        .len(),
        1
    );
}
#[test]
fn formatted_occurrences_unescape_and_preserve_holes() {
    for (file, source, copy) in [
        ("home.rs", "fn home() -> String { format!(\"DOMAIN {{marker}}::segment with punctuation {x}\") }", "fn caller() { let x = format!(\"DOMAIN {{marker}}::segment with punctuation {x}\"); }"),
        ("home.py", "def home(x):\n    return \"DOMAIN {{marker}}::segment with punctuation {}\".format(x)", "COPY = \"DOMAIN {{marker}}::segment with punctuation {}\".format(x)"),
        ("home.go", "package p\nimport \"fmt\"\nfunc home(x string) string { return fmt.Sprintf(\"DOMAIN 100%%::segment with punctuation %s\", x) }", "package p\nimport \"fmt\"\nvar COPY = fmt.Sprintf(\"DOMAIN 100%%::segment with punctuation %s\", x)"),
    ] {
        let mapped = extract_homes(file, source);
        assert_eq!(occurrences(&file.replace("home", "caller"), copy, &mapped).len(), 1, "{file}");
    }
}
#[test]
fn anonymous_default_exports_are_template_homes() {
    for source in [
        format!("export default () => \"{SEG}\";"),
        format!("export default function () {{ return \"{SEG}\"; }}"),
    ] {
        let mapped = extract_homes("home.ts", &source);
        assert_eq!(mapped.len(), 1, "{source}");
        assert_eq!(mapped[0].name, "default");
        assert_eq!(
            occurrences("caller.ts", &format!("const COPY = \"{SEG}\";"), &mapped).len(),
            1
        );
    }
}
#[test]
fn python_module_class_and_function_docstrings_are_excluded() {
    let source = format!(
        "(\"{SEG}\")\nclass Thing:\n    \"{SEG}\"\ndef caller():\n    \"{SEG}\"\n    pass\n"
    );
    assert!(occurrences("caller.py", &source, &homes()).is_empty());
    assert_eq!(
        occurrences(
            "caller.py",
            &format!("{source}\nCOPY = \"{SEG}\""),
            &homes()
        )
        .len(),
        1
    );
    for prefix in ["b", "f"] {
        assert_eq!(
            occurrences("caller.py", &format!("{prefix}\"{SEG}\""), &homes()).len(),
            1
        );
    }
}
