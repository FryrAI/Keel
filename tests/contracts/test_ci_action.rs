// The shipped CI recipe is a contract too (T2.3).
//
// These files are executed on other people's runners, where a typo is a broken
// build for every consumer and there is no compiler in front of it. So: every
// embedded script parses under `bash -n`, nothing reaches for an interpreter
// keel does not ship, and the two rules that make the recipe honest — no
// prefix restore-key on the graph cache, no `gh pr view/edit` for the sticky
// comment — are asserted rather than remembered.

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

const ACTIONS: &[&str] = &[
    ".github/actions/keel/action.yml",
    ".github/actions/keel-graph/action.yml",
];

const SCRIPTS: &[&str] = &[
    ".github/actions/keel/entrypoint.sh",
    ".github/actions/keel/sticky_comment.sh",
];

/// Every `run: |` block in a composite action, dedented back to column zero.
fn embedded_scripts(yaml: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut lines = yaml.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        if trimmed != "run: |" && trimmed != "run: |-" {
            continue;
        }
        let indent = line.len() - trimmed.len();
        let mut body = String::new();
        while let Some(next) = lines.peek() {
            let next_indent = next.len() - next.trim_start().len();
            if !next.trim().is_empty() && next_indent <= indent {
                break;
            }
            let owned = lines.next().unwrap();
            body.push_str(owned.get(indent + 2..).unwrap_or(""));
            body.push('\n');
        }
        blocks.push(body);
    }
    blocks
}

