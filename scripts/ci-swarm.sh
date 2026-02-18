#!/usr/bin/env bash
set -euo pipefail

# ============================================================================
# ci-swarm.sh — Launch the continuous improvement agent swarm
#
# Usage:
#   bash scripts/ci-swarm.sh            # Full launch
#   bash scripts/ci-swarm.sh --dry-run  # Check prerequisites only
# ============================================================================

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SESSION="keel-ci"
DRY_RUN=false

# Worktree paths
WT_TEST_INFRA="$HOME/keel-ci-test-infra"
WT_ENFORCEMENT="$HOME/keel-ci-enforcement"
WT_BUGS="$HOME/keel-ci-bugs"

# Prompt directory
PROMPT_DIR="$REPO_ROOT/scripts/ci-prompts"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Parse args
for arg in "$@"; do
    case $arg in
        --dry-run) DRY_RUN=true ;;
        *) echo "Unknown arg: $arg"; exit 1 ;;
    esac
done

# ============================================================================
# Prerequisites check
# ============================================================================
check_prereqs() {
    local failed=0

    echo "=== Checking prerequisites ==="

    # Rust toolchain
    if command -v cargo &>/dev/null; then
        echo -e "  ${GREEN}✓${NC} cargo $(cargo --version 2>/dev/null | cut -d' ' -f2)"
    else
        echo -e "  ${RED}✗${NC} cargo not found"; failed=1
    fi

    # bubblewrap (Linux only)
    if [[ "$(uname)" == "Linux" ]]; then
        if command -v bwrap &>/dev/null; then
            echo -e "  ${GREEN}✓${NC} bwrap $(bwrap --version 2>/dev/null)"
        else
            echo -e "  ${RED}✗${NC} bwrap not found (apt install bubblewrap)"; failed=1
        fi
    fi

    # socat
    if command -v socat &>/dev/null; then
        echo -e "  ${GREEN}✓${NC} socat installed"
    else
        echo -e "  ${RED}✗${NC} socat not found (apt install socat)"; failed=1
    fi

    # tmux
    if command -v tmux &>/dev/null; then
        echo -e "  ${GREEN}✓${NC} tmux $(tmux -V 2>/dev/null)"
    else
        echo -e "  ${RED}✗${NC} tmux not found"; failed=1
    fi

    # Claude Code
    if command -v claude &>/dev/null; then
        echo -e "  ${GREEN}✓${NC} claude installed"
    else
        echo -e "  ${RED}✗${NC} claude not found"; failed=1
    fi

    # Agent teams enabled
    if claude settings get env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS 2>/dev/null | grep -q "1"; then
        echo -e "  ${GREEN}✓${NC} agent teams enabled"
    else
        echo -e "  ${YELLOW}?${NC} agent teams setting not verified (check settings.json manually)"
    fi

    # Prompt files exist
    if [[ -d "$PROMPT_DIR" ]]; then
        local count
        count=$(ls "$PROMPT_DIR"/*.md 2>/dev/null | wc -l)
        if [[ "$count" -ge 3 ]]; then
            echo -e "  ${GREEN}✓${NC} $count prompt files in $PROMPT_DIR"
        else
            echo -e "  ${RED}✗${NC} Expected 3 prompt files, found $count"; failed=1
        fi
    else
        echo -e "  ${RED}✗${NC} Prompt directory missing: $PROMPT_DIR"; failed=1
    fi

    # Clean git state
    if [[ -z "$(git -C "$REPO_ROOT" status --porcelain 2>/dev/null)" ]]; then
        echo -e "  ${GREEN}✓${NC} git working tree clean"
    else
        echo -e "  ${YELLOW}!${NC} git working tree has uncommitted changes"
    fi

    # Baseline test check
    echo ""
    echo "=== Baseline test status ==="
    if cargo test --workspace --quiet 2>&1 | tail -3; then
        echo -e "  ${GREEN}✓${NC} baseline tests captured"
    else
        echo -e "  ${YELLOW}!${NC} some tests failing (swarm will attempt to fix)"
    fi

    if [[ $failed -ne 0 ]]; then
        echo ""
        echo -e "${RED}Prerequisites not met. Fix issues above before launching.${NC}"
        exit 1
    fi

    echo ""
    echo -e "${GREEN}All prerequisites met.${NC}"
}

# ============================================================================
# Create worktrees
# ============================================================================
create_worktrees() {
    echo ""
    echo "=== Setting up worktrees ==="
    cd "$REPO_ROOT"

    for wt_path in "$WT_TEST_INFRA" "$WT_ENFORCEMENT" "$WT_BUGS"; do
        local branch
        branch=$(basename "$wt_path" | sed 's/keel-ci-//')
        branch="ci/${branch}"

        if [[ -d "$wt_path" ]]; then
            echo -e "  ${YELLOW}→${NC} Reusing existing worktree: $wt_path"
        else
            echo -e "  ${GREEN}+${NC} Creating worktree: $wt_path (branch: $branch)"
            git worktree add "$wt_path" -b "$branch" 2>/dev/null || \
                git worktree add "$wt_path" "$branch" 2>/dev/null || \
                { echo -e "  ${RED}✗${NC} Failed to create worktree"; exit 1; }
        fi
    done

    git worktree list
}

# ============================================================================
# Write launch scripts (avoids tmux + zsh + backtick issues)
# ============================================================================
write_launch_scripts() {
    echo ""
    echo "=== Writing launch scripts ==="
    mkdir -p /tmp/claude/

    # Pane 1: Test Infrastructure
    cat > /tmp/claude/launch-test-infra.sh <<SCRIPT
#!/usr/bin/env bash
cd "$WT_TEST_INFRA"
claude --dangerously-skip-permissions -p "\$(cat "$PROMPT_DIR/test-infra.md")"
SCRIPT

    # Pane 2: Enforcement
    cat > /tmp/claude/launch-enforcement.sh <<SCRIPT
#!/usr/bin/env bash
cd "$WT_ENFORCEMENT"
claude --dangerously-skip-permissions -p "\$(cat "$PROMPT_DIR/enforcement.md")"
SCRIPT

    # Pane 3: Bugs
    cat > /tmp/claude/launch-bugs.sh <<SCRIPT
#!/usr/bin/env bash
cd "$WT_BUGS"
claude --dangerously-skip-permissions -p "\$(cat "$PROMPT_DIR/bugs.md")"
SCRIPT

    chmod +x /tmp/claude/launch-*.sh
    echo -e "  ${GREEN}✓${NC} Launch scripts written to /tmp/claude/"
}

# ============================================================================
# Launch tmux session
# ============================================================================
launch_tmux() {
    echo ""
    echo "=== Launching tmux session: $SESSION ==="

    # Kill existing session if present
    tmux kill-session -t "$SESSION" 2>/dev/null || true

    # Create session — Pane 0 (orchestrator) in root repo
    tmux new-session -d -s "$SESSION" -n "swarm" -c "$REPO_ROOT"

    # Split into 4 panes (all splits first, then send-keys)
    # Bug fix: interleaving splits + send-keys causes pane index shift
    tmux split-window -h -t "$SESSION:0" -c "$WT_TEST_INFRA"
    tmux split-window -v -t "$SESSION:0.0" -c "$WT_ENFORCEMENT"
    tmux split-window -v -t "$SESSION:0.1" -c "$WT_BUGS"

    # After splits: 0=top-left(orch), 1=top-right(test-infra),
    #               2=bottom-left(enforce), 3=bottom-right(bugs)

    # Small delay for panes to initialize their shells
    sleep 2

    # Send launch commands to each agent pane
    tmux send-keys -t "$SESSION:0.1" "bash /tmp/claude/launch-test-infra.sh" C-m
    tmux send-keys -t "$SESSION:0.2" "bash /tmp/claude/launch-enforcement.sh" C-m
    tmux send-keys -t "$SESSION:0.3" "bash /tmp/claude/launch-bugs.sh" C-m

    # Select pane 0 (orchestrator) for human interaction
    tmux select-pane -t "$SESSION:0.0"

    echo ""
    echo -e "${GREEN}=== Swarm launched ===${NC}"
    echo ""
    echo "  Pane 0 (top-left):     Orchestrator — YOU interact here"
    echo "  Pane 1 (top-right):    Test Infrastructure agent"
    echo "  Pane 2 (bottom-left):  Enforcement agent"
    echo "  Pane 3 (bottom-right): Bug-fixing agent"
    echo ""
    echo "  Attach: tmux attach -t $SESSION"
    echo "  Monitor: /tmux-observe (from pane 0)"
    echo "  Stop: tmux kill-session -t $SESSION"
    echo ""

    # Attach only in interactive mode
    if [ -t 0 ]; then
        tmux attach -t "$SESSION"
    else
        echo "Non-interactive: run 'tmux attach -t $SESSION' to connect"
    fi
}

# ============================================================================
# Main
# ============================================================================
main() {
    echo "============================================"
    echo "  keel continuous improvement swarm"
    echo "============================================"
    echo ""

    check_prereqs

    if $DRY_RUN; then
        echo ""
        echo -e "${GREEN}Dry run complete. All prerequisites met.${NC}"
        echo "Run without --dry-run to launch the swarm."
        exit 0
    fi

    create_worktrees
    write_launch_scripts
    launch_tmux
}

main "$@"
