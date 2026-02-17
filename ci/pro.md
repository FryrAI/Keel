# Keel Pro — Paid Tier Features

```yaml
status: planning
tiers: [free, team, enterprise]
monetization: usage-gated features, not feature-gated core
philosophy: "Free is complete. Pro is multiplied."
```

> The free tier is a complete structural enforcement tool. Paid tiers add **team visibility**, **naming governance**, **detailed analytics**, and **private infrastructure** — things that matter at scale, not things we held back.

---

## Tier Overview

| Feature | Free | Team | Enterprise |
|---------|------|------|------------|
| All CLI commands | Yes | Yes | Yes |
| All 4 languages | Yes | Yes | Yes |
| 11 tool integrations | Yes | Yes | Yes |
| Local telemetry | Yes | Yes | Yes |
| `keel config` | Yes | Yes | Yes |
| Circuit breaker + batch | Yes | Yes | Yes |
| MCP + HTTP server | Yes | Yes | Yes |
| **Detailed telemetry** | - | Yes | Yes |
| **Remote telemetry** | Aggregate only | Aggregate + detailed | Private endpoint |
| **Naming conventions UI** | - | Yes | Yes |
| **Team dashboard** | - | Yes | Yes |
| **Prompt performance** | - | Yes | Yes |
| **Private hosting** | - | - | Yes |
| **Encrypted export** | - | - | Yes |
| **SSO + audit log** | - | - | Yes |
| **Custom rules** | - | - | Yes |
| **SLA** | - | - | Yes |

---

## Feature Details

### 1. Naming Conventions (Team+)

**The problem:** Teams argue about naming. `getUserById` vs `get_user_by_id` vs `fetchUser`. Each agent picks whatever feels right. Over 200 PRs, you have 5 naming styles in one codebase.

**The solution:** `keel name` + a naming governance UI.

#### Config (already shipped in Free)

```json
{
  "naming_conventions": {
    "style": "snake_case",
    "prefixes": ["keel_", "test_"]
  }
}
```

#### CLI (Free — suggests, doesn't enforce)

```bash
$ keel name "validate user authentication" --module src/auth
  suggestion: validate_user_auth
  module:     src/auth/validators.ts
  score:      0.87
  convention: snake_case (from config)
```

#### Online UI (Team — configure per-directory rules)

A web dashboard at `app.keel.engineer` where teams:

1. **Define naming zones** — drag directories onto a canvas, assign a naming style
   - `src/api/` → `camelCase`, prefix `handle`
   - `src/models/` → `PascalCase`, no prefix
   - `src/utils/` → `snake_case`, prefix `util_`
   - `tests/` → `snake_case`, prefix `test_`

2. **Set naming rules per entity kind**
   - Functions: `camelCase` in TypeScript, `snake_case` in Python
   - Classes: `PascalCase` everywhere
   - Constants: `SCREAMING_SNAKE`
   - Modules/files: `kebab-case`

3. **Review violations** — dashboard shows naming drift with confidence scores
   - "87 functions match convention, 12 don't"
   - "Most common deviation: `get_` prefix in `src/api/` (should be `handle`)"

4. **Export to `keel.json`** — the UI writes back to config, `keel compile` enforces

#### Enforcement (Team — `keel compile` enforces naming)

New warning code:

```
WARNING W003 naming_convention
├── src/api/handlers.ts:47 function getUserById
├── convention: camelCase with prefix "handle" (from naming_conventions)
└── suggestion: handleGetUserById
```

- **Free tier:** `keel name` suggests, but compile doesn't check naming
- **Team tier:** `keel compile` emits W003 warnings for naming violations
- **Enterprise tier:** W003 can be escalated to ERROR via config

---

### 2. Detailed Telemetry (Team+)

**The problem:** Free telemetry tracks command-level aggregates (duration, exit codes, counts). Teams need to know *which functions* are causing violations, *which modules* drift most, and *how agent behavior changes over time*.