/// The file with every whole-line `#` comment dropped.
///
/// These files explain their own prohibitions ("no restore-keys, because…"),
/// so a naive substring check on the raw text fails on the documentation
/// rather than on the behavior. The assertions below are about what the file
/// DOES.
fn code_only(content: &str) -> String {
    content
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `bash -n` a script, returning its stderr on a syntax error.
fn bash_syntax_error(script: &str, label: &str) -> Option<String> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("script.sh");
    std::fs::write(&path, script).unwrap();
    let out = Command::new("bash")
        .arg("-n")
        .arg(&path)
        .output()
        .expect("bash must be available to check the shipped CI scripts");
    if out.status.success() {
        return None;
    }
    Some(format!(
        "{label}: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    ))
}

#[test]
fn every_embedded_action_script_is_valid_bash() {
    for action in ACTIONS {
        let yaml = read(action);
        let blocks = embedded_scripts(&yaml);
        assert!(
            !blocks.is_empty(),
            "{action}: no run blocks found — the extractor drifted from the file layout"
        );
        for (i, block) in blocks.iter().enumerate() {
            if let Some(err) = bash_syntax_error(block, &format!("{action} block {i}")) {
                panic!("{err}\n---\n{block}");
            }
        }
    }
}

#[test]
fn every_shipped_shell_script_is_valid_bash() {
    for script in SCRIPTS {
        let path = repo_root().join(script);
        assert!(path.exists(), "{script} is missing");
        if let Some(err) = bash_syntax_error(&read(script), script) {
            panic!("{err}");
        }
    }
}

/// Article 1: single binary, zero runtime dependencies. The action used to
/// reshape keel's JSON with an inline `python3` heredoc; `--format github` in
/// the binary replaced it and nothing may bring it back.
#[test]
fn the_action_needs_no_interpreter() {
    for file in ACTIONS.iter().chain(SCRIPTS) {
        let content = code_only(&read(file));
        for forbidden in ["python3", "python ", "node -e", "ruby -e"] {
            assert!(
                !content.contains(forbidden),
                "{file} reaches for `{forbidden}` — keel ships one binary and no interpreters"
            );
        }
    }
}

/// A prefix restore hands CI a graph built from different source, and
/// `compile --changed` against it manufactures phantom E001/E004. A miss costs
/// one `keel map`; a wrong graph costs trust in every annotation.
#[test]
fn the_graph_cache_has_no_prefix_fallback() {
    let yaml = code_only(&read(".github/actions/keel-graph/action.yml"));
    assert!(
        !yaml.contains("restore-keys"),
        "the graph cache must never fall back to a prefix match"
    );
    assert!(
        yaml.contains("git merge-base"),
        "the cache key must be the merge-base commit, computed with git merge-base"
    );
    assert!(
        yaml.contains("actions/cache/restore@v4") && yaml.contains("actions/cache/save@v4"),
        "restore and save are separate so only a push run (which maps at the keyed commit) writes the cache"
    );
}

/// `gh pr view`/`gh pr edit` go through GraphQL and 400 on the Projects-classic
/// deprecation on FryrAI repos. The sticky comment is REST or nothing. And
/// `pull_request_target` — the usual "fix" for a fork's read-only token — hands
/// fork code a write token, so it stays out of this repo entirely.
#[test]
fn the_sticky_comment_is_rest_only_and_never_uses_pull_request_target() {
    let script = code_only(&read(".github/actions/keel/sticky_comment.sh"));
    assert!(
        script.contains("issues/${PR_NUMBER}/comments") && script.contains("issues/comments/"),
        "the sticky comment must find, create and update through the issues REST endpoints"
    );
    for forbidden in ["gh pr view", "gh pr edit", "gh pr comment"] {
        assert!(
            !script.contains(forbidden),
            "`{forbidden}` 400s on Projects-classic repos — use `gh api` REST"
        );
    }
    for file in ACTIONS.iter().chain(SCRIPTS) {
        assert!(
            !code_only(&read(file)).contains("pull_request_target"),
            "{file} must never use pull_request_target"
        );
    }
}

/// A fork's token cannot comment. That degrades to the step summary; it never
/// fails the job, and the script says so out loud.
#[test]
fn a_fork_degrades_to_the_step_summary() {
    let script = code_only(&read(".github/actions/keel/sticky_comment.sh"));
    assert!(script.contains("IS_FORK"), "forks must be detected");
    assert!(
        script.contains("GITHUB_STEP_SUMMARY"),
        "the fallback surface must be the step summary"
    );
    assert!(
        !script.contains("set -e") || script.contains("set -uo pipefail"),
        "the comment script must not abort the job on a failed API call"
    );
}

/// The composite action cannot reference its sibling with a relative path —
/// `./` resolves against the CALLING repository's workspace — so the full repo
/// reference is load-bearing, not stylistic.
#[test]
fn the_keel_action_provisions_the_graph_by_full_repo_reference() {
    let yaml = read(".github/actions/keel/action.yml");
    assert!(
        yaml.contains("uses: FryrAI/Keel/.github/actions/keel-graph@v0"),
        "the keel action must provision the graph through the keel-graph action"
    );
    assert!(
        !yaml.contains("uses: ./.github/actions/keel-graph"),
        "a relative uses: would resolve against the consumer's checkout"
    );
}

/// What `keel init` scaffolds and what the maintained action does must be the
/// same recipe. This is the divergence T2.3 deleted: the scaffold used to
/// `curl install.sh` and run `keel map --json --strict`, so a user following
/// keel's own instructions never saw keel's own annotations.
#[test]
fn the_scaffolded_workflow_uses_the_maintained_action() {
    let template = read("crates/keel-cli/templates/ci/github-actions.yml");
    assert!(
        template.contains("uses: FryrAI/Keel/.github/actions/keel@v0"),
        "the scaffold must call the maintained composite action"
    );
    assert!(
        template.contains("fetch-depth: 0"),
        "review parses the base side out of git; a shallow clone has none"
    );
    assert!(
        template.contains("pull_request"),
        "the scaffold must run on pull_request"
    );
    for forbidden in ["install.sh", "map --json --strict"] {
        assert!(
            !template.contains(forbidden),
            "the scaffold must not keep its own divergent recipe (`{forbidden}`)"
        );
    }
}

/// `bash -n` on a deliberately broken block must fail — otherwise the two
/// tests above would pass on anything.
#[test]
fn the_syntax_check_actually_checks() {
    assert!(bash_syntax_error("if [ 1 -eq 1 ]; then\n", "probe").is_some());
    assert!(bash_syntax_error("echo ok\n", "probe").is_none());
}

/// The block extractor must find the real scripts, not an empty set.
#[test]
fn the_extractor_dedents_a_run_block() {
    let yaml = "runs:\n  steps:\n    - name: x\n      run: |\n        set -e\n        echo hi\n    - name: y\n";
    let blocks = embedded_scripts(yaml);
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0], "set -e\necho hi\n");
}

// --------------------------------------------------------------------------
// The sticky comment, driven against a fake `gh`
//
// "Update only when the rendered body changes" is the rule that keeps a
// re-push from notifying every reviewer again, and nothing else in the repo
// exercises it. The stub records the REST calls the script makes and stores
// the one comment it manages.
// --------------------------------------------------------------------------

