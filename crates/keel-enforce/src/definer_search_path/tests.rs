use std::cmp::Ordering;

use super::history::findings;
use super::lex::{lex, Kind};
use super::statements::{statements, Action, Event, Setting};
use super::types::argument_type;
use super::{applied_order, is_applied, natural_cmp};

fn words(sql: &str) -> Vec<(Kind, String)> {
    lex(sql)
        .into_iter()
        .map(|token| (token.kind, token.text))
        .collect()
}

/// The names flagged in a single source.
fn flagged(sql: &str) -> Vec<String> {
    let mut names: Vec<String> = findings(&[sql])
        .remove(&0)
        .unwrap_or_default()
        .into_iter()
        .map(|finding| finding.name)
        .collect();
    names.sort();
    names
}

/// The canonical type of an input argument; `None` for an `OUT` one.
fn argument(sql: &str) -> Option<String> {
    let (name, input) = argument_type(&lex(sql));
    input.then_some(name)
}

#[test]
fn lexer_keeps_constants_whole_and_drops_comments() {
    assert_eq!(
        words("Set /* a /* nested */ note */ \"Mixed\" -- tail\n = 'it''s' E'a\\'b' $q$x;y$q$ $1"),
        [
            (Kind::Word, "set".to_string()),
            (Kind::Quoted, "Mixed".to_string()),
            (Kind::Punct, "=".to_string()),
            (Kind::Str, "it's".to_string()),
            (Kind::Str, "a'b".to_string()),
            (Kind::Str, "x;y".to_string()),
            (Kind::Punct, "$".to_string()),
            (Kind::Word, "1".to_string()),
        ]
    );
}

#[test]
fn lexer_survives_unterminated_input() {
    assert_eq!(words("'open"), [(Kind::Str, "open".to_string())]);
    assert_eq!(words("$$open"), [(Kind::Str, "open".to_string())]);
    assert_eq!(words("/* open"), []);
    assert_eq!(words("größe$x"), [(Kind::Word, "größe$x".to_string())]);
    // Only a glued `E` is a prefix: here `e` is a name and `\` is literal.
    assert_eq!(
        words("e 'a\\'"),
        [
            (Kind::Word, "e".to_string()),
            (Kind::Str, "a\\".to_string())
        ]
    );
}

#[test]
fn argument_types_drop_names_modes_defaults_and_aliases() {
    assert_eq!(argument("p_id int").as_deref(), Some("integer"));
    assert_eq!(argument("integer").as_deref(), Some("integer"));
    assert_eq!(
        argument("IN p_tags text[] DEFAULT '{}'").as_deref(),
        Some("text[]")
    );
    assert_eq!(
        argument("p_at timestamptz = now()").as_deref(),
        Some("timestamp with time zone")
    );
    assert_eq!(
        argument("timestamp(3) with time zone").as_deref(),
        Some("timestamp with time zone")
    );
    assert_eq!(
        argument("p_x double precision").as_deref(),
        Some("double precision")
    );
    assert_eq!(argument("p_n numeric(10, 2)").as_deref(), Some("numeric"));
    assert_eq!(
        argument("VARIADIC p_ids public.item_id[]").as_deref(),
        Some("item_id[]")
    );
    assert_eq!(argument("INOUT p_total bigint").as_deref(), Some("bigint"));
    assert_eq!(argument("OUT p_total bigint"), None);
    assert_eq!(argument_type(&lex("OUT p_total bigint")).0, "bigint");
    assert_eq!(argument("p_x float").as_deref(), Some("double precision"));
    assert_eq!(argument("p_n dec(10, 2)").as_deref(), Some("numeric"));
    assert_eq!(argument("p_ids int[3][3]").as_deref(), Some("integer[]"));
    assert_eq!(
        argument("p_ids integer ARRAY[3]").as_deref(),
        Some("integer[]")
    );
}

#[test]
fn statements_read_each_alter_action() {
    let found = statements(
        "ALTER FUNCTION app.f(int, OUT text) OWNER TO app_owner SET SCHEMA other \
         SECURITY INVOKER RENAME TO g RESET ALL SET work_mem = '1MB';",
    );
    let [statement] = found.as_slice() else {
        panic!("{found:?}");
    };
    let Event::Alter { routine, actions } = &statement.event else {
        panic!("{statement:?}");
    };
    assert_eq!(routine.schema.as_deref(), Some("app"));
    let args = routine.args.as_ref().unwrap();
    assert_eq!(args.inputs, ["integer"]);
    assert_eq!(args.all, ["integer", "text"]);
    assert_eq!(
        actions,
        &[
            Action::SetSchema("other".to_string()),
            Action::Definer(false),
            Action::Rename("g".to_string()),
            Action::Path(Setting::Unset),
        ]
    );
}

