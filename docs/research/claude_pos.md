# Keel's category claim holds up — with caveats

**The core thesis is valid: no shipping product as of February 2026 deterministically enforces architectural contracts during LLM code generation.** Every tool in the ecosystem either provides context before generation (Augment, .cursorrules, AGENTS.md), catches problems after generation (Greptile, CodeRabbit, CI linters), or enforces operational security boundaries (Codex sandboxes, Gemini CLI policies). The specific gap — structural enforcement at write-time via hook interception — is real, documented, and increasingly painful as AI coding adoption hits **84–90% of developers** while trust has collapsed to **29%**. Two tools partially blur the boundary but don't close it: Codacy Guardrails enforces security/quality patterns during generation but has zero architectural awareness, and AWS Kiro provides architectural guidance through steering docs that remain LLM-mediated rather than deterministic. The positioning of "structural guardrails for code agents" describes a genuine, unoccupied category.

## The enforcement gap is real but the moat is narrow

The landscape decomposes cleanly into the categories keel's thesis describes. **Augment Code Context Engine** is explicitly read-only — its MCP server exposes a `query_codebase` tool that retrieves semantic context but cannot block, modify, or enforce anything. Cursor's `.cursorrules` and `.mdc` files are prompt-injected suggestions the LLM interprets but can ignore. Windsurf's rules are "living documents" that guide behavior through natural language, not formal constraints. Sourcegraph Amp reads AGENT.md for conventions with no enforcement mechanism. Letta Code focuses on persistent memory across sessions. Codex CLI has strong OS-level sandboxing (network isolation, command whitelisting) but zero code-structure awareness.

**Codacy Guardrails** deserves close attention as the nearest analog. It intercepts AI-generated code in real-time using static analysis (SAST, SCA) to catch security vulnerabilities, hardcoded secrets, and code smells — then auto-remediates before the code reaches the developer. This is genuine during-generation enforcement, but for a categorically different problem: individual code-level defects rather than system-level architectural contracts. It has no concept of module boundaries, interface signatures, or adjacency constraints. **Kiro** is the other watchlist item. Its spec-driven workflow and `.kiro/steering/` markdown files produce architectural warnings like "Architecture violation detected — Application layer should not import adapters." But the enforcement is the LLM reading natural language rules and self-correcting — if the LLM misinterprets the rule or generates violating code, there is no external formal checker to catch it.

The moat concern is timing. Hook systems are now shipping across all major agents (Claude Code since June 2025, Cursor v1.7 September 2025, Gemini CLI v0.26.0 January 2026, Windsurf v1.12.41 late 2025). The building blocks for someone else to build architectural enforcement are publicly available. Any well-resourced competitor could wire AST validation into hooks within weeks. The defensibility lies in doing it first, doing it well, and building the contract definition language that becomes the standard.

## The hook system landscape favors keel's architecture — mostly

The exit-code-2 blocking convention that keel relies on is supported by **three of the four target agents**: Claude Code, Gemini CLI, and Windsurf all treat exit code 2 as a deterministic block on pre-hooks. This is the critical mechanism — the agent proposes a file write, the hook runs a structural check, and exit code 2 prevents the write while feeding the error back to the LLM for self-correction.

**Claude Code** has the most mature hook system with **~13 event types** and three handler types (command, prompt, agent). PreToolUse hooks with exit code 2 block tool calls deterministically. **Gemini CLI** offers **11 events** with unique model-level hooks (BeforeModel, AfterModel, BeforeToolSelection) that no other agent provides, plus the ability to provide synthetic responses to skip LLM calls entirely. **Windsurf** provides **11 events** with clean pre/post symmetry; all pre-hooks support exit code 2 blocking. **Cursor**, however, uses JSON-based output decisions (`permission: "deny"`) rather than exit code 2 — keel will need a Cursor-specific adapter. GitHub Copilot also uses JSON-based decisions (`permissionDecision: "deny"`) with 5 hook events, representing an expansion opportunity.

