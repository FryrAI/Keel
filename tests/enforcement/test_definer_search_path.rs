// W013 (definer_search_path): a SECURITY DEFINER SQL routine whose effective
// `search_path` does not end with `pg_temp`. One positive and one negative case
// per clause; every negative fixture also carries `control_fn`, which must be
// flagged, so a build that never emits the code cannot pass it.
use crate::common::{compile_json, keel, mapped_project, violations_with_code};

#[path = "test_definer_search_path_readings.rs"]
mod readings;

pub(super) const CODE: &str = "W013";
pub(super) const TABLE: &str = "CREATE TABLE items (id int PRIMARY KEY, seen timestamptz);\n\n";

/// `CREATE FUNCTION <name>() …` with `clauses` between the signature and the body.
pub(super) fn routine(name: &str, clauses: &str) -> String {
    format!(
        "CREATE FUNCTION {name}() RETURNS void\n    LANGUAGE sql {clauses}\n    \
         AS $$ UPDATE items SET seen = now() $$;\n\n"
    )
}

/// A definer function with no `search_path`: flagged in every fixture.
pub(super) fn control() -> String {
    routine("control_fn", "SECURITY DEFINER")
}

/// The W013 findings of `keel compile <file>` as sorted `(function, line)` pairs.
pub(super) fn flagged_at(files: &[(&str, &str)], file: &str) -> Vec<(String, u64)> {
    let dir = mapped_project(files);
    let result = compile_json(dir.path(), file);
    let mut hits: Vec<(String, u64)> = violations_with_code(&result, CODE)
        .iter()
        .map(|v| {
            let message = v["message"].as_str().unwrap();
            let name = message
                .split('`')
                .nth(1)
                .unwrap_or_else(|| panic!("no function name in `{message}`"));
            (name.to_string(), v["line"].as_u64().unwrap())
        })
        .collect();
    hits.sort();
    hits
}

