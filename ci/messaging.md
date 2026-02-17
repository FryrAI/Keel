# Keel Messaging

```yaml
tagline: "The backbone your agents are missing."
secondary: "Structural integrity for AI-generated code."
cta: "Ship code that holds."
tone: Direct, technical, confident — never cute
```

## Taglines

### Primary Tagline

> **The backbone your agents are missing.**

Used in: hero section, social bios, meta descriptions, README header.

This line works because:
- "Backbone" = structural metaphor (keel = backbone of a ship)
- "Your agents" = speaks directly to the user's workflow
- "Missing" = identifies the gap without being alarmist

### Secondary Tagline

> **Structural integrity for AI-generated code.**

Used in: subheadlines, longer descriptions, PR copy where the primary is already used.

### Core Value Statement

> **keel prevents the agent from reading the wrong things and breaking the unseen things.**

Used in: problem framing, hero subline, pitch decks, README second paragraph.

This line works because:
- **Two failure modes in one sentence** — bad context in ("reading the wrong things") + invisible breakage out ("breaking the unseen things")
- "Prevents" = active, decisive — keel does something, not just flags
- "The agent" = singular, specific — speaks to the user's daily reality
- "Unseen things" = the core fear — downstream callers, type consumers, cross-module dependencies that no one checks until production

### Call to Action

> **Ship code that holds.**

Used in: CTA buttons, closing sections, email subjects. "Ship" carries the nautical double meaning (shipping code + ships have keels).

---

## Value Propositions

Three pillars, each with a headline, description, and proof point.

### 1. Real-Time Enforcement

**Headline:** "Catch violations before they compile."

**Description:** keel intercepts structural violations at generation time — not at review time, not at build time. When an agent removes a function that 12 callers depend on, keel stops it before the damage spreads.

**Proof point:** `<200ms` compile time for single-file incremental checks.

### 2. Universal Graph

**Headline:** "One graph. Every language."

**Description:** keel builds a structural graph of your codebase — functions, classes, modules, and their call relationships. TypeScript, Python, Go, Rust. Same schema, same enforcement, same commands.

**Proof point:** `<5s` full re-map for 100k LOC repositories.

### 3. Agent-Native Integration

**Headline:** "Built for the tools you're already using."

**Description:** keel integrates with 11 AI coding tools via CLI hooks, instruction files, and workflow configs. Claude Code, Cursor, Gemini CLI, Windsurf, Letta Code, Copilot, Aider, Codex CLI, Antigravity, GitHub Actions, and VS Code — all supported. `keel init` auto-detects your tools and generates configs.

**Proof point:** 11 tool integrations, zero runtime dependencies, single binary.

---

## Tone of Voice

### We Are

| Trait | Description | Example |
|-------|-------------|---------|
| **Direct** | Say what we mean. No hedging, no qualifiers. | "keel catches broken callers." not "keel can help identify potential issues with function references." |
| **Technical** | Speak to engineers as engineers. Use precise terms. | "Incremental compile in <200ms" not "Lightning-fast checking" |
| **Confident** | We know what we built. No apologies, no "just a tool." | "Ship code that holds." not "We hope this helps your workflow." |
| **Concise** | Every word earns its place. Cut the rest. | "4 languages. 11 tool integrations. Zero deps." not "We support a wide variety of programming languages and development tools." |

### We Are Not

| Anti-trait | Description | Example to avoid |
|-----------|-------------|------------------|
| **Cute** | No puns, no jokes, no playful tone. | "Let's sail!" / "Ahoy, developer!" |
| **Aggressive** | No FUD, no "your code is broken without us." | "Stop shipping garbage" |
| **Vague** | No marketing fluff, no unmeasurable claims. | "Supercharge your development" |
| **Apologetic** | No hedging with "might," "could," "try to." | "keel might help catch some issues" |

### Voice Examples

**Good:**
- "keel maps your codebase in under 5 seconds."
- "When an agent deletes a function, keel tells it who's calling."
- "`keel fix --apply` — auto-generates and applies fix plans for every violation."
- "Zero config. Zero runtime deps. One binary."
- "1,071 tests. 4 languages. Every edge case we could find."

**Bad:**
- "keel is a powerful tool that helps developers..." (vague, passive)
- "Supercharge your AI coding workflow!" (marketing fluff)
- "Navigate the treacherous waters of AI-generated code" (too nautical)
- "keel: Because your code deserves better" (sentimental)

### The Nautical Line

The nautical metaphor lives in the **brand** (name, logo, colors), not in the **copy**. We don't write pirate-speak. The ocean influence is visual and structural, not verbal. The only nautical word in our vocabulary is "keel" itself — and we use it as a product name, not a metaphor.

**Exception:** "Ship code that holds" is approved because "ship" is standard engineering vernacular (shipping code).

---

## Boilerplate Copy

### One-liner (for READMEs, social bios)
> keel — structural enforcement for AI-generated code.

### One-paragraph (for about pages, PR)
> keel builds a real-time structural graph of your codebase and enforces architectural contracts at generation time. When AI coding agents add, modify, or remove code, keel validates that the change doesn't break callers, violate type contracts, or damage the dependency graph — before the code is ever committed. It supports TypeScript, Python, Go, and Rust, integrates with 11 AI coding tools, and runs as a single binary with zero runtime dependencies.

### Three-sentence (for descriptions, meta)
> keel is a structural enforcement engine for AI-generated code. It builds a real-time graph of your codebase and catches architectural violations at generation time — not at review. One binary, zero dependencies, four languages, 1,071 tests.

---

## Pro Messaging

### Free vs Pro Positioning

> **Free is complete. Pro is multiplied.**

This is the anchor line. The free tier is not a trial, not a demo, not crippled. It's the full structural enforcement tool. Pro adds **team-scale visibility** — dashboards, naming governance, analytics, private hosting.

### Pro Value Props

| Feature | One-liner |
|---------|-----------|
| Naming conventions | "Define once, enforce everywhere. Your agents follow the same rules." |
| Team dashboard | "See what your agents build — module health, error trends, naming drift." |
| Detailed telemetry | "Know which modules break most and which agents fix fastest." |
| Prompt performance | "Measure first-compile success rate, fix latency, backpressure compliance." |
| Private hosting | "Your infrastructure, your data. Same dashboard, your network." |

### Pricing Copy

> **$29/user/month.** Every CLI command is free forever. Team adds the dashboard your engineering lead has been asking for.

### Upgrade CTAs

- "See what your team is building" → Team tier
- "Your infrastructure, your rules" → Enterprise tier
- Free → Team: "You've been enforcing locally. Now see the big picture."

---

## Competitor Positioning

We don't attack competitors by name. We position against categories:

| Category | Our Positioning |
|----------|----------------|
| Linters (ESLint, Ruff) | "Linters check style. keel checks structure." |
| Type checkers (tsc, mypy) | "Type checkers validate signatures. keel validates relationships." |
| Architecture tools (ArchUnit) | "Architecture tests run in CI. keel runs at generation time." |
| Code review | "Review catches what got past. keel prevents it from getting past." |
| Observability (Datadog, etc.) | "They monitor runtime. keel monitors generation time." |
