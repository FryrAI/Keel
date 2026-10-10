//! Cases found in review, each a statement form PostgreSQL (or the runner
//! applying the file) reads differently from a naive scan.

use super::history::findings;

/// A definer function with no `search_path`: flagged unless something fixes it.
const G: &str = "CREATE FUNCTION g() RETURNS int SECURITY DEFINER RETURN 1;";
const SAFE: &str = "SET search_path = public, pg_temp";

/// `(name, line)` of every finding in a single source, sorted.
fn flagged(sql: &str) -> Vec<(String, u32)> {
    let mut hits: Vec<(String, u32)> = findings(&[sql])
        .remove(&0)
        .unwrap_or_default()
        .into_iter()
        .map(|finding| (finding.name, finding.line))
        .collect();
    hits.sort();
    hits
}

fn names(sql: &str) -> Vec<String> {
    flagged(sql).into_iter().map(|(name, _)| name).collect()
}

#[test]
fn commas_inside_an_argument_do_not_split_it() {
    let sql = format!(
        "{G}
         CREATE FUNCTION f(x numeric(10,2), y text DEFAULT concat('a', 'b')) RETURNS int
             SECURITY DEFINER RETURN 1;
         CREATE FUNCTION f(x int) RETURNS int SECURITY DEFINER {SAFE} RETURN 1;
         ALTER FUNCTION f(numeric, text) {SAFE};"
    );
    assert_eq!(names(&sql), ["g"]);
}

#[test]
fn down_sections_are_not_applied() {
    for marker in ["-- migrate:down", "-- +goose Down", "-- +migrate Down"] {
        let sql = format!("-- up\n{G}\n{marker}\nDROP FUNCTION g();\n");
        assert_eq!(names(&sql), ["g"], "{marker}");
    }
    // An ordinary comment naming a marker word ends nothing.
    let sql = format!("{G}\n-- going down the list\nALTER FUNCTION g() {SAFE};");
    assert!(names(&sql).is_empty());
}

#[test]
fn byte_order_mark_and_psql_meta_commands_hide_nothing() {
    assert_eq!(names(&format!("\u{feff}{G}")), ["g"]);
    assert_eq!(names(&format!("\\set ON_ERROR_STOP on\n{G}")), ["g"]);
    let fixed = format!("{G}\n\\connect app\nALTER FUNCTION g() {SAFE};");
    assert!(names(&fixed).is_empty());
}

#[test]
fn procedure_out_arguments_may_be_listed_by_type() {
    let sql = format!(
        "{G}
         CREATE PROCEDURE p(IN a int, OUT b int) LANGUAGE plpgsql SECURITY DEFINER
             AS $$ BEGIN b := a; END $$;
         CREATE PROCEDURE gone(IN a int, OUT b int) LANGUAGE plpgsql SECURITY DEFINER
             AS $$ BEGIN b := a; END $$;
         ALTER PROCEDURE p(int, int) {SAFE};
         DROP PROCEDURE gone(integer, integer);"
    );
    assert_eq!(names(&sql), ["g"]);
}

#[test]
fn built_in_aliases_pick_the_overload() {
    // Each ALTER resets exactly one of two safe overloads.
    for (written, named) in [
        ("float", "double precision"),
        ("dec", "numeric"),
        ("int[][]", "integer ARRAY"),
        ("timestamptz", "timestamp with time zone"),
    ] {
        let sql = format!(
            "CREATE FUNCTION f(x {written}) RETURNS int SECURITY DEFINER {SAFE} RETURN 1;\n\
             CREATE FUNCTION f(x text) RETURNS int SECURITY DEFINER {SAFE} RETURN 1;\n\
             ALTER FUNCTION f({named}) RESET search_path;"
        );
        assert_eq!(flagged(&sql), [("f".to_string(), 1)], "{written}");
    }
}

#[test]
fn unicode_prefixed_forms_are_one_schema_name() {
    let sql = format!(
        "{G}
         CREATE FUNCTION ident() RETURNS int SECURITY DEFINER
             SET search_path = public, U&\"pg_temp\" RETURN 1;
         CREATE FUNCTION string() RETURNS int SECURITY DEFINER
             SET search_path = public, U&'pg_temp' RETURN 1;"
    );
    assert_eq!(names(&sql), ["g"]);
}