/// The names of the functions `keel compile <file>` flags, sorted.
pub(super) fn flagged(files: &[(&str, &str)], file: &str) -> Vec<String> {
    flagged_at(files, file)
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

/// One migration holding `TABLE`, the control and `routines`.
pub(super) fn flagged_in_one_file(routines: &[String]) -> Vec<String> {
    let sql = format!("{TABLE}{}{}", control(), routines.concat());
    flagged(
        &[("migrations/0001_items.sql", &sql)],
        "migrations/0001_items.sql",
    )
}

const ITEMS: &str = "\
-- items and the routines that maintain them
CREATE TABLE items (id int PRIMARY KEY, seen timestamptz, tags text[]);

-- marks an item as seen
CREATE FUNCTION touch_item(p_id int, p_tags text[] DEFAULT '{}') RETURNS void
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'public'
    AS $$ UPDATE items SET seen = now(), tags = p_tags WHERE id = p_id $$;

CREATE FUNCTION count_items() RETURNS bigint
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'public'
    AS $$ SELECT count(*) FROM items $$;
";

#[test]
fn definer_without_search_path_is_flagged() {
    let sql = "\
-- items and their touch routine
CREATE TABLE items (id int PRIMARY KEY, seen timestamptz);

-- marks an item as seen
CREATE FUNCTION touch_item(p_id int) RETURNS void
    LANGUAGE sql SECURITY DEFINER
    AS $$ UPDATE items SET seen = now() WHERE id = p_id $$;
";
    let dir = mapped_project(&[("migrations/0001_items.sql", sql)]);
    let result = compile_json(dir.path(), "migrations/0001_items.sql");
    let hits = violations_with_code(&result, CODE);
    assert_eq!(hits.len(), 1, "{result}");
    let hit = &hits[0];
    assert_eq!(hit["severity"], "WARNING");
    assert_eq!(hit["category"], "definer_search_path");
    assert_eq!(hit["file"], "migrations/0001_items.sql");
    assert_eq!(hit["line"], 5, "the finding sits on the CREATE line");
    assert!(hit["message"].as_str().unwrap().contains("`touch_item`"));
    assert!(hit["fix_hint"]
        .as_str()
        .unwrap()
        .starts_with("append pg_temp as the last search_path entry"));
    assert!(hit["confidence"].as_f64().unwrap() > 0.0);
    assert!(!hit["resolution_tier"].as_str().unwrap().is_empty());

    // The finding carries the stored node's hash, so `keel discover <hash>` works.
    let listing = keel(
        dir.path(),
        &["discover", "migrations/0001_items.sql", "--llm"],
    );
    let listing = String::from_utf8_lossy(&listing.stdout).to_string();
    let hash = hit["hash"].as_str().unwrap();
    assert!(!hash.is_empty());
    assert!(
        listing.contains(&format!("function touch_item hash={hash}")),
        "hash {hash} is not touch_item's in:\n{listing}"
    );
}

#[test]
fn definer_with_pg_temp_last_is_not_flagged() {
    let safe = "SECURITY DEFINER SET search_path = pg_catalog, public, pg_temp";
    assert_eq!(
        flagged_in_one_file(&[
            routine("touch_item", safe),
            routine("only_temp", "SECURITY DEFINER SET search_path = pg_temp"),
        ]),
        ["control_fn"]
    );
}

#[test]
fn search_path_not_ending_in_pg_temp_is_flagged() {
    assert_eq!(
        flagged_in_one_file(&[
            routine("no_temp", "SECURITY DEFINER SET search_path = public"),
            routine(
                "temp_first",
                "SECURITY DEFINER SET search_path = pg_temp, public"
            ),
            routine(
                "temp_last",
                "SECURITY DEFINER SET search_path = public, pg_temp"
            ),
        ]),
        ["control_fn", "no_temp", "temp_first"]
    );
}

#[test]
fn first_occurrence_of_a_duplicate_schema_decides() {
    assert_eq!(
        flagged_in_one_file(&[
            // pg_temp is searched first: its later repeat changes nothing.
            routine(
                "temp_repeated",
                "SECURITY DEFINER SET search_path = pg_temp, public, pg_temp"
            ),
            // public's repeat is dropped, so the list still ends with pg_temp.
            routine(
                "public_repeated",
                "SECURITY DEFINER SET search_path = public, pg_temp, public"
            ),
        ]),
        ["control_fn", "temp_repeated"]
    );
}

#[test]
fn alter_function_in_a_later_file_is_the_effective_setting() {
    let harden = "\
ALTER FUNCTION public.touch_item(integer, text[]) OWNER TO app_owner;
ALTER FUNCTION public.touch_item(integer, text[])
    SET search_path = pg_catalog, public, pg_temp;
";
    let files = [
        ("migrations/0001_items.sql", ITEMS),
        ("migrations/0002_harden.sql", harden),
    ];
    assert_eq!(
        flagged(&files, "migrations/0001_items.sql"),
        ["count_items"]
    );
    // Without the later file both functions keep `public` alone.
    assert_eq!(
        flagged(&files[..1], "migrations/0001_items.sql"),
        ["count_items", "touch_item"]
    );
}

#[test]
fn alter_function_in_a_later_file_can_unset_a_safe_list() {
    let safe = "SECURITY DEFINER SET search_path = pg_catalog, public, pg_temp";
    let first = format!(
        "{TABLE}{}{}",
        routine("touch_item", safe),
        routine("kept", safe)
    );
    let loosen = "-- widen the path\n\nALTER FUNCTION touch_item() SET search_path = public;\n";
    let files = [
        ("migrations/0001_items.sql", first.as_str()),
        ("migrations/0002_loosen.sql", loosen),
    ];
    let dir = mapped_project(&files);
    let result = compile_json(dir.path(), "migrations/0001_items.sql");
    let hits = violations_with_code(&result, CODE);
    assert_eq!(hits.len(), 1, "{result}");
    let message = hits[0]["message"].as_str().unwrap();
    assert!(message.contains("`touch_item`"), "{message}");
    // The message names the statement that set the effective list.
    assert!(
        message.contains("migrations/0002_loosen.sql:3"),
        "{message}"
    );
    assert_eq!(
        hits[0]["line"], 3,
        "reported on the CREATE, not on the ALTER"
    );
}

#[test]
fn alter_function_later_in_the_same_file_counts() {
    assert_eq!(
        flagged_in_one_file(&[
            routine("touch_item", "SECURITY DEFINER SET search_path = public"),
            "ALTER FUNCTION touch_item() SET search_path = public, pg_temp;\n".to_string(),
        ]),
        ["control_fn"]
    );
}

#[test]
fn migration_files_apply_in_numeric_order() {
    // `10_` sorts before `9_` byte-wise; a migration runner applies it after.
    let create = format!(
        "{TABLE}{}{}",
        control(),
        routine("touch_item", "SECURITY DEFINER SET search_path = public")
    );
    let harden = "ALTER FUNCTION touch_item() SET search_path = public, pg_temp;\n";
    assert_eq!(
        flagged(
            &[
                ("migrations/9_items.sql", create.as_str()),
                ("migrations/10_harden.sql", harden),
            ],
            "migrations/9_items.sql"
        ),
        ["control_fn"]
    );
}

#[test]
fn reset_search_path_removes_the_setting() {
    let safe = "SECURITY DEFINER SET search_path = pg_catalog, public, pg_temp";
    let first = format!(
        "{TABLE}{}{}{}{}",
        control(),
        routine("reset_one", safe),
        routine("reset_all", safe),
        routine("reset_other", &format!("{safe} SET work_mem = '64MB'")),
    );
    let reset = "\
ALTER FUNCTION reset_one() RESET search_path;
ALTER FUNCTION reset_all() RESET ALL;
ALTER FUNCTION reset_other() RESET work_mem;
";
    assert_eq!(
        flagged(
            &[
                ("migrations/0001_items.sql", first.as_str()),
                ("migrations/0002_reset.sql", reset),
            ],
            "migrations/0001_items.sql"
        ),
        ["control_fn", "reset_all", "reset_one"]
    );
}

#[test]
fn security_invoker_is_not_flagged() {
    assert_eq!(
        flagged_in_one_file(&[
            routine("explicit_invoker", "SECURITY INVOKER"),
            routine("default_invoker", "STABLE"),
            routine(
                "invoker_with_path",
                "SECURITY INVOKER SET search_path = public"
            ),
        ]),
        ["control_fn"]
    );
}