**Codex CLI remains hookless.** GitHub issue #2109 is an open feature request, and discussion #2150 shows users explicitly comparing its lack of hooks to Claude Code's system. One user asked: "that doesn't react to exit code 2, does it?" This is a gap in keel's coverage — OpenAI's agent lacks the infrastructure. However, Codex CLI's sandbox-based security model may eventually evolve toward hooks given community pressure.

## The buyer exists and the pain is acute

The research uncovered a market that is not just receptive but actively building DIY solutions to the exact problem keel solves. A developer named Stefan van Egmond built **ArchCodex** — an open-source tool with a four-layer architecture (Boundaries, Constraints, Examples, Validation) that maps precisely to keel's value proposition. Another developer built **Giga AI** after "two years of frustration watching AI assistants constantly forget where files were located, create duplicates, and use completely incorrect patterns." These are not hypothetical users; they are people who have already invested time building fragments of what keel provides.

The quantitative evidence is striking. A Carnegie Mellon study found that after Cursor adoption, teams saw a **30% increase in static analysis warnings and 41% increase in complexity** after an initial 3–5x output spike. GitClear reported an **8x increase in duplicate code blocks**. Google's DORA research showed that **25% more AI adoption correlated with 7.2% less delivery stability**. Harness found **67% of developers spend more time debugging AI-generated code** than before. The OpsLevel survey of engineering leaders found code quality (53%) and maintainability (38%) as top barriers to AI coding adoption — behind only security (59%).

The primary buyer persona is the **VP/Director of Engineering at companies with 50+ developers** who have adopted AI coding tools and are watching architectural coherence erode. The secondary buyer is the **platform engineering team** responsible for standardizing tooling — these teams are already distributing `.cursorrules` and `AGENTS.md` files as "policy-as-prompt," which is precisely the manual workaround keel would formalize and enforce. The tertiary opportunity is embedding in **AI-native platforms** (Lovable, Replit, Bolt.new), all of which currently lack architectural enforcement and face user complaints about "complexity ceilings" where generated apps collapse under their own weight.

## "Augment for context, keel for enforcement" is credible positioning

Augment Code's Context Engine MCP is definitively a read-only semantic retrieval layer. It indexes codebases, understands cross-service relationships, and provides relevant context to LLMs — but it cannot block code changes, validate structure, or enforce anything. The MCP protocol was designed for exactly this kind of composability: specialized servers running in parallel, each handling a distinct capability. Anthropic itself promotes "server specialization" and "composable" MCP architectures. The architecture is clean: MCP servers extend what an agent can know, hooks constrain what an agent can do.

Augment has raised **$252 million** at a **$977 million valuation**, counts Webflow, MongoDB, and Tekion among customers, and was the first AI coding assistant with ISO/IEC 42001 certification. Its enterprise customer base — large teams with complex codebases who already care about code quality governance — maps directly to keel's buyer persona. The pricing model (credit-based, ~$50/month standard) means Augment users are already paying for AI coding infrastructure. Adding keel as a complementary enforcement layer is a natural upsell in a stack these teams are actively assembling.

The risk in this positioning is dependency on Augment's continued prominence. If Augment adds enforcement features to its own platform, the "complementary" narrative collapses. However, Augment's strategic direction is firmly toward being a context/intelligence layer, not an enforcement layer. Their Rules system is prompt-based, their Code Review is advisory. Nothing in their product trajectory suggests they are building deterministic enforcement.

## FSL licensing is pragmatically accepted but carries ecosystem risk

