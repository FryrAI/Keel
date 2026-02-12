# Keel Landing Page Brief — keel.engineer

```yaml
sections: 9
style: dark-mode, terminal-forward
target: AI-augmented developers, engineering leads
goal: Install keel (curl command) or star the repo
```

> This document is a section-by-section wireframe description with copy direction. A designer or AI design tool (Stitch, Pencil) should be able to build from this alone.

---

## Section 1: Hero

**Layout:** Full-width, viewport height. Dark background (`--bg-abyss`).

**Content:**
- Top: Navigation bar — logo (mark + wordmark), links (Docs, GitHub, Pricing), CTA button ("Get Started")
- Center: Tagline in Display type (64px Geist Bold):
  > The backbone your agents are missing.
- Below tagline: Secondary line in Body L (18px Inter):
  > Structural enforcement for AI-generated code. One binary. Zero dependencies.
- Below secondary: Two buttons — "Get Started" (teal, primary) and "View on GitHub" (outlined, secondary)
- Below buttons: Terminal block showing a live `keel compile` interaction:

```
$ keel compile src/api/handlers.ts

  ERROR E001 broken_caller
  ├── src/api/handlers.ts:47 calls getUserById()
  ├── src/models/user.ts:12 — function was removed in this edit
  └── fix: restore getUserById() or update 3 callers

  ERROR E005 arity_mismatch
  ├── src/api/handlers.ts:89 calls createOrder(user, items)
  ├── src/models/order.ts:23 — signature is now createOrder(user, items, options)
  └── fix: add 'options' parameter to call at handlers.ts:89

  2 errors, 0 warnings — compile failed (47ms)
```

**Design notes:**
- Terminal block has `--bg-surface` background, 12px border-radius, subtle `--border-subtle` border
- Error keywords in `--error` red, file paths in `--text-muted`, fix hints in `--teal-500`
- Subtle fade-in animation on load (400ms)

---

## Section 2: Problem Statement

**Layout:** Centered text block, max-width 720px. Below: 3 cards in a row.

**Headline (h1):**
> AI agents don't understand your architecture.

**Subtext (Body L):**
> They generate code fast. They don't check what it breaks. Every removed function, every changed signature, every misplaced module — invisible until review, painful to fix.

**Three pain-point cards (side by side):**

| Card | Icon | Title | Description |
|------|------|-------|-------------|
| 1 | `x-circle` (red) | Broken Callers | Agent deletes a function. 12 callers break silently. You find out in CI — or production. |
| 2 | `warning` (coral) | Type Drift | Agent changes a return type. Downstream consumers expect the old shape. TypeScript compiles. Runtime crashes. |
| 3 | `shuffle` (gray) | Structure Rot | Agent adds utilities to the wrong module. Over 50 PRs, your architecture dissolves into a flat mess. |

**Design notes:**
- Cards use `--bg-surface` background, `--border-subtle` border, 8px radius
- Each card has a colored icon at top, title in h3, description in Body S
- Cards animate in with stagger (80ms offset)

---

## Section 3: How It Works

**Layout:** Overline label ("HOW IT WORKS"), h1, then 3-step horizontal flow.

**Headline (h1):**
> Three commands. Complete structural coverage.

**Three steps (connected by a thin horizontal line):**

### Step 1: Map
- **Number:** 01
- **Title:** Map your codebase
- **Command:** `keel map`
- **Description:** keel builds a structural graph — every function, class, module, and call edge. TypeScript, Python, Go, Rust. Under 5 seconds for 100k LOC.
- **Terminal preview:** `$ keel map` → `mapped 1,247 nodes, 3,891 edges (2.3s)`

### Step 2: Compile
- **Number:** 02
- **Title:** Compile your changes
- **Command:** `keel compile src/`
- **Description:** Incremental validation catches broken callers, arity mismatches, missing types, and placement violations. Under 200ms per file.
- **Terminal preview:** `$ keel compile src/api/` → `0 errors, 0 warnings — clean compile (142ms)`

### Step 3: Fix
- **Number:** 03
- **Title:** Fix with guidance
- **Command:** `keel explain E001 kXt9mRp2v4L`
- **Description:** Every violation includes a fix hint, confidence score, and resolution chain. Your agent reads it and self-corrects — no human in the loop.
- **Terminal preview:** `$ keel explain E001 kXt9mRp2v4L` → explanation with fix_hint

**Design notes:**
- Steps connected by thin `--border-subtle` line
- Active step (or hovered) highlights its terminal block with `--teal-500` border
- Step numbers in `--teal-500`, large (28px Geist SemiBold)

---

## Section 4: Integrations

**Layout:** Overline "INTEGRATIONS", h1, then a grid of tool logos.

**Headline (h1):**
> Works with the tools you already use.

