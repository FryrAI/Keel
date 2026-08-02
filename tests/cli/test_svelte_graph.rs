// End-to-end acceptance for T1.3: a helper that a SvelteKit route imports and
// uses only from markup must reach the graph, so `keel search` and
// `keel discover` stop reporting it at zero callers.

use std::fs;
use std::process::Command;

use tempfile::TempDir;

use crate::common::keel_bin;

/// A minimal SvelteKit app: a model module, and a route that imports five of
/// its helpers on separate lines — one called from the script, three named only
/// in markup, one never used at all.
fn make_app() -> TempDir {
    let dir = TempDir::new().unwrap();
    let write = |rel: &str, body: &str| {
        let full = dir.path().join(rel);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, body).unwrap();
    };

    write(
        "tsconfig.json",
        "{\n  \"compilerOptions\": { \"paths\": { \"$lib\": [\"./src/lib\"], \"$lib/*\": [\"./src/lib/*\"] } }\n}\n",
    );
    write(
        "src/lib/portfolio/model.ts",
        "export function matchesQuery(q: string): boolean { return q.length > 0; }\n\
         export function completenessPct(v: number): number { return v; }\n\
         export function fristLabel(v: number): string { return String(v); }\n\
         export function offenTotal(v: number): number { return v; }\n\
         export function neverUsed(v: number): number { return v; }\n",
    );
    write(
        "src/routes/+page.svelte",
        "<script lang=\"ts\">\n\
           import {\n\
             matchesQuery,\n\
             completenessPct,\n\
             fristLabel,\n\
             offenTotal,\n\
             neverUsed\n\
           } from '$lib/portfolio/model';\n\
           let rows: number[] = [];\n\
           const hits = rows.filter((r) => matchesQuery(String(r)));\n\
         </script>\n\
         \n\
         {#each hits as v}\n\
           {@const pct = completenessPct(v)}\n\
           <span class=\"lbl\">{fristLabel(v)} — „offen\u{201c}: {pct}</span>\n\
         {/each}\n\
         \n\
         {#await load() then value}\n\
           <b>{offenTotal(value)}</b>\n\
         {/await}\n",
    );

    let keel = keel_bin();
    for args in [vec!["init"], vec!["map"]] {
        let out = Command::new(&keel)
            .args(&args)
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "keel {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    dir
}

/// `keel search <name> --json` result for the first exact hit.
fn search(dir: &TempDir, name: &str) -> serde_json::Value {
    let out = Command::new(keel_bin())
        .args(["search", name, "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("search --json");
    v["results"]
        .as_array()
        .and_then(|r| r.iter().find(|e| e["name"] == name))
        .cloned()
        .unwrap_or_else(|| panic!("no result for {name}: {v}"))
}

#[test]
fn markup_only_imported_helpers_report_callers() {
    let dir = make_app();

    // Called from the `<script>` — this one only ever worked because it is the
    // FIRST name of the import list; the other four used to be dropped.
    assert!(
        search(&dir, "matchesQuery")["callers"].as_u64().unwrap() >= 1,
        "script-level call must resolve"
    );

    for name in ["completenessPct", "fristLabel", "offenTotal"] {
        let hit = search(&dir, name);
        assert!(
            hit["callers"].as_u64().unwrap() >= 1,
            "{name} is used from markup and must report callers >= 1: {hit}"
        );
    }
}

#[test]
fn an_unused_import_stays_at_zero_callers() {
    let dir = make_app();
    let hit = search(&dir, "neverUsed");
    assert_eq!(
        hit["callers"].as_u64().unwrap(),
        0,
        "importing a name is not using it: {hit}"
    );
}

#[test]
fn the_markup_caller_is_listed_by_discover() {
    let dir = make_app();
    let hash = search(&dir, "completenessPct")["hash"]
        .as_str()
        .unwrap()
        .to_string();

    let out = Command::new(keel_bin())
        .args(["discover", &hash, "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("discover --json");
    let upstream = v["upstream"].as_array().expect("upstream array");
    assert!(
        upstream
            .iter()
            .any(|c| c["file"].as_str().unwrap_or("").ends_with("+page.svelte")),
        "the route naming the helper in markup must be listed as a caller: {v}"
    );
}