**Free telemetry (shipped — Round 14):**
- Command name, duration, exit code
- Error/warning counts
- Language mix percentages
- Resolution tier distribution
- No file paths, no function names, no code

**Team telemetry (`detailed: true`):**
- **Module-level heat map** — which modules have the most violations over time
- **Error code trends** — E001 count this week vs last week
- **Agent session tracking** — how many compile cycles per editing session
- **Resolution tier breakdown** — % of edges resolved at Tier 1/2/3
- **Circuit breaker frequency** — how often auto-downgrade fires

**Privacy contract (Team):**
- Module names are included (e.g., `src/auth/validators`)
- Function names are **hashed** (same xxhash as the graph — `kXt9mRp2v4L`)
- No source code, no variable names, no git history
- Encrypted in transit (HTTPS) and at rest (AES-256)

**Enterprise telemetry:**
- Everything in Team
- Raw module + function names (encrypted at rest)
- Custom retention policies (30/90/365 days)
- Export to your own analytics pipeline (S3, BigQuery, Snowflake)

---

### 3. Team Dashboard (Team+)

**URL:** `app.keel.engineer/dashboard`

A web dashboard that visualizes telemetry across repos and team members.

#### Views

**Overview:**
- Total invocations this week/month
- Avg compile time trend (line chart)
- Top 10 modules by violation count (bar chart)
- Language mix pie chart
- Error code distribution

**Modules:**
- Heat map of violation density per module
- "Structural health score" per module (0-100)
- Drift detection: modules whose health score dropped >10% this week
- Click through to module detail: functions, callers, recent violations

**Agents:**
- Per-tool breakdown (Claude Code, Cursor, Gemini CLI, etc.)
- Avg compile cycles per editing session
- "Fix success rate" — % of violations that get fixed within the same session
- "Escalation rate" — % of violations that survive to the next session

**Naming:**
- Convention compliance score per directory
- Naming drift over time (line chart)
- "Worst offenders" list with suggested renames
- One-click export to keel.json

#### Data Flow

```
keel CLI → telemetry.db (local)
           ↓ (opt-in, Team+ only)
         POST api.keel.engineer/telemetry
           ↓
         Dashboard DB (encrypted)
           ↓
         app.keel.engineer/dashboard
```

---

### 4. Prompt Performance Tracking (Team+)

**The problem:** You're using keel to enforce structure, but you don't know if it's actually making agents better. Are they producing fewer violations over time? Are some agents better than others?

**Metrics tracked:**

| Metric | Description | Insight |
|--------|-------------|---------|
| Violations per session | Errors + warnings per editing session | "Agents are improving over time" |
| First-compile success rate | % of compiles that pass on first try | "Claude Code: 73%, Cursor: 61%" |
| Fix latency | Time from violation to fix | "Avg 12s — agents fix fast when told what's wrong" |
| Repeat violations | Same error code on same hash | "E001 on auth module — 3 times this week" |
| Backpressure compliance | How often agents respect BUDGET directives | "87% of HIGH pressure → contract response" |

#### Dashboard Widget

```
Prompt Performance (last 7 days)
  First-compile success: 71% (+3% from last week)
  Avg fix latency:       14s
  Repeat violations:     8 (down from 12)
  Backpressure:          89% compliance
```

---

### 5. Private Hosting (Enterprise)

**The problem:** Some organizations can't send telemetry to a third-party server. They need keel's dashboard running on their own infrastructure.

**What we provide:**
- Docker image: `ghcr.io/fryrai/keel-dashboard:latest`
- Helm chart for Kubernetes
- Terraform module for AWS/GCP
- Single binary for bare metal (same Rust stack)

**Architecture:**

```
Your infrastructure:
  keel CLI → your-keel.internal/telemetry (POST)
           → your-keel.internal/dashboard (GET)
           → PostgreSQL (your DB)
           → S3/GCS (exports)
```