const GH_STUB: &str = r#"#!/usr/bin/env bash
state="${GH_STATE:?}"
method="GET"; url=""; body=""; prev=""
for a in "$@"; do
  [ "$prev" = "-X" ] && method="$a"
  case "$a" in
    body=*) body="${a#body=}" ;;
    repos/*) [ -z "$url" ] && url="$a" ;;
  esac
  prev="$a"
done
[ -n "$body" ] && [ "$method" = "GET" ] && method="POST"
echo "$method" >> "$state/calls.log"
case "$url" in
  */issues/comments/*)
    if [ "$method" = "PATCH" ]; then printf '%s' "$body" > "$state/comment.txt"; echo '{}';
    else [ -f "$state/comment.txt" ] && cat "$state/comment.txt"; fi ;;
  */comments)
    if [ "$method" = "POST" ]; then printf '%s' "$body" > "$state/comment.txt"; echo '{}';
    elif [ -f "$state/comment.txt" ]; then
      # What the script's --jq extracts from the list response: id and the
      # body-hash the comment carries, from the ONE call.
      h=$(grep -m1 -o 'keel:body-hash [0-9a-f]*' "$state/comment.txt" | cut -d' ' -f2)
      printf '1\t%s\n' "$h"
    fi ;;
esac
exit 0
"#;

/// A temp dir holding the stub `gh` and the state it writes.
struct StickyHarness {
    dir: tempfile::TempDir,
}

impl StickyHarness {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let gh = bin.join("gh");
        std::fs::write(&gh, GH_STUB).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        Self { dir }
    }

    /// Run the script over `review_output` and return the methods it called.
    fn run(&self, review_output: &str, is_fork: bool) -> Vec<String> {
        let root = self.dir.path();
        let review = root.join("review.txt");
        std::fs::write(&review, review_output).unwrap();
        let calls = root.join("calls.log");
        let _ = std::fs::remove_file(&calls);
        let path = format!(
            "{}:{}",
            root.join("bin").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let out = Command::new("bash")
            .arg(repo_root().join(".github/actions/keel/sticky_comment.sh"))
            .arg(&review)
            .env("PATH", path)
            .env("GH_STATE", root)
            .env("GH_TOKEN", "x")
            .env("GITHUB_REPOSITORY", "o/r")
            .env("PR_NUMBER", "5")
            .env("IS_FORK", if is_fork { "true" } else { "false" })
            .env("GITHUB_STEP_SUMMARY", root.join("summary.md"))
            .output()
            .expect("bash must be available");
        assert!(
            out.status.success(),
            "the comment step must never fail the job: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        std::fs::read_to_string(&calls)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn comment(&self) -> String {
        std::fs::read_to_string(self.dir.path().join("comment.txt")).unwrap_or_default()
    }
}

/// The whole point: post once, then stay quiet until the content changes.
#[test]
fn the_sticky_comment_is_posted_once_and_only_rewritten_on_change() {
    let h = StickyHarness::new();

    let calls = h.run("execute() signature changed in a.ts\n", false);
    assert_eq!(calls, ["GET", "POST"], "first run must create the comment");
    assert!(h.comment().contains("<!-- keel:sticky-review -->"));
    assert!(h.comment().contains("keel:body-hash"));

    let calls = h.run("execute() signature changed in a.ts\n", false);
    assert_eq!(
        calls,
        ["GET"],
        "an unchanged re-push must not rewrite the comment (no new notification), \
         and the one list call must answer both id and body-hash"
    );

    let calls = h.run("other() removed from b.ts\n", false);
    assert_eq!(
        calls,
        ["GET", "PATCH"],
        "changed content must update the existing comment in place"
    );
    assert!(h.comment().contains("other() removed"));

    let calls = h.run("", false);
    assert_eq!(
        calls,
        ["GET", "PATCH"],
        "a diff that went clean must update the comment rather than leave it scary"
    );
    assert!(h.comment().contains("No contract changes"));
}

/// Clean diff, nothing posted yet: say nothing at all — the same contract as
/// `keel compile` exiting 0 with empty stdout.
#[test]
fn a_clean_diff_with_no_existing_comment_posts_nothing() {
    let h = StickyHarness::new();
    assert_eq!(h.run("", false), ["GET"]);
    assert_eq!(h.comment(), "");
}

/// A fork's token cannot comment: no API call at all, and a green job.
#[test]
fn a_fork_makes_no_api_calls() {
    let h = StickyHarness::new();
    assert!(h.run("execute() changed in a.ts\n", true).is_empty());
    let summary =
        std::fs::read_to_string(h.dir.path().join("summary.md")).expect("summary must be written");
    assert!(summary.contains("execute() changed in a.ts"));
}
