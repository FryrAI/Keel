# Positioning Synthesis

Sources: [[perplexity_pos]], [[gemini_pos]], [[claude_pos]]

---

## Enforcement Gap Validation

**All three sources confirm the enforcement gap is real.** The AI coding landscape has three occupied categories and one empty one:

| # | Category | What It Does | Examples |
|---|----------|-------------|---------|
| 1 | **Context providers** | Help the LLM understand the codebase | Augment Code, Aider repo map, Cursor semantic indexing |
| 2 | **Review-time checkers** | Catch problems after code is written | Qodo, Greptile, CodeQL, Snyk, Semgrep |
| 3 | **Agent guardrails** | Constrain agent behavior generically | LangGraph, Letta memory, MCP-Scan, Codex sandboxing |
| 4 | **Structural enforcement during generation** | Prevent invalid code from being written in real time | **Empty — keel's category** |

The gap is specific: no shipping product deterministically enforces architectural contracts (type signatures, module boundaries, adjacency constraints) during LLM code generation. Current enforcement relies on "Prompt & Pray" — giving the LLM context and instructions and hoping it complies.

**Perplexity** emphasizes this is a gap between "raw hooks and a general-purpose, language-aware enforcement product." **Gemini** frames it as the missing "controller" in a market flooded with generators and reviewers. **Claude** calls it "empirically real" and notes developers are already building DIY solutions to fill it (ArchCodex, Giga AI, the German "Vertragssystem").

---

## Nearest Competitors

### Direct threats

| Competitor | Why It Matters | Why It's Not keel |
|-----------|---------------|-------------------|
| **Codacy Guardrails** | Real-time interception of AI-generated code using SAST/SCA — genuine during-generation enforcement | Enforces security/quality patterns, not architectural contracts. No concept of module boundaries, interface signatures, or adjacency. |
| **AWS Kiro** | Spec-driven workflow with `.kiro/steering/` markdown that produces architecture violation warnings | Enforcement is LLM-mediated (the model reads natural language rules). No external formal checker — if the LLM misinterprets, violations pass. |
| **Qodo (CodiumAI)** | "Agentic code integrity platform" — compliance checks, breaking change detection | Post-hoc review (PR-level). Does not intervene during generation. "The Inspector, not the Guardrail." |
| **ArchCodex** (open source) | Four-layer architecture (Boundaries, Constraints, Examples, Validation) mapping precisely to keel's value prop | Single developer, open source. Validates the market but represents a free alternative. |
| **"Vertragssystem"** | 16 contracts with ~142 rules enforced via Claude Code PreToolUse hooks — real-time pattern-based enforcement | Third-party hack built for one stack (PHP). Mostly regex/textual patterns, not AST-level or cross-language. |

### Adjacent competitors (context/review layer)

| Tool | Relationship to keel |
|------|---------------------|
| **Augment Code Context Engine** | Complementary — context provider, not enforcement. See Augment section below. |
| **Sourcegraph Amp** | Knowledge-first agent using code graph for impact analysis. Relies on model compliance, not system enforcement. |
| **Cursor + Semgrep** | Emerging pattern (hooks + SAST). keel must differentiate beyond "wired Semgrep to hooks." |
| **Letta Code** | Memory-first coding agent with hooks. Provides enforcement primitives but doesn't ship contract enforcement. |

### Blind spots flagged by sources

- **Qodo** may be closer to architectural enforcement than fully captured — positions as "agentic code integrity platform" (Claude)
- **Kiro's evolution** — if AWS invests in making steering-doc enforcement deterministic rather than LLM-mediated, it could close the gap with AWS's distribution advantage (Claude)
- **GitHub Copilot hooks** — a fifth hook system not in the original thesis, with massive market share (Claude)

---

## Hook System Audit

### Converged table of 7 tools with hooks

