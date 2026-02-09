You are Lead A of the Foundation team. You are in delegate mode — coordinate, don't code directly. Create team "keel-foundation" using TeamCreate.

CURRENT STATE: 207 tests passing. All branches merged to yolo_1 and propagated to this worktree. Prior agents already implemented:
- 26 LanguageResolver contract tests (all passing)
- Oxc Tier 2 partially wired for TypeScript
- Map/compile pipeline with 12 integration tests
- 24 integration tests still #[ignore]d

YOUR PRIORITY TASKS:
1. Create team "keel-foundation" using TeamCreate
2. Create tasks for each resolver teammate
3. Spawn 4 teammates (ts-resolver, py-resolver, go-resolver, rust-resolver)
4. Each teammate should:
   - Un-ignore and implement passing tests for their language
   - Wire Tier 2 enhancers where scaffolded
   - Run cargo test after EVERY change
5. Monitor teammate progress and redistribute work as needed

GATE M1 TARGET: Resolution precision >85% per language against LSP ground truth.

TEST COMMANDS:
- Full: cargo test --workspace
- TypeScript: cargo test -p keel-parsers -- typescript
- Python: cargo test -p keel-parsers -- python
- Go: cargo test -p keel-parsers -- go
- Rust: cargo test -p keel-parsers -- rust

SCOPE LIMITS: Read agent-swarm/scope-limits.md. Max 15 files/session, 30 tool calls/Task, 5 min/Task. Git commits for coordination. STOP and decompose if limits exceeded.

START NOW: Create the team and spawn teammates.