#[test]
fn clauses_after_the_body_and_sql_standard_bodies_are_read() {
    assert_eq!(
        flagged(
            "CREATE FUNCTION body_first() RETURNS void AS $$ SELECT 1 $$ \
                 LANGUAGE sql SECURITY DEFINER SET search_path = public, pg_temp;
             CREATE FUNCTION atomic() RETURNS TABLE (set int, security int) \
                 LANGUAGE sql SECURITY DEFINER \
                 BEGIN ATOMIC UPDATE items SET search_path = 1; SELECT 1, 2; END;
             CREATE FUNCTION external() RETURNS int EXTERNAL SECURITY DEFINER \
                 RETURN 1;"
        ),
        ["atomic", "external"]
    );
}

#[test]
fn alter_with_types_matching_no_overload_leaves_them_undetermined() {
    let overloads = "\
        CREATE FUNCTION f(a int) RETURNS int SECURITY DEFINER RETURN 1;
        CREATE FUNCTION f(a text) RETURNS int SECURITY DEFINER RETURN 1;
        CREATE FUNCTION g(a int) RETURNS int SECURITY DEFINER RETURN 1;";
    assert_eq!(flagged(overloads), ["f", "f", "g"]);
    // `item_id` may be a domain over either type: no overload is ruled out.
    let unknown = format!("{overloads} ALTER FUNCTION f(item_id) SET search_path = pg_temp;");
    assert_eq!(flagged(&unknown), ["g"]);
    // A lone candidate of that arity is the target however the type is spelled.
    let lone = format!("{overloads} ALTER FUNCTION g(item_id) SET search_path = pg_temp;");
    assert_eq!(flagged(&lone), ["f", "f"]);
}

#[test]
fn qualified_and_unqualified_names_meet() {
    assert!(flagged(
        "CREATE FUNCTION f() RETURNS int SECURITY DEFINER RETURN 1;
         ALTER FUNCTION public.f() SET search_path = public, pg_temp;"
    )
    .is_empty());
    assert_eq!(
        flagged(
            "CREATE FUNCTION app.f() RETURNS int SECURITY DEFINER RETURN 1;
             ALTER FUNCTION public.f() SET search_path = public, pg_temp;"
        ),
        ["f"]
    );
}

#[test]
fn findings_carry_the_create_line_and_the_changing_statement() {
    let first = "\n\nCREATE FUNCTION f() RETURNS int SECURITY DEFINER\n  SET search_path = pg_temp RETURN 1;";
    let second = "-- later\nALTER FUNCTION f() RESET search_path;";
    let found = findings(&[first, second]);
    let [finding] = found[&0].as_slice() else {
        panic!("{found:?}");
    };
    assert_eq!((finding.line, &finding.list), (3, &None));
    let site = finding.changed_at.unwrap();
    assert_eq!((site.source, site.line), (1, 2));
    assert!(!found.contains_key(&1));
}

#[test]
fn file_names_order_as_a_migration_runner_applies_them() {
    let mut names = [
        "10_c.sql",
        "9_b.sql",
        "V2__d.sql",
        "V10__e.sql",
        "0002_a.sql",
        "001_a.sql",
    ];
    names.sort_by(|a, b| natural_cmp(a, b));
    assert_eq!(
        names,
        [
            "001_a.sql",
            "0002_a.sql",
            "9_b.sql",
            "10_c.sql",
            "V2__d.sql",
            "V10__e.sql"
        ]
    );
    assert_eq!(natural_cmp("01_a.sql", "1_a.sql"), Ordering::Less);
    // A version ends at `__`: 1 precedes 1.1 and 1_1.
    assert_eq!(natural_cmp("V1__a.sql", "V1.1__b.sql"), Ordering::Less);
    assert_eq!(natural_cmp("V2_1__b.sql", "V2__a.sql"), Ordering::Greater);
    assert_eq!(natural_cmp("V2_1__b.sql", "V2_2__a.sql"), Ordering::Less);
    assert_eq!(natural_cmp("1_a.sql", "1_a.sql"), Ordering::Equal);
    assert_eq!(
        natural_cmp("20240102_a.sql", "20240101_b.sql"),
        Ordering::Greater
    );
}

#[test]
fn flyway_repeatable_files_apply_after_versioned_ones() {
    let mut names = [
        "R__routines.sql",
        "V1__items.sql",
        "V2__more.sql",
        "README.sql",
    ];
    names.sort_by(|a, b| applied_order(a, b));
    assert_eq!(
        names,
        [
            "README.sql",
            "V1__items.sql",
            "V2__more.sql",
            "R__routines.sql"
        ]
    );
}

#[test]
fn only_forward_sql_files_are_replayed() {
    assert!(is_applied("0001_items.sql"));
    assert!(is_applied("0001_items.up.sql"));
    assert!(is_applied("0002_MORE.SQL"));
    assert!(!is_applied("0001_items.down.sql"));
    assert!(!is_applied("notes.md"));
}