| Tool | Hook System? | Events | Blocking Mechanism | Config Location | Key Finding |
|------|-------------|--------|-------------------|----------------|-------------|
| **Claude Code** | Yes (most mature) | ~13-14 events (PreToolUse, PostToolUse, Stop, SubagentStop, SessionStart, SessionEnd, UserPromptSubmit, Notification, PermissionRequest, PreCompact, SubagentStart, TaskCompleted, PostToolUseFailure, TeammateIdle) | Exit code 2 blocks; stderr fed back to model | `~/.claude/settings.json` | 3 handler types: command, prompt, **agent** (spawns sub-agent) |
| **Cursor** | Yes (v1.7, Oct 2025) | ~10 events (beforeShellExecution, beforeMCPExecution, beforeReadFile, afterFileEdit, afterShellExecution, afterMCPExecution, afterAgentResponse, afterAgentThought, stop, beforeSubmitPrompt) | Exit code 2 blocks **AND** JSON `permission: "deny"` | `.cursor/hooks.json` | **v2.0 regression**: `agent_message` field in hook responses is ignored, limiting ability to explain *why* a block happened |
| **Gemini CLI** | Yes (v0.26.0, Jan 2026) | ~8-11 events (BeforeAgent, AfterAgent, BeforeModel, AfterModel, BeforeTool, AfterTool, BeforeToolSelection, SessionStart, SessionEnd, Notification) | Exit code 2 = "System Block"; also JSON `decision: "deny"` | `.gemini/settings.json` | **Unique: AfterAgent with exit 2 triggers automatic retry turn** — self-correction loop without user intervention |
| **Windsurf** | Yes (Cascade Hooks, late 2025) | ~11 events (pre/post for user_prompt, read_code, write_code, run_command, mcp_tool_use) | Exit code 2 blocks on **pre-hooks only** | `.windsurf/hooks.json` | Post-hooks are informational only; pre-hooks must be fast (synchronous) |
| **Letta Code** | Yes | ~12 events (PermissionRequest, UserPromptSubmit, Notification, Stop, SubagentStop, PreCompact, Setup, SessionStart, SessionEnd, PreToolUse, PostToolUse) | Exit code 2 blocks | Letta config | Very similar semantics to Claude Code; validates the pattern |
| **GitHub Copilot** | Partial (MCP policies) | ~5 events; JSON-based decisions (`permissionDecision: "deny"`) | JSON-based, not exit code 2 | Copilot settings | MCP allowlists and registry policies — governance layer, not scriptable hooks |
| **Codex CLI** | **No** | None | None | N/A | GitHub issue #2109 is top community request. Users explicitly compare unfavorably to Claude Code. |

### Critical findings across sources

1. **Exit code 2 is the universal blocking convention** — Claude Code, Gemini CLI, Windsurf, and Letta all use it. keel can standardize on this.
2. **Cursor uses JSON, not just exit code 2** — needs a Cursor-specific adapter that returns `{ permission: "deny" }` JSON.
3. **Gemini's AfterAgent retry is unique and powerful** — keel could inspect code in AfterAgent, fail it, and force an immediate rewrite loop without user intervention.
4. **Cursor v2.0 regression** — `agent_message`/`userMessage` fields are ignored in v2.0+, limiting context injection back to the agent. This limits keel's ability to explain *why* in Cursor.
5. **Letta Code is the 5th hook tool** — not just Claude/Cursor/Gemini/Windsurf. Same exit-code-2 semantics.
6. **GitHub Copilot has hooks too** — JSON-based, not exit-code-2, but represents an expansion opportunity given Copilot's market share.
7. **keel can be enforced in 5 tools** (Claude Code, Cursor, Gemini CLI, Windsurf, Letta), not the original claim of 1. This is a significant tailwind.

---

## Buyer Persona

### Primary: VP/Director Engineering + Platform/DevEx Leads

All three sources converge on this. The buyer is not the individual developer — it's the **guardian of codebase integrity**.

- **Perplexity**: "Primary economic buyers: engineering directors / VP Eng / CTO + platform/DevEx owners in AI-forward orgs"
- **Gemini**: "Platform & DevEx Engineering Leads... overwhelmed by Code Churn"
- **Claude**: "VP/Director of Engineering at companies with 50+ developers who have adopted AI coding tools and are watching architectural coherence erode"

### Pain quantified

