// W013 cases the rule's proposal leaves open, each pinned to the reading
// PostgreSQL itself applies.
use super::{control, flagged, flagged_at, flagged_in_one_file, routine, TABLE};

const SAFE: &str = "SECURITY DEFINER SET search_path = pg_catalog, public, pg_temp";

#[test]
fn to_and_equals_set_the_same_list() {
    assert_eq!(
        flagged_in_one_file(&[
            routine(
                "to_safe",
                "SECURITY DEFINER SET search_path TO public, pg_temp"
            ),
            routine("to_unsafe", "SECURITY DEFINER SET search_path TO public"),
            routine(
                "eq_safe",
                "SECURITY DEFINER SET search_path = public, pg_temp"
            ),
            routine("eq_unsafe", "SECURITY DEFINER SET search_path = public"),
        ]),
        ["control_fn", "eq_unsafe", "to_unsafe"]
    );
}

#[test]
fn identifier_quoting_follows_postgresql() {
    assert_eq!(
        flagged_in_one_file(&[
            // Unquoted identifiers fold to lower case.
            routine(
                "folded",
                "SECURITY DEFINER SET search_path = Public, PG_TEMP"
            ),
            routine(
                "quoted",
                "SECURITY DEFINER SET search_path = public, \"pg_temp\""
            ),
            // A quoted identifier keeps its case: this names another schema.
            routine(
                "quoted_upper",
                "SECURITY DEFINER SET search_path = public, \"PG_TEMP\""
            ),
            // Each string constant is one schema name, taken verbatim.
            routine(
                "strings",
                "SECURITY DEFINER SET search_path = 'public', 'pg_temp'"
            ),
            routine(
                "one_string",
                "SECURITY DEFINER SET search_path = 'public, pg_temp'"
            ),
            // pg_temp_3 is a real schema name, not the alias.
            routine(
                "numbered",
                "SECURITY DEFINER SET search_path = public, pg_temp_3"
            ),
        ]),
        ["control_fn", "numbered", "one_string", "quoted_upper"]
    );
}

#[test]
fn empty_and_default_lists_are_flagged() {
    assert_eq!(
        flagged_in_one_file(&[
            // An empty path still searches pg_temp first.
            routine("empty", "SECURITY DEFINER SET search_path = ''"),
            // TO DEFAULT stores no setting at all.
            routine("to_default", "SECURITY DEFINER SET search_path TO DEFAULT"),
        ]),
        ["control_fn", "empty", "to_default"]
    );
}

#[test]
fn from_current_is_left_unflagged() {
    // The stored list is whatever the creating session had: not in the file.
    assert_eq!(
        flagged_in_one_file(&[routine(
            "from_current",
            "SECURITY DEFINER SET search_path FROM CURRENT"
        )]),
        ["control_fn"]
    );
}

#[test]
fn alter_function_with_argument_types_targets_one_overload() {
    let sql = "\
CREATE TABLE items (id int PRIMARY KEY, label text, seen timestamptz);

CREATE FUNCTION find_item(p_id int) RETURNS items
    LANGUAGE sql SECURITY DEFINER SET search_path = public
    AS $$ SELECT * FROM items WHERE id = p_id $$;

CREATE FUNCTION find_item(p_label text) RETURNS items
    LANGUAGE sql SECURITY DEFINER SET search_path = public
    AS $$ SELECT * FROM items WHERE label = p_label $$;

CREATE FUNCTION find_item(p_id int, p_label text) RETURNS items
    LANGUAGE sql SECURITY DEFINER SET search_path = public
    AS $$ SELECT * FROM items WHERE id = p_id AND label = p_label $$;

ALTER FUNCTION find_item(text) SET search_path = public, pg_temp;
";
    assert_eq!(
        flagged_at(
            &[("migrations/0001_items.sql", sql)],
            "migrations/0001_items.sql"
        ),
        [("find_item".to_string(), 3), ("find_item".to_string(), 11)]
    );
}

#[test]
fn create_or_replace_supersedes_the_earlier_definition() {
    let unsafe_list = "SECURITY DEFINER SET search_path = public";
    let first = format!(
        "{TABLE}{}{}{}",
        control(),
        routine("made_safe", unsafe_list),
        routine("made_unsafe", SAFE),
    );
    let second = format!(
        "{}{}",
        routine("made_safe", SAFE).replace("CREATE FUNCTION", "CREATE OR REPLACE FUNCTION"),
        routine("made_unsafe", unsafe_list)
            .replace("CREATE FUNCTION", "CREATE OR REPLACE FUNCTION"),
    );
    let files = [
        ("migrations/0001_items.sql", first.as_str()),
        ("migrations/0002_replace.sql", second.as_str()),
    ];
    // The replaced definitions no longer exist; the live ones are judged alone.
    assert_eq!(flagged(&files, "migrations/0001_items.sql"), ["control_fn"]);
    assert_eq!(
        flagged(&files, "migrations/0002_replace.sql"),
        ["made_unsafe"]
    );
}

#[test]
fn replacement_discards_an_earlier_alter() {
    // CREATE OR REPLACE assigns every property anew: the ALTER's list is gone.
    let first = format!("{TABLE}{}", routine("touch_item", "SECURITY DEFINER"));
    let harden = "ALTER FUNCTION touch_item() SET search_path = public, pg_temp;\n";
    let third = routine("touch_item", "SECURITY DEFINER")
        .replace("CREATE FUNCTION", "CREATE OR REPLACE FUNCTION");
    let files = [
        ("migrations/0001_items.sql", first.as_str()),
        ("migrations/0002_harden.sql", harden),
        ("migrations/0003_replace.sql", third.as_str()),
    ];
    assert_eq!(
        flagged(&files, "migrations/0003_replace.sql"),
        ["touch_item"]
    );
    assert!(flagged(&files, "migrations/0001_items.sql").is_empty());
}

