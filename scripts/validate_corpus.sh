#!/usr/bin/env bash
set -euo pipefail

# Validate keel against a test corpus. Runs init/map/compile/stats on each repo,
# captures metrics JSON, and detects regressions from previous rounds.
#
# Usage: ./scripts/validate_corpus.sh --round N --corpus /tmp/claude/test-corpus [--quick]
# --quick: only re-run repos that had issues in previous round

ROUND=0
CORPUS_DIR="/tmp/claude/test-corpus"
METRICS_BASE="/tmp/claude/metrics"
KEEL_BIN="$(cd "$(dirname "$0")/.." && pwd)/target/release/keel"
QUICK=false
LARGE=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --round) ROUND="$2"; shift 2 ;;
        --corpus) CORPUS_DIR="$2"; shift 2 ;;
        --keel) KEEL_BIN="$2"; shift 2 ;;
        --quick) QUICK=true; shift ;;
        --large) LARGE=true; shift ;;
        *) echo "Unknown arg: $1"; exit 2 ;;
    esac
done

METRICS_DIR="$METRICS_BASE/round-$ROUND"
PREV_DIR="$METRICS_BASE/round-$((ROUND - 1))"
mkdir -p "$METRICS_DIR"

if [ ! -x "$KEEL_BIN" ]; then
    echo "ERROR: keel binary not found at $KEEL_BIN"
    exit 2
fi

echo "=== Corpus Validation Round $ROUND ==="
echo "Corpus: $CORPUS_DIR"
echo "Binary: $KEEL_BIN"
echo "Metrics: $METRICS_DIR"
echo ""

TOTAL=0
PASSED=0
FAILED=0
REGRESSIONS=0

detect_language() {
    local repo_dir="$1"
    if find "$repo_dir" -maxdepth 3 -name "*.go" -print -quit 2>/dev/null | grep -q .; then
        echo "go"
    elif find "$repo_dir" -maxdepth 3 -name "*.rs" -print -quit 2>/dev/null | grep -q .; then
        echo "rust"
    elif find "$repo_dir" -maxdepth 3 -name "*.ts" -print -quit 2>/dev/null | grep -q .; then
        echo "typescript"
    elif find "$repo_dir" -maxdepth 3 -name "*.py" -print -quit 2>/dev/null | grep -q .; then
        echo "python"
    else
        echo "unknown"
    fi
}