| Metric | Source |
|--------|--------|
| **29% trust** in AI-generated code | Claude (industry survey) |
| **41% increase in complexity** after AI adoption | Claude (Carnegie Mellon study) |
| **8x increase in duplicate code blocks** | Claude (GitClear) |
| **30% increase in static analysis warnings** after Cursor adoption | Claude (Carnegie Mellon) |
| **67% of developers spend more time debugging** AI code than before | Claude (Harness) |
| **25% more AI adoption = 7.2% less delivery stability** | Claude (Google DORA) |
| **53% cite code quality** as top barrier to AI coding adoption | Claude (OpsLevel survey) |
| **84-90% of developers** now use AI coding tools | Claude |

### Secondary: Senior/Staff engineers

Champions and daily power users, not economic buyers. They feel the pain of "AI slop" and architecture decay. They adopt CLI tools locally and champion internally. But $500/month is multi-order-of-magnitude above personal spend — they are the **land** part of land-and-expand.

### Anti-persona: AI-native agencies and indie hackers

Vibe coders focused on shipping MVPs in minutes. They view enforcement as friction. They generate massive technical debt but are not the immediate buyer until they scale (Gemini).

### Tertiary: AI-native platforms (embedding opportunity)

Lovable, Replit, Bolt.new — all lack architectural enforcement and face user complaints about "complexity ceilings" where generated apps collapse under their own weight (Claude). Potential OEM/embedding customers for the enforcement engine.

---

## Augment + keel

**Strongly validated as "context + enforcement" stack by all three sources.**

> "Augment gives your agent perfect recall. keel gives it perfect discipline. Use both."

### Why this composition works

- **Augment** = purely a retrieval system. Its MCP server exposes a `query_codebase` tool that returns relevant code. It cannot block, modify, or enforce anything.
- **keel** = enforcement layer. Constrains what the agent is allowed to do with that context.
- Both integrate via **MCP and hooks** into the same agents (Claude Code, Cursor, Gemini CLI, Windsurf)
- Augment's enterprise customer base ($252M raised, $977M valuation, Webflow/MongoDB customers) maps directly to keel's buyer persona
- The architecture is clean: **MCP servers extend what an agent can know; hooks constrain what an agent can do**

### Key risk

If Augment adds enforcement features to its own platform, the "complementary" narrative collapses. However, all three sources note Augment's strategic direction is firmly toward context/intelligence, not enforcement. Their Rules system is prompt-based, their Code Review is advisory. Nothing in their trajectory suggests deterministic enforcement.

---

## FSL License

**Accepted with caveats. All three sources agree FSL is pragmatically viable but not uncontroversial.**

### Reception status (Feb 2026)

- **10+ products** now use FSL: Sentry, GitButler, Codecov, Convex, PowerSync, Liquibase
- SPDX recognition gives institutional legitimacy
- "Fair Source" branding (fair.io) provides coherent identity distinct from the more aggressive BSL (HashiCorp/Redis)
- Enterprise compliance teams are accepting FSL because of clear "Competing Use" definition and fixed change date

### The two camps

| Camp | View |
|------|------|
| **Pragmatic developers/companies** | Accept it for direct use. Better than closed source. 2-year sunset is genuine. |
| **OSS purists/foundations** | Firmly reject. Apache, CNCF, similar foundations explicitly prohibit FSL dependencies. "Not open source." |

### The Liquibase precedent (Claude — critical warning)

When Liquibase adopted FSL in September 2025, it triggered blocking issues in Keycloak (CNCF), Apache Fineract, and Spring Boot — all of which cannot accept FSL dependencies under foundation governance. **If keel is a direct-use CLI tool, FSL is fine. If it ever needs to be embedded as a dependency in open source projects, FSL will create real friction.**

### Recommendation from sources

Use FSL. Be explicit it's "Fair Source, not open source." Ensure the product architecture works as a standalone tool rather than an embeddable library. This avoids the Liquibase trap.

---

## Pricing

**All three sources flag $500/month floor as too high for initial adoption.**

### Per-developer cost comparison

| Scenario | keel Cost | Per-Dev Equivalent | Competitors |
|----------|----------|-------------------|-------------|
| 3-person startup | $500/mo | **$167/dev/mo** | Greptile $30/dev, Snyk $25/dev, Semgrep $40/dev |
| 5-person team | $500/mo | **$100/dev/mo** | 2-4x more expensive than comparable tools |
| 10-person team | $500/mo | $50/dev/mo | Competitive with Sourcegraph ($49/dev) |
| 30-person team | $2,000/mo | $67/dev/mo | Well-calibrated for premium tooling |