#[test]
fn dropped_function_is_not_flagged() {
    let first = format!(
        "{TABLE}{}{}{}",
        control(),
        routine("dropped", "SECURITY DEFINER"),
        routine("dropped_too", "SECURITY DEFINER"),
    );
    let drop = "DROP FUNCTION IF EXISTS dropped(), public.dropped_too CASCADE;\n";
    assert_eq!(
        flagged(
            &[
                ("migrations/0001_items.sql", first.as_str()),
                ("migrations/0002_drop.sql", drop),
            ],
            "migrations/0001_items.sql"
        ),
        ["control_fn"]
    );
}

#[test]
fn alter_function_security_changes_what_is_judged() {
    let first = format!(
        "{TABLE}{}{}{}",
        control(),
        routine("made_invoker", "SECURITY DEFINER"),
        routine("made_definer", "STABLE"),
    );
    let alter = "\
ALTER FUNCTION made_invoker() SECURITY INVOKER;
ALTER FUNCTION made_definer() SECURITY DEFINER;
";
    assert_eq!(
        flagged(
            &[
                ("migrations/0001_items.sql", first.as_str()),
                ("migrations/0002_security.sql", alter),
            ],
            "migrations/0001_items.sql"
        ),
        ["control_fn", "made_definer"]
    );
}

#[test]
fn renamed_and_moved_functions_keep_their_history() {
    let first = format!(
        "{TABLE}{}{}{}",
        control(),
        routine("old_name", "SECURITY DEFINER"),
        routine("public.moved", "SECURITY DEFINER"),
    );
    let later = "\
ALTER FUNCTION old_name() RENAME TO new_name;
ALTER FUNCTION new_name() SET search_path = public, pg_temp;
ALTER FUNCTION public.moved() SET SCHEMA app;
ALTER FUNCTION app.moved() SET search_path = app, pg_temp;
";
    assert_eq!(
        flagged(
            &[
                ("migrations/0001_items.sql", first.as_str()),
                ("migrations/0002_rename.sql", later),
            ],
            "migrations/0001_items.sql"
        ),
        ["control_fn"]
    );
}

#[test]
fn text_in_bodies_and_comments_is_not_a_clause() {
    let sql = format!(
        "{TABLE}{}\
-- SECURITY DEFINER SET search_path = public is what this one does NOT do
CREATE FUNCTION describe_item() RETURNS text
    LANGUAGE sql /* SECURITY DEFINER */
    AS $$ SELECT 'SECURITY DEFINER; SET search_path = public' $$;

CREATE FUNCTION body_only() RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER -- SET search_path = public, pg_temp
    AS $body$
BEGIN
    SET search_path = public, pg_temp;
    UPDATE items SET seen = now();
END;
$body$;
",
        control()
    );
    assert_eq!(
        flagged(
            &[("migrations/0001_items.sql", &sql)],
            "migrations/0001_items.sql"
        ),
        ["body_only", "control_fn"]
    );
}

#[test]
fn procedures_follow_the_same_rule() {
    let sql = format!(
        "{TABLE}{}\
CREATE PROCEDURE purge_items() LANGUAGE sql SECURITY DEFINER
    AS $$ DELETE FROM items $$;

CREATE PROCEDURE trim_items() LANGUAGE sql SECURITY DEFINER
    SET search_path = public, pg_temp
    AS $$ DELETE FROM items WHERE seen IS NULL $$;
",
        control()
    );
    assert_eq!(
        flagged(
            &[("migrations/0001_items.sql", &sql)],
            "migrations/0001_items.sql"
        ),
        ["control_fn", "purge_items"]
    );
}

#[test]
fn statements_inside_a_do_block_leave_their_target_unflagged() {
    // Whether a conditional statement ran is not in the file.
    let first = format!(
        "{TABLE}{}{}",
        control(),
        routine("maybe_fixed", "SECURITY DEFINER")
    );
    let later = "\
DO $$
BEGIN
    IF current_setting('server_version_num')::int >= 140000 THEN
        ALTER FUNCTION maybe_fixed() SET search_path = public, pg_temp;
    END IF;
END
$$;
";
    assert_eq!(
        flagged(
            &[
                ("migrations/0001_items.sql", first.as_str()),
                ("migrations/0002_maybe.sql", later),
            ],
            "migrations/0001_items.sql"
        ),
        ["control_fn"]
    );
}

#[test]
fn clean_migration_compiles_silently() {
    let sql = format!("{TABLE}{}", routine("touch_item", SAFE));
    let dir = crate::common::mapped_project(&[("migrations/0001_items.sql", &sql)]);
    let out = crate::common::keel(dir.path(), &["compile", "migrations/0001_items.sql"]);
    assert!(out.status.success());
    assert!(
        out.stdout.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    // Positive control: the same file without the clause is not silent.
    let bare = format!("{TABLE}{}", routine("touch_item", "SECURITY DEFINER"));
    let dir = crate::common::mapped_project(&[("migrations/0001_items.sql", &bare)]);
    let out = crate::common::keel(dir.path(), &["compile", "migrations/0001_items.sql"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("W013"));
}