**Requirements:** PostgreSQL 14+, 2 vCPU, 4GB RAM. Handles 100 developers.

---

### 6. Encrypted Export (Enterprise)

**The problem:** Enterprise teams need to export structural data for compliance, audit, or integration with internal tools — but the data contains module/function names.

**Export formats:**
- JSON (full graph + telemetry)
- CSV (flat tables for analytics)
- Parquet (for data lake ingestion)

**Encryption:**
- AES-256-GCM at rest
- Customer-managed keys (BYOK)
- PGP-signed exports for chain of custody

---

### 7. Custom Rules (Enterprise)

**The problem:** Some teams have architectural rules that go beyond keel's built-in error codes. "No direct database calls from API handlers." "All public functions must have integration tests."

**Rule DSL (future):**

```yaml
# .keel/rules/no-direct-db.yml
rule: no-direct-db-from-handlers
description: "API handlers must not call database functions directly"
severity: ERROR
match:
  caller:
    module: "src/api/**"
    kind: function
  callee:
    module: "src/db/**"
    kind: function
message: "Use a service layer between API handlers and database calls"
```

- Rules live in `.keel/rules/` (committed to repo)
- `keel compile` loads and evaluates custom rules alongside built-in E001-E005
- Custom rules get error codes C001-C999
- Rules can reference the structural graph (callers, callees, modules, types)

---

## Implementation Roadmap

### Phase 1: Foundation (Round 14 — DONE)
- [x] `Tier` enum in config (Free/Team/Enterprise)
- [x] `TelemetryConfig` with `detailed` and `remote` flags
- [x] `NamingConventionsConfig` stub with `style` and `prefixes`
- [x] Local telemetry engine (telemetry.db)
- [x] `keel config` command for all settings
- [x] `keel stats` shows telemetry aggregate

### Phase 2: Remote Telemetry
- [ ] HTTP POST to `api.keel.engineer/telemetry`
- [ ] Fire-and-forget with 2s timeout
- [ ] `TelemetryAggregate` payload (never raw events for Free)
- [ ] Opt-out: `keel config telemetry.remote false`
- [ ] Server: receive, store, aggregate (simple Rust service)

### Phase 3: Naming Enforcement
- [ ] W003 naming_convention warning in `keel compile`
- [ ] Per-directory naming rules in keel.json
- [ ] Per-entity-kind rules (function, class, constant, module)
- [ ] `keel name` respects config when suggesting

### Phase 4: Team Dashboard
- [ ] `api.keel.engineer/dashboard` — read-only web UI
- [ ] `api_key` field in KeelConfig for authentication
- [ ] Module heat map, error trends, agent comparison
- [ ] Naming compliance view with one-click export

### Phase 5: Enterprise
- [ ] Docker image + Helm chart for private hosting
- [ ] Encrypted export (AES-256-GCM, BYOK)
- [ ] Custom rules DSL
- [ ] SSO (SAML/OIDC)
- [ ] Audit log

---

## Pricing (Draft)

| Tier | Price | Target |
|------|-------|--------|
| **Free** | $0 forever | Solo devs, OSS, evaluation |
| **Team** | $29/user/month | Teams of 3-50, startups |
| **Enterprise** | Custom | Compliance-driven orgs |

**Free is not a trial.** It's the complete CLI tool with local telemetry. Team adds visibility. Enterprise adds governance.

---

## Key Design Principles

1. **Free is complete.** Every CLI command, every language, every integration. No feature walls on the core tool.
2. **Pro is visibility.** Paid tiers add dashboards, trends, and governance — things that matter when 5+ developers use keel daily.
3. **Privacy scales with trust.** Free = no identifying data. Team = hashed identifiers. Enterprise = encrypted raw data.
4. **Config drives everything.** Every pro feature is configured in `keel.json`. The dashboard writes to the same config file. No hidden server-side state.
5. **Opt-OUT, not opt-in.** Remote telemetry defaults to `true` because we need early data. One command to disable. No dark patterns.