### Converged recommendation

- **The $500/month floor creates significant friction** for the 3-5 person startups most likely to adopt a new tool early
- Every comparable tool offers either a free tier, a startup discount, or both
- Without a lower entry point, keel risks losing the bottom-up adoption motion that drives developer tool growth
- **Add a ~$200/month starter tier** for teams under 5, or a free tier for <5 devs (Claude), or 50% startup discount (Claude)
- The $2,000/month growth and custom enterprise tiers are well-calibrated and defensible

### Flat-rate model strengths

- Predictable budgeting for engineering managers
- No penalty for broad team adoption
- Simpler procurement than per-seat counting
- Positions as infrastructure (like Datadog) rather than tooling (like Jira)

---

## Category Naming

**"Structural Guardrails" may not resonate with buyers.** Claude notes that the market's own language is different:

- "Architectural drift" (what buyers complain about)
- "Comprehension debt" (the HN/blog term)
- "Guardrails as code" (the aspiration)
- "Code quality" and "architectural governance" (what buyers search for)

keel's messaging should meet buyers where they are, not force a new vocabulary. The category exists — the naming needs market validation.

---

## Blind Spots

All three sources identify areas the positioning doesn't adequately address:

### 1. Zero-config / integration fatigue
Developers are tired of configuring YAMLs. keel should work zero-config by reading existing `.cursorrules`, `tsconfig.json`, etc. and enforcing them strictly, rather than requiring a new proprietary rule syntax (Gemini).

### 2. Auto-correction, not just blocking
keel should not just block (exit 2) — it should guide the agent to fix the code automatically. The value prop shifts from "Blocking" to "Auto-Correction." Gemini's AfterAgent retry loop is the model to emulate (Gemini).

### 3. CLI-only risk
Many developers live in VS Code/Cursor/Windsurf, JetBrains, or cloud IDEs. keel likely needs first-class hook bundles for each tool, possibly thin IDE extensions, not just a standalone CLI (Perplexity).

### 4. Contract authoring UX
At scale, teams need: a library of prebuilt contracts, a reasonable DSL that architects can extend without writing regex/AST visitors, and tooling for false positives and contract evolution (Perplexity).

### 5. Telemetry / proving value
How does keel measure its own value? Error catch rate, false positive rate, token savings. Without self-measurement, it's hard to prove ROI (plan context).

### 6. Moat is thin
Hook systems are public, AST validation is well-understood. A well-funded competitor (Codacy, Qodo, even Augment) could build this in months. Defensibility lies in doing it first, doing it well, and building the contract definition language that becomes the standard (Claude).

---

## Disagreements Between Sources

| Topic | Perplexity | Gemini | Claude |
|-------|-----------|--------|--------|
| **Severity of gap** | "Largely real" — acknowledges Vertragssystem and Semgrep+hooks as credible partial solutions | "Real and critical" — strongest language, treats gap as binary | "Real but moat is narrow" — most cautious about timing and competitive response |
| **Codacy Guardrails** | Not mentioned | Noted as nearest analog with "real-time interception" | Not mentioned by name |
| **AWS Kiro** | Not mentioned | Brief mention as watchlist | Flagged as potential threat if AWS invests in deterministic enforcement |
| **Pricing floor** | Suggests lower entry tier or usage-based model | Explicitly says $500 is "too high" and recommends $200 or free tier for <5 devs | Recommends $200/month starter tier or 50% startup discount |
| **FSL risk level** | "Not universally accepted but normalized" | "Minimal controversy" — most optimistic | Flags Liquibase precedent as a concrete risk — most cautious |
| **Cursor v2.0 regression** | Mentions "regressions in some versions" | Details the specific `agent_message` regression | Does not mention |
| **Gemini AfterAgent retry** | Documents it but doesn't emphasize strategic value | Calls it "arguably superior to Claude's" and recommends keel exploit it | Does not mention |
| **GitHub Copilot hooks** | Mentions MCP policies briefly | Not mentioned | Flags as 5th hook system keel should support |
| **ArchCodex** | Not mentioned | Not mentioned | Flags as open-source predecessor solving the same problem — "validates market but represents free alternative" |
