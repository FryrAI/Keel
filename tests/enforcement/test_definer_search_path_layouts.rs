// W013 across the file layouts migration runners use: which files and sections
// a forward run applies, in which order, and how the finding behaves around
// keel's own machinery (batch mode, node hashes).
use std::fs;

use super::{control, flagged, routine, CODE, TABLE};
use crate::common::{compile_json, keel, mapped_project, violations_with_code};

const SAFE: &str = "SECURITY DEFINER SET search_path = public, pg_temp";

fn names(result: &serde_json::Value) -> Vec<String> {
    let mut names: Vec<String> = violations_with_code(result, CODE)
        .iter()
        .map(|v| {
            v["message"]
                .as_str()
                .unwrap()
                .split('`')
                .nth(1)
                .unwrap()
                .to_string()
        })
        .collect();
    names.sort();
    names
}

#[test]
fn down_half_of_an_up_down_pair_is_not_replayed() {
    let up = format!(
        "{TABLE}{}{}",
        control(),
        routine("touch_item", "SECURITY DEFINER")
    );
    let down = "DROP FUNCTION touch_item();\nDROP FUNCTION control_fn();\n";
    assert_eq!(
        flagged(
            &[
                ("migrations/0001_items.up.sql", up.as_str()),
                ("migrations/0002_cleanup.down.sql", down),
            ],
            "migrations/0001_items.up.sql"
        ),
        ["control_fn", "touch_item"]
    );
}

#[test]
fn down_section_of_a_file_is_not_replayed() {
    let sql = format!(
        "-- migrate:up\n{TABLE}{}{}\n-- migrate:down\n\
         DROP FUNCTION touch_item();\nDROP FUNCTION control_fn();\nDROP TABLE items;\n",
        control(),
        routine("touch_item", SAFE),
    );
    assert_eq!(
        flagged(
            &[("migrations/0001_items.sql", &sql)],
            "migrations/0001_items.sql"
        ),
        ["control_fn"]
    );
}

#[test]
fn repeatable_file_is_applied_after_versioned_ones() {
    // `R__` sorts before `V1__` by name; the runner applies it last.
    let versioned = format!("{TABLE}{}{}", control(), routine("touch_item", SAFE));
    let repeatable = routine("touch_item", "SECURITY DEFINER")
        .replace("CREATE FUNCTION", "CREATE OR REPLACE FUNCTION");
    let files = [
        ("migrations/V1__items.sql", versioned.as_str()),
        ("migrations/R__routines.sql", repeatable.as_str()),
    ];
    assert_eq!(
        flagged(&files, "migrations/R__routines.sql"),
        ["touch_item"]
    );
    assert_eq!(flagged(&files, "migrations/V1__items.sql"), ["control_fn"]);
}

#[test]
fn siblings_in_another_case_or_encoding_still_count() {
    let first = format!(
        "{TABLE}{}{}{}",
        control(),
        routine("reset_by_upper", SAFE),
        routine("reset_by_latin1", SAFE),
    );
    let dir = mapped_project(&[("migrations/0001_items.sql", &first)]);
    let migrations = dir.path().join("migrations");
    fs::write(
        migrations.join("0002_WEAKEN.SQL"),
        "ALTER FUNCTION reset_by_upper() RESET search_path;\n",
    )
    .unwrap();
    // A Latin-1 comment makes the file invalid UTF-8; its statement is ASCII.
    fs::write(
        migrations.join("0003_legacy.sql"),
        b"-- caf\xe9\nALTER FUNCTION reset_by_latin1() RESET search_path;\n",
    )
    .unwrap();
    let result = compile_json(dir.path(), "migrations/0001_items.sql");
    assert_eq!(
        names(&result),
        ["control_fn", "reset_by_latin1", "reset_by_upper"]
    );
}

#[test]
fn batch_mode_defers_the_warning() {
    let sql = format!("{TABLE}{}", control());
    let dir = mapped_project(&[("migrations/0001_items.sql", &sql)]);
    keel(dir.path(), &["compile", "--batch-start"]);
    let during = compile_json(dir.path(), "migrations/0001_items.sql");
    assert!(names(&during).is_empty(), "{during}");
    let end = keel(dir.path(), &["compile", "--batch-end", "--json"]);
    let end: serde_json::Value = serde_json::from_slice(&end.stdout).unwrap();
    assert_eq!(names(&end), ["control_fn"]);
}

#[test]
fn each_overload_carries_its_own_hash() {
    let sql = "\
CREATE TABLE items (id int PRIMARY KEY, label text);

CREATE FUNCTION find_item(p_id int) RETURNS items
    LANGUAGE sql SECURITY DEFINER
    AS $$ SELECT * FROM items WHERE id = p_id $$;

CREATE FUNCTION find_item(p_label text) RETURNS items
    LANGUAGE sql SECURITY DEFINER
    AS $$ SELECT * FROM items WHERE label = p_label $$;
";
    let dir = mapped_project(&[("migrations/0001_items.sql", sql)]);
    let result = compile_json(dir.path(), "migrations/0001_items.sql");
    let hits = violations_with_code(&result, CODE);
    assert_eq!(hits.len(), 2, "{result}");
    let listing = keel(
        dir.path(),
        &["discover", "migrations/0001_items.sql", "--llm"],
    );
    let listing = String::from_utf8_lossy(&listing.stdout).to_string();
    for hit in &hits {
        let hash = hit["hash"].as_str().unwrap();
        assert!(
            listing.contains(&format!("hash={hash} ")),
            "{hash} not in:\n{listing}"
        );
    }
    assert_ne!(hits[0]["hash"], hits[1]["hash"]);
}
