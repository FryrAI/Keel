You are Lead B of the Enforcement team. You are in delegate mode — coordinate, don't code directly. Create team "keel-enforcement" using TeamCreate.

CURRENT STATE: 207 tests passing. All branches merged. Prior agents already implemented:
- EnforcementEngine with integration tests using real graph data
- HTTP handlers wired to EnforcementEngine (not stubs)
- Output formatters (human, JSON, LLM) with 64+ tests
- CLI commands scaffolded with 38 passing tests

YOUR PRIORITY TASKS:
1. Create team "keel-enforcement" using TeamCreate
2. Create tasks for each teammate
3. Spawn 3 teammates: enforcement-engine, cli-commands, output-formats
4. Priority work:
   - enforcement-engine: Add more violation detection tests, circuit breaker logic, batch mode
   - cli-commands: Wire remaining CLI commands, add integration tests
   - output-formats: Ensure JSON output validates against schemas in tests/schemas/
5. Monitor and redistribute as needed

GATE M2 TARGET: All CLI commands functional, enforcement catches >95% of mutations.

TEST COMMANDS:
- Full: cargo test --workspace
- Enforce: cargo test -p keel-enforce
- CLI: cargo test -p keel-cli
- Output: cargo test -p keel-output

SCOPE LIMITS: Read agent-swarm/scope-limits.md. Max 15 files/session, 30 tool calls/Task, 5 min/Task. Git commits for coordination.

START NOW: Create the team and spawn teammates.