# Query SQLite via Python (sqlite3 CLI may not be installed)
query_db_metrics() {
    local db_file="$1"
    if [ ! -f "$db_file" ]; then
        echo "0 0 0 0 0 0 0"
        return
    fi
    python3 -c "
import sqlite3, sys
try:
    db = sqlite3.connect('$db_file')
    c = db.cursor()
    nodes = c.execute('SELECT COUNT(*) FROM nodes').fetchone()[0]
    edges = c.execute('SELECT COUNT(*) FROM edges').fetchone()[0]
    calls = c.execute(\"SELECT COUNT(*) FROM edges WHERE kind = 'calls'\").fetchone()[0]
    imports = c.execute(\"SELECT COUNT(*) FROM edges WHERE kind = 'imports'\").fetchone()[0]
    contains = c.execute(\"SELECT COUNT(*) FROM edges WHERE kind = 'contains'\").fetchone()[0]
    xfile = c.execute(\"\"\"SELECT COUNT(*) FROM edges e
        WHERE e.kind = 'calls'
        AND (SELECT file_path FROM nodes WHERE id = e.source_id)
         != (SELECT file_path FROM nodes WHERE id = e.target_id)\"\"\").fetchone()[0]
    files = c.execute('SELECT COUNT(DISTINCT file_path) FROM nodes WHERE file_path IS NOT NULL').fetchone()[0]
    print(f'{nodes} {edges} {calls} {imports} {contains} {xfile} {files}')
    db.close()
except Exception as e:
    print('0 0 0 0 0 0 0', file=sys.stderr)
    print('0 0 0 0 0 0 0')
" 2>/dev/null || echo "0 0 0 0 0 0 0"
}

query_orphans() {
    local db_file="$1"
    if [ ! -f "$db_file" ]; then
        echo "0"
        return
    fi
    python3 -c "
import sqlite3
db = sqlite3.connect('$db_file')
c = db.cursor()
n = c.execute('''SELECT COUNT(*) FROM edges
    WHERE source_id NOT IN (SELECT id FROM nodes)
       OR target_id NOT IN (SELECT id FROM nodes)''').fetchone()[0]
print(n)
db.close()
" 2>/dev/null || echo "0"
}

validate_repo() {
    local name="$1"
    local repo_dir="$CORPUS_DIR/$name"
    local metrics_file="$METRICS_DIR/${name}.json"
    local stderr_file="/tmp/claude/validate_stderr_${name}.txt"
    local lang
    lang=$(detect_language "$repo_dir")

    echo "--- $name ($lang) ---"

    # Init if needed (keel uses CWD)
    if [ ! -d "$repo_dir/.keel" ]; then
        (cd "$repo_dir" && "$KEEL_BIN" init 2>"$stderr_file") || true
    fi

    # Map with timing (keel uses CWD)
    local map_start map_end map_ms
    map_start=$(date +%s%3N)
    (cd "$repo_dir" && "$KEEL_BIN" map >/dev/null 2>"$stderr_file") || true
    map_end=$(date +%s%3N)
    map_ms=$((map_end - map_start))

    # Compile with timing
    local compile_start compile_end compile_ms compile_exit
    compile_start=$(date +%s%3N)
    local compile_out="/tmp/claude/validate_compile_${name}.json"
    (cd "$repo_dir" && "$KEEL_BIN" compile --json >"$compile_out" 2>>"$stderr_file") || true
    compile_exit=$?
    compile_end=$(date +%s%3N)
    compile_ms=$((compile_end - compile_start))

    # Query metrics directly from SQLite (more reliable than stats --json)
    local db_file="$repo_dir/.keel/graph.db"
    local db_metrics
    db_metrics=$(query_db_metrics "$db_file")
    local nodes edges_total edges_calls edges_imports edges_contains cross_file files
    read -r nodes edges_total edges_calls edges_imports edges_contains cross_file files <<< "$db_metrics"

    local orphaned
    orphaned=$(query_orphans "$db_file")

    # Count violations from compile output
    local violations_error=0
    local violations_warning=0

    # Collect stderr errors
    local stderr_errors="[]"
    if [ -f "$stderr_file" ] && [ -s "$stderr_file" ]; then
        stderr_errors=$(grep -i "error\|panic\|crash\|FOREIGN\|UNIQUE" "$stderr_file" | head -5 | python3 -c "
import sys,json
lines=[l.strip() for l in sys.stdin if l.strip()]
print(json.dumps(lines))
" 2>/dev/null || echo "[]")
    fi

    # Write metrics JSON
    cat > "$metrics_file" <<METRICS_EOF
{
  "repo": "$name",
  "language": "$lang",
  "round": $ROUND,
  "map_time_ms": $map_ms,
  "compile_time_ms": $compile_ms,
  "compile_exit": $compile_exit,
  "nodes": $nodes,
  "edges_total": $edges_total,
  "edges_calls": $edges_calls,
  "edges_imports": $edges_imports,
  "edges_contains": $edges_contains,
  "cross_file_edges": $cross_file,
  "orphaned_edges": $orphaned,
  "files": $files,
  "violations_error": $violations_error,
  "violations_warning": $violations_warning,
  "stderr_errors": $stderr_errors
}
METRICS_EOF

    # Print summary line
    local status="OK"
    if [ "$compile_exit" -eq 2 ]; then
        status="CRASH"
        FAILED=$((FAILED + 1))
    elif [ "$orphaned" -gt 0 ]; then
        status="ORPHANS($orphaned)"
        FAILED=$((FAILED + 1))
    elif [ "$cross_file" -eq 0 ] && [ "$nodes" -gt 10 ]; then
        status="NO_XFILE"
        FAILED=$((FAILED + 1))
    else
        PASSED=$((PASSED + 1))
    fi

    printf "  %-12s  map=%5dms  compile=%5dms  nodes=%-5s edges=%-5s xfile=%-5s orphans=%-3s  [%s]\n" \
        "$name" "$map_ms" "$compile_ms" "$nodes" "$edges_total" "$cross_file" "$orphaned" "$status"

    TOTAL=$((TOTAL + 1))

    # Regression check against previous round
    if [ -f "$PREV_DIR/${name}.json" ]; then
        check_regression "$name" "$PREV_DIR/${name}.json" "$metrics_file"
    fi

    rm -f "$stderr_file" "/tmp/claude/validate_compile_${name}.json"
}

check_regression() {
    local name="$1"
    local prev_file="$2"
    local curr_file="$3"

    python3 -c "
import json, sys
with open('$prev_file') as f: prev = json.load(f)
with open('$curr_file') as f: curr = json.load(f)
issues = []
if prev.get('compile_exit',0) != 2 and curr.get('compile_exit',0) == 2:
    issues.append('REGRESSION: compile now crashes')
if prev.get('cross_file_edges',0) > 0 and curr.get('cross_file_edges',0) == 0:
    issues.append('REGRESSION: cross_file_edges dropped to 0')
if prev.get('orphaned_edges',0) == 0 and curr.get('orphaned_edges',0) > 0:
    issues.append('REGRESSION: orphaned edges appeared')
if prev.get('nodes',0) > 0 and curr.get('nodes',0) == 0:
    issues.append('REGRESSION: nodes dropped to 0')
for i in issues:
    print(f'  ** {i} for $name')
sys.exit(1 if issues else 0)
" 2>/dev/null || REGRESSIONS=$((REGRESSIONS + 1))
}

# Main loop — sort for deterministic order
for repo_dir in $(find "$CORPUS_DIR" -mindepth 1 -maxdepth 1 -type d | sort); do
    name=$(basename "$repo_dir")
    [ "$name" = "keel-self" ] && continue  # skip self-reference

    if $QUICK && [ -f "$PREV_DIR/${name}.json" ]; then
        prev_status=$(python3 -c "
import json
with open('$PREV_DIR/${name}.json') as f: d=json.load(f)
ok = d.get('compile_exit',0) != 2 and d.get('orphaned_edges',0) == 0 and (d.get('cross_file_edges',0) > 0 or d.get('nodes',0) <= 10)
print('ok' if ok else 'fail')
" 2>/dev/null || echo "fail")
        if [ "$prev_status" = "ok" ]; then
            echo "--- $name [skip: was green] ---"
            cp "$PREV_DIR/${name}.json" "$METRICS_DIR/${name}.json"
            TOTAL=$((TOTAL + 1))
            PASSED=$((PASSED + 1))
            continue
        fi
    fi

    validate_repo "$name"
done

if $LARGE; then
    echo ""
    echo "=== Large repos (--large) ==="
    # These are optional, slow repos for thorough validation
    for repo_dir in $(find "$CORPUS_DIR" -mindepth 1 -maxdepth 1 -type d | sort); do
        name=$(basename "$repo_dir")
        case "$name" in
            vscode|terraform) validate_repo "$name" ;;
        esac
    done
fi

echo ""
echo "=== Round $ROUND Summary ==="
echo "Total: $TOTAL  Passed: $PASSED  Failed: $FAILED  Regressions: $REGRESSIONS"

if [ "$FAILED" -eq 0 ] && [ "$REGRESSIONS" -eq 0 ]; then
    echo "STATUS: GREEN"
    exit 0
else
    echo "STATUS: RED ($FAILED failures, $REGRESSIONS regressions)"
    exit 1
fi