The Functional Source License has moved from controversial novelty (Sentry's launch in November 2023) to pragmatic middle ground, with **10+ products** now using it including Sentry, GitButler, Codecov, Convex, PowerSync, and Liquibase. The standardized non-compete clause (no customizable "Additional Use Grant" like BSL) and fixed 2-year sunset to Apache 2.0 or MIT have won grudging respect from pragmatic developers. Armin Ronacher (Sentry co-founder, Flask creator) calls it "incredibly close to what Open Source is all about, with some modest protections."

The developer community divides into two camps. **Open source purists and foundations** firmly reject FSL — Thierry Carrez (OSI VP) called it "proprietary gatekeeping wrapped in open-washed clothing," and Apache Software Foundation, CNCF, and similar foundations explicitly prohibit FSL-licensed dependencies. **Pragmatic developers and companies** generally accept it for direct use, viewing it as better than closed-source and adequate for non-competing use cases. The trend line favors acceptance: FSL's adopter base has grown steadily, the "Fair Source" branding (fair.io) provides a coherent identity, and SPDX recognition gives it institutional legitimacy.

The critical risk for keel is the **Liquibase precedent**. When Liquibase adopted FSL in September 2025, it triggered blocking issues in Keycloak (CNCF), Apache Fineract, and Spring Boot — all of which cannot accept FSL-licensed dependencies under their foundation governance policies. If keel is primarily a direct-use CLI tool (like Sentry), FSL is fine. If keel ever needs to be embedded as a dependency in open source projects, FSL will create real friction. The recommendation: use FSL, be explicit that it is "Fair Source, not open source," and ensure the product architecture works as a standalone tool rather than an embeddable library.

## Pricing needs recalibration at the low end

Keel's flat-rate model is unusual in a market dominated by per-seat pricing. At **5 developers**, the $500/month startup tier works out to $100/developer/month — **2–4x more expensive** than Snyk Team ($25/dev), Greptile ($30/dev), GitHub Code Security ($30/dev), Semgrep ($40/dev), or Sourcegraph ($49/dev). At **10 developers**, it drops to $50/developer/month, which is competitive with Sourcegraph and Snyk Ignite. The $2,000/month growth tier becomes competitive at **20–50 developers** ($40–100/dev/month), aligning with premium tooling.

The flat-rate model has genuine advantages: predictable budgeting, no penalty for broad team adoption, and simpler procurement. But the **$500/month floor creates significant friction** for the 3–5 person startup teams most likely to adopt a new tool early. Every comparable tool in this market offers either a free tier, a startup discount program, or both. Greptile offers 50% off for startups ($15/dev). Semgrep has a startup pricing program. Codacy's free tier covers IDE use. Without a lower entry point, keel risks losing the bottom-up adoption motion that drives developer tool growth.

The $2,000/month growth tier and custom enterprise pricing are well-calibrated. For a 30-person engineering team, $67/developer/month for architectural enforcement during AI code generation — a category with no alternatives — is defensible. Enterprise custom pricing is industry standard. The gap is at the bottom: consider a **$200/month starter tier** for teams under 5, or a 50% startup discount, to capture early adopters who will champion the tool internally.

## What's strongest, weakest, and the blind spots

**Strongest elements of the thesis:**

- The enforcement gap is empirically real — no shipping product does deterministic architectural enforcement during LLM code generation
- The pain is acute, quantified, and growing — trust at 29%, complexity up 41%, developers already building DIY solutions
- The hook infrastructure exists across four major agents and supports the exact blocking mechanism keel needs
- The "Augment for context, keel for enforcement" composability narrative is architecturally honest and targets a $977M-valuation company's customer base

**Weakest elements:**

- The moat is thin — hook systems are public, AST validation is well-understood, and a well-funded competitor (Codacy, Qodo, even Augment) could build this in months
- Cursor's JSON-based blocking (not exit code 2) and Codex CLI's complete lack of hooks mean keel cannot uniformly cover the market today
- Flat-rate pricing at $500/month creates an adoption barrier for the small teams most likely to try new tools first
- FSL may limit community contribution velocity compared to true open source

**Blind spots to watch:**

- **Qodo** positions as an "agentic code integrity platform" that enforces "coding standards, architecture rules, and compliance policies" — it may be closer to architectural enforcement than this research fully captured
- **ArchCodex** is an open-source predecessor solving the exact same problem with a similar architecture — it validates the market but represents a free alternative
- **Kiro's evolution** — if AWS invests in making steering-doc enforcement deterministic rather than LLM-mediated, it could close the gap with the distribution advantage of AWS behind it
- **GitHub Copilot hooks** are a fifth hook system not in the original thesis that keel should support — Copilot has massive market share
- The category name "Structural Guardrails for Code Agents" may not resonate with buyers who think in terms of "code quality" or "architectural governance" — the language of the market is "architectural drift," "comprehension debt," and "guardrails as code," and keel's messaging should meet buyers where they are