#[test]
fn set_schema_with_a_string_sets_the_search_path() {
    let sql = format!(
        "{G}
         CREATE FUNCTION made_safe() RETURNS int SECURITY DEFINER SET SCHEMA 'pg_temp' RETURN 1;
         CREATE FUNCTION made_unsafe() RETURNS int SECURITY DEFINER {SAFE} RETURN 1;
         ALTER FUNCTION made_unsafe() SET SCHEMA 'public';
         CREATE FUNCTION public.moved() RETURNS int SECURITY DEFINER {SAFE} RETURN 1;
         ALTER FUNCTION public.moved() SET SCHEMA app;"
    );
    assert_eq!(names(&sql), ["g", "made_unsafe"]);
}

#[test]
fn an_unqualified_name_means_public_before_any_other_schema() {
    // A plain `two` is created beside `app.two`, not over it.
    let sql = format!(
        "CREATE FUNCTION app.two() RETURNS int SECURITY DEFINER RETURN 1;\n\
         CREATE FUNCTION two() RETURNS int SECURITY DEFINER {SAFE} RETURN 1;"
    );
    assert_eq!(flagged(&sql), [("two".to_string(), 1)]);
    // An unqualified ALTER picks the public routine when there is one …
    let sql = format!(
        "CREATE FUNCTION public.t() RETURNS int SECURITY DEFINER RETURN 1;\n\
         CREATE FUNCTION app.t() RETURNS int SECURITY DEFINER RETURN 1;\n\
         ALTER FUNCTION t() {SAFE};"
    );
    assert_eq!(flagged(&sql), [("t".to_string(), 2)]);
    // … and the only routine of that name otherwise: the session's path
    // decides, and it is not in the file.
    let sql = format!(
        "{G}\nCREATE FUNCTION app.only() RETURNS int SECURITY DEFINER RETURN 1;\n\
         ALTER FUNCTION only() {SAFE};"
    );
    assert_eq!(names(&sql), ["g"]);
}

#[test]
fn undetermined_is_neither_applied_nor_ignored() {
    let overloads = format!(
        "CREATE FUNCTION f(a int) RETURNS int SECURITY DEFINER {SAFE} RETURN 1;\n\
         CREATE FUNCTION f(a text) RETURNS int SECURITY DEFINER {SAFE} RETURN 1;\n\
         CREATE FUNCTION lone(a int) RETURNS int SECURITY DEFINER {SAFE} RETURN 1;\n"
    );
    // Types matching neither overload: applying the RESET to both would flag both.
    let unknown = format!("{overloads}ALTER FUNCTION f(item_id) RESET search_path;");
    assert!(names(&unknown).is_empty());
    // A lone candidate IS the target: its RESET is applied.
    let lone = format!("{overloads}ALTER FUNCTION lone(item_id) RESET search_path;");
    assert_eq!(names(&lone), ["lone"]);
    // A statement inside a DO block may not have run: its RESET is not applied.
    let maybe = format!(
        "{overloads}DO $$ BEGIN IF now() > '2000-01-01' THEN \
         ALTER FUNCTION lone(int) RESET search_path; END IF; END $$;"
    );
    assert!(names(&maybe).is_empty());
}

#[test]
fn to_default_stores_no_setting() {
    let found = findings(&[
        "CREATE FUNCTION f() RETURNS int SECURITY DEFINER SET search_path TO DEFAULT RETURN 1;",
    ]);
    assert_eq!(found[&0][0].list, None);
}

#[test]
fn text_of_a_sql_standard_body_is_not_a_clause() {
    assert_eq!(
        names(
            "CREATE FUNCTION atomic() RETURNS void LANGUAGE sql SECURITY DEFINER \
                 BEGIN ATOMIC UPDATE items SET search_path = pg_temp; END;
             CREATE FUNCTION returned() RETURNS int SECURITY DEFINER \
                 RETURN (SELECT 1 FROM items WHERE note = 'SET search_path = pg_temp');"
        ),
        ["atomic", "returned"]
    );
}

#[test]
fn many_statements_keep_their_lines() {
    let sql: String = (0..500).map(|n| format!("\n{G}\nALTER FUNCTION g() {SAFE};\nCREATE FUNCTION f{n}() RETURNS int SECURITY DEFINER RETURN 1;")).collect();
    let hits = flagged(&sql);
    assert_eq!(hits.len(), 500);
    assert!(
        hits.contains(&("f499".to_string(), 1501)),
        "{:?}",
        hits.last()
    );
}