**Integration grid (3 columns x 3 rows):**

| Tool | Tier Badge | Integration Method |
|------|------------|-------------------|
| Cursor | Enforced | MCP server |
| GitHub Copilot | Cooperative | VS Code extension |
| Claude Code | Enforced | MCP server |
| Aider | Cooperative | CLI hook |
| Windsurf | Cooperative | MCP server |
| Cline | Cooperative | MCP server |
| Continue | Cooperative | MCP server |
| Codex CLI | Cooperative | File watcher |
| VS Code | Native | Extension |

**Tier badges:**
- "Enforced" = `--teal-500` badge (keel can block the action)
- "Cooperative" = `--text-muted` badge (keel advises, tool decides)

**Design notes:**
- Each integration is a card with the tool's logo (or text name), tier badge, and integration method
- Logos in grayscale, colorize on hover
- Cards on `--bg-surface`, subtle hover lift (2px translateY)

---

## Section 5: Performance

**Layout:** Overline "PERFORMANCE", h1, then 4 big number cards.

**Headline (h1):**
> Fast enough to run on every keystroke.

**Four stat cards:**

| Metric | Value | Label | Context |
|--------|-------|-------|---------|
| Compile | `<200ms` | per file, incremental | "Faster than your test suite" |
| Map | `<5s` | full re-map, 100k LOC | "From cold start to full graph" |
| Discover | `<50ms` | adjacency lookup | "Instant context for any node" |
| Memory | `20-35MB` | runtime footprint | "Less than your language server" |

**Design notes:**
- Numbers in Display size (64px Geist Bold), `--teal-500` color
- Labels in Body S, `--text-muted`
- Numbers animate with count-up on scroll into view (1200ms, ease-out)
- Cards have no visible border — just the numbers floating on `--bg-abyss`

---

## Section 6: Developer Experience

**Layout:** Overline "DEVELOPER EXPERIENCE", h1, then side-by-side comparison.

**Headline (h1):**
> Output designed for agents and humans.

**Side-by-side:**
- **Left panel:** "JSON Output (for agents)" — shows `keel compile --format json` output
- **Right panel:** "Human Output (for you)" — shows `keel compile --format human` output

Both panels are terminal blocks with the same violation, formatted differently.

**Below comparison:** A row of 3 small feature highlights:
1. `--format llm` — Optimized for LLM context windows
2. `--batch-start/end` — Batch mode defers non-critical checks
3. `--format json | jq` — Machine-parseable for CI pipelines

---

## Section 7: Social Proof (By the Numbers)

**Layout:** Overline "PROVEN", h1, then 4 stat blocks in a row.

**Headline (h1):**
> Built with obsessive rigor.

**Stats:**

| Number | Label |
|--------|-------|
| 442+ | Tests passing |
| 4 | Languages supported |
| 9+ | Tool integrations |
| 0 | Runtime dependencies |

**Design notes:**
- Numbers in h1 size (48px), `--text-heading`
- Labels in Body S, `--text-muted`
- No cards — just clean numbers on the background
- Optional: "All tests" links to the test suite on GitHub

---

## Section 8: Pricing

**Layout:** Overline "PRICING", h1, then 3 pricing cards.

**Headline (h1):**
> Free to start. Free to stay.

**Three tiers:**

### Free (Open Source)
- **Price:** $0 / forever
- **For:** Individual developers, open-source projects
- **Includes:** All commands, all languages, single-repo, CLI only
- **CTA:** "Install Now"

### Team (Free)
- **Price:** $0 / during beta
- **For:** Teams shipping AI-generated code
- **Includes:** Everything in Free + multi-repo, `keel serve`, team dashboards
- **CTA:** "Join Beta"
- **Badge:** "Beta" in coral

### Enterprise
- **Price:** Contact us
- **For:** Organizations with compliance requirements
- **Includes:** Everything in Team + SSO, audit log, SLA, custom rules
- **CTA:** "Contact Sales"

**Design notes:**
- Middle card (Team) slightly elevated with `--teal-500` border to draw attention
- Enterprise card more subdued
- All cards on `--bg-surface` with `--border-subtle`

---

## Section 9: Final CTA

**Layout:** Full-width, centered, generous vertical padding (96px top/bottom).

**Headline (h1):**
> Start enforcing structure today.

**Install command (large terminal block):**

```bash
curl -fsSL keel.engineer/install.sh | sh
```

**Below command:** Secondary CTA — "Or install via Cargo: `cargo install keel`"

**Below that:** Footer with links (Docs, GitHub, License, Twitter/X) and copyright.

**Design notes:**
- The install command is the focal point — large, centered, with a "copy" button
- Terminal block uses `--bg-surface`, large padding (32px), 12px radius
- Footer is minimal — single line of links in `--text-muted`
