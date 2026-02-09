Here is a point‑by‑point synthesis, then an overall positioning read at the end.

---

## 1. Enforcement gap: does anyone already enforce type/signature/adjacency contracts in real time?

## What the named tools actually do

**Augment Code Context Engine MCP**

- Provides high‑quality semantic code retrieval via MCP, both local (Auggie CLI as MCP server) and remote (hosted Context Engine) for cross‑repo context.[augmentcode+3](https://www.augmentcode.com/changelog/context-engine-mcp-in-ga)
    
- The MCP exposes tools like `codebase-retrieval` that return highly relevant files/snippets; agents then use that to write better code.[augmentcode+1](https://www.augmentcode.com/blog/context-engine-mcp-now-live)
    
- The value prop is “context architecture” and quality gains (30–80% improvement in correctness/completeness) plus fewer tool calls and turns.[[augmentcode](https://www.augmentcode.com/blog/context-engine-mcp-now-live)]​
    
- There is no mention of hard enforcement of type contracts, function signatures, or architectural adjacency constraints during generation; it is about better retrieval, not guardrails.
    

**Cursor (1.7+ with Hooks)**

- Cursor 1.7 introduced a hooks system via `.cursor/hooks.json` to observe and control agent lifecycle: `beforeShellExecution`, `beforeMCPExecution`, `beforeReadFile`, `afterFileEdit`, `stop`, etc.[infoq+2](https://www.infoq.com/news/2025/10/cursor-hooks/)
    
- Hooks can inspect proposed shell commands, MCP calls, file edits, etc., and return JSON like `{ permission: "allow" | "deny" | "ask", ... }` or block via exit code 2 (equivalent to permission `"deny"`).[theaistack+2](https://www.theaistack.dev/p/cursor-introduces-hooks)
    
- Ecosystem partners (Runlayer, Corridor, Semgrep) now use hooks to inject security scanning and “real‑time feedback to the agent on code implementation and security design decisions” as code is written.[[cursor](https://cursor.com/blog/hooks-partners)]​
    
- This is powerful policy enforcement, but it is _pattern / tool / policy based_, not a built‑in notion of language‑level type/signature/adjacency contracts.
    

**Gemini CLI (v0.26.0 Hooks)**

- v0.26.0 adds hooks that run synchronously as part of the agent loop, configured in `.gemini/settings.json`.[reddit+2](https://www.reddit.com/r/GeminiCLI/comments/1qpoqf1/gemini_cli_weekly_update_v0260_skills_hooks_and/)
    
- Events include at least `BeforeAgent`, `AfterAgent`, `BeforeModel`, `BeforeTool`, `AfterTool`, etc., with JSON over stdin/stdout and strict formatting rules.[developers.googleblog+1](https://developers.googleblog.com/tailor-gemini-cli-to-your-workflow-with-hooks/)
    
- Exit code semantics:
    
    - `0` = success; stdout JSON is parsed and can modify/approve actions.
        
    - `2` = “System Block”: aborts the target action and uses `stderr` as the rejection reason.[[dev](https://dev.to/gioboa/hooks-are-here-now-you-can-intercept-and-direct-the-path-of-the-gemini-cli-oeh)]​
        
- Again: generic hook/policy mechanism; no first‑class contract system for program invariants or adjacency constraints.
    

**Windsurf / Cascade Hooks**

- Windsurf’s Cascade agent exposes “Cascade Hooks (Beta)” that run arbitrary scripts on events like `pre_read_code`, `post_read_code`, `pre_write_code`, `post_write_code`, `pre_run_command`, `post_run_command` via `.windsurf/hooks.json`.[zenn+1](https://zenn.dev/imohuke/articles/windsurf-cascade-hooks-general)
    
- Hooks receive JSON context; **pre‑hooks can block actions with exit code 2** (documented explicitly as validation/policy mechanism).[[docs.windsurf](https://docs.windsurf.com/windsurf/cascade/hooks)]​
    
- Intended uses: auto‑format after AI edits, notifications after commands, security/validation gates before writes or commands.[zenn+1](https://zenn.dev/imohuke/articles/windsurf-cascade-hooks-general)
    
- No notion of contract‑aware generation; it is a generic hook layer.
    

**Sourcegraph Amp**

- Amp is an agentic coding tool focused on “high‑quality outcomes at enterprise scale”, using Sourcegraph’s code graph to give LLMs rich semantic context and whole‑codebase visibility.[amplifilabs+1](https://www.amplifilabs.com/post/sourcegraph-amp-agent-accelerating-code-intelligence-for-ai-driven-development)
    
- It exposes context (symbols, references, dependency graphs) and tools for refactoring, but documentation and reviews describe it as a context/intelligence layer, not as an enforcement engine.[reddit+2](https://www.reddit.com/r/cursor/comments/1kpin6e/tried_amp_sourcegraphs_new_ai_coding_agent_heres/)
    
- There is no public mention of type‑contract enforcement or adjacency constraints baked directly into its generation loop.
    

**Letta Code**

- Letta Code is a terminal‑based, memory‑first coding agent on top of the Letta API.[github+3](https://github.com/letta-ai/letta-code)
    
- It ships:
    
    - A **hook system** with events like `PermissionRequest`, `UserPromptSubmit`, `Notification`, `Stop`, `SubagentStop`, `PreCompact`, `Setup`, `SessionStart`, `SessionEnd`, `PreToolUse`, `PostToolUse`.[letta+1](https://docs.letta.com/letta-code/hooks/)
        
    - Command hooks (shell scripts) and prompt hooks (LLM evaluation) that can **block with exit code 2** (“Blocking error – action stopped, stderr shown to agent”).[letta+1](https://docs.letta.com/letta-code/hooks)
        
    - A permissions system and human‑in‑the‑loop model where tools can be gated or pre‑approved.[letta+1](https://docs.letta.com/guides/core-concepts/tools/human-in-the-loop)
        
- This is very close to Claude/Cursor/Gemini hook semantics. It gives you the primitives to implement enforcement, but Letta itself doesn’t ship language‑level type/signature contract enforcement.
    

**Codex CLI**

- Official docs emphasize approvals, sandboxing, MCP integration, and `exec` for scripted, non‑interactive use.[developers.openai+2](https://developers.openai.com/codex/cli/reference/)
    
- There is **no hooks system** in the Codex CLI as of early 2026. A Reddit thread explicitly notes: “Codex currently lacks hook support… my initial look suggested environment hooks, but a deeper review shows that’s not the case.”[[reddit](https://www.reddit.com/r/codex/comments/1p6lzqg/autoload_skills_with_userpromptsubmit_hook_in/)]​
    
- Another thread asking about a hook system or `UserPromptSubmit`‑style hooks concludes that Codex “doesn’t seem to know about a ~/.codex/hooks folder” and has no hook triggers; the author walks back an earlier assumption that such hooks exist.[[reddit](https://www.reddit.com/r/codex/comments/1p6lzqg/autoload_skills_with_userpromptsubmit_hook_in/)]​
    
- So Codex has approvals and sandbox modes, but not extensible, scriptable enforcement the way Claude/Cursor/Gemini/Windsurf/Letta do.
    

**MCP servers and policy tools**

- There is an emerging class of MCP servers that handle **policy enforcement at the tool/endpoint level**, e.g.:
    
    - FAST MCP Policy Enforcement server exposing `enforce_policy` to validate requested actions against configurable rules.[mcpmarket+1](https://mcpmarket.com/zh/server/policy)
        
    - MXCP’s policy system, using CEL expressions to `deny` or filter fields on input/output of tools, including fine‑grained authorization and sensitive‑field masking.[[mxcp](https://mxcp.dev/docs/features/policies/)]​
        
    - Security‑scanner MCP servers that scan content and block operations or flag vulnerabilities.[github+1](https://github.com/jonsoku-dev/mcp-security-scanner)
        
- These enforce _tool‑level_ and _data‑level_ policies—not language type/signature contracts within code generation.
    

## Existing “contract” systems on top of hooks

The closest thing to what you’re describing is a **contract layer built on top of Claude Code hooks**:

- A public article describes a **“Vertragssystem” (contract system)** for Claude Code: 16 contracts with ~142 rules that validate AI‑generated code before it is written. Rules cover PHP security, SQL security, HTML tables, architecture gates, YAML safety, etc.[[karlkratz](https://karlkratz.de/contracts-rules-claude-code)]​
    
- Mechanism:
    
    - A `PreToolUse` hook for edit/write tools scans proposed changes against regex and structural patterns.
        
    - Rules can be `forbidden` (pattern may not appear), `required` (must appear), or `warning` (non‑blocking).
        
    - Severity levels: `Critical` → block action; `Major`/`Minor`/`Info` → warnings only.[[karlkratz](https://karlkratz.de/contracts-rules-claude-code)]​
        
    - On a critical violation (e.g. `eval($userInput)`), the hook blocks the write and returns a detailed message; Claude receives the stderr feedback and regenerates code to comply.[[karlkratz](https://karlkratz.de/contracts-rules-claude-code)]​
        
- Some contracts explicitly target architecture (e.g., “no direct repos in controllers”, “no domain infra dependency”, `strict_types` required) and language idioms.[[karlkratz](https://karlkratz.de/contracts-rules-claude-code)]​
    
- This _is_ real‑time enforcement of **pattern‑based contracts** during LLM code generation, but:
    
    - It’s built as a third‑party hook stack, not a core Claude feature.
        
    - It mostly matches on textual/structural patterns, not a full type system or AST‑level type‑contract solver across languages.
        

## Are type/signature/adjacency contracts enforced by any mainstream tool?

From available public sources as of Feb 2026:

- Vendors are **standardizing around hooks** plus:
    
    - Tool/command gating (PreTool/PreShell hooks),
        
    - Security scanning (Semgrep, secret scanners, SAST tools),
        
    - Policy enforcement for MCP and external APIs,
        
    - Pattern‑based contract systems like the Claude contract framework above.[thedocumentation+4](https://thedocumentation.org/claude-code-development-kit/hooks/security_scanner/)
        
- None of the named tools (Augment, Cursor, Gemini CLI, Windsurf/Antigravity, Sourcegraph Amp, Letta Code, Codex CLI) advertises:
    
    - First‑class **type contract** enforcement (e.g., verifying language type invariants at generation time across a project),
        
    - **Signature contracts** beyond what static analysis / compilers already catch post‑generation,
        
    - **Adjacency constraints** in the sense of WFC‑style or graph adjacency constraints on code structure.
        

So the **“enforcement gap” is largely real**:

- Core tools ship context (Augment, Amp) and hook primitives (Claude, Cursor, Gemini, Windsurf, Letta).
    
- Enforcement of architectural, security, and style rules is either:
    
    - Done **after the fact** in CI/PR (Snyk, CodeQL, Semgrep, Sonar, etc.), or
        
    - Implemented via **bespoke hook scripts** and pattern scanners—no widely‑adopted, vendor‑supported contract DSL for per‑team invariants during generation.
        

You are _not_ totally alone—there are credible third‑party experiments in contract‑style enforcement using Claude hooks and Cursor+Semgrep. But there is still a large gap between raw hooks and a general‑purpose, language‑aware enforcement product focused on type/signature/adjacency contracts.[cursor+1](https://cursor.com/blog/hooks-partners)

---

## 2. Hook system audit (Claude, Cursor, Gemini CLI, Windsurf, Codex, “others”)

## Claude Code

**Event types**

The official hooks documentation (including the Chinese mirror) lists 10 event types:[code.claude+2](https://code.claude.com/docs/en/hooks)

- `PreToolUse` – before a tool executes
    
- `PostToolUse` – after a tool completes
    
- `Stop` – when Claude finishes a response
    
- `SubagentStop` – when a subagent completes
    
- `Notification` – when a desktop notification fires
    
- `UserPromptSubmit` – before processing user input
    
- `PermissionRequest` – when a permission dialog appears
    
- `PreCompact` – before context compaction
    
- `SessionStart` – when a session begins
    
- `SessionEnd` – when a session ends
    

**Handler types**

- **Command hooks**: run shell commands, configured with `"type": "command", "command": "…"`.[claudecn+1](https://claudecn.com/en/docs/claude-code/advanced/hooks/)
    
- **Prompt hooks**: send structured input to an LLM, configured with `"type": "prompt", "prompt": "…"`, optionally specifying a model.[code.claude+1](https://code.claude.com/docs/en/hooks)
    
- Both types receive JSON on stdin and can influence whether an action proceeds, how it’s modified, or whether additional questions are asked.
    

**Exit code 2 behavior**

- Official docs: exit code 2 is “block tool/stop operation”, with other non‑zero codes treated as “error logged, continue”.[claudecn+1](https://claudecn.com/en/docs/claude-code/advanced/hooks/)
    
- Eesel’s guide and community posts confirm that **exit code 2 is the canonical blocking signal**; for blocking events like `PreToolUse`, it prevents execution and passes stderr back to Claude as feedback.[eesel+2](https://www.eesel.ai/blog/hooks-reference-claude-code)
    
- More advanced patterns use exit code 0 plus JSON output to make allow/deny decisions programmatically (e.g., implementing richer policy logic without using exit 2).[code.claude+1](https://code.claude.com/docs/en/hooks-guide)
    

## Cursor (v1.7+)

**Confirmation of hooks in v1.7**

- Cursor 1.7 (released late Sept/Oct 2025) introduces Hooks “to apply custom logic to your Agent’s behavior and output” via a `hooks.json` configuration.[theaistack+4](https://www.theaistack.dev/p/cursor-introduces-hooks)
    
- Hooks exist at both user and project scope (`~/.cursor/hooks.json` and `.cursor/hooks.json`).[cupcake.eqtylab+2](https://cupcake.eqtylab.io/reference/harnesses/cursor/)
    

**Event types**

From Cursor docs and third‑party references:[infoq+2](https://www.infoq.com/news/2025/10/cursor-hooks/)

- Before‑actions (can block):
    
    - `beforeShellExecution`
        
    - `beforeMCPExecution`
        
    - `beforeReadFile`
        
    - `beforeSubmitPrompt`
        
- After‑actions (fire‑and‑forget):
    
    - `afterShellExecution`
        
    - `afterMCPExecution`
        
    - `afterFileEdit`
        
    - `afterAgentResponse`
        
    - `afterAgentThought`
        
- Lifecycle:
    
    - `stop` (allows e.g. follow‑up messaging before ending a run)
        

**Handlers and exit behavior**

- Hooks run arbitrary commands; they receive a JSON payload on stdin, and respond with JSON (e.g. `permission: "allow" | "deny" | "ask"`, `continue: true/false`, `userMessage`, `agentMessage`).[theaistack+3](https://www.theaistack.dev/p/cursor-introduces-hooks)
    
- Exit codes:
    
    - Docs: **exit code 2** explicitly means “Block the action (equivalent to returning permission: 'deny')”; other non‑zero exit codes are treated as hook failure and the action proceeds (fail‑open by default).[[cursor](https://cursor.com/docs/agent/hooks)]​
        
    - Community bug reports confirm that “deny” and equivalent blocking semantics work; “allow/ask” have had regressions in some versions but conceptually exist and are respected when implemented correctly.[cursor+2](https://forum.cursor.com/t/regression-hook-response-fields-usermessage-agentmessage-ignored-in-v2-0-64/141516)
        

So: **yes, exit code 2 blocks actions in Cursor**; it’s a thin wrapper around JSON with `permission: "deny"`.

## Gemini CLI (v0.26.0+)

**Hooks shipped in v0.26.0**

- Release notes and Google’s developer blog confirm that **hooks are enabled by default in Gemini CLI v0.26.0+**, giving users “full control to customize the agentic loop”.[geminicli+2](https://geminicli.com/docs/hooks/)
    

**Event types**

From the blog and docs:[developers.googleblog+1](https://developers.googleblog.com/tailor-gemini-cli-to-your-workflow-with-hooks/)

- High‑level lifecycle:
    
    - `BeforeAgent` – after user submits prompt, before planning.
        
    - `AfterAgent` – when the agent loop ends (can force retry or halt).
        
- Model‑level:
    
    - `BeforeModel` – before sending a request to the LLM (can modify prompts, swap models, mock responses).
        
- Tool‑level:
    
    - `BeforeTool` – before calling a tool (e.g., `write_file`, `replace`) – can validate arguments, block, or augment.
        
    - `AfterTool` – after a tool finishes (for logging, post‑processing, etc.).
        
- Hooks are matchable via regex (`matcher`) on tool names or lifecycle events.[[developers.googleblog](https://developers.googleblog.com/tailor-gemini-cli-to-your-workflow-with-hooks/)]​
    

**Exit code 2 behavior**

- Docs and blog posts state:
    
    - Exit `0` → success, stdout JSON parsed.
        
    - Exit `2` → **“System Block”**: aborts the target action and uses stderr as the rejection reason.[[dev](https://dev.to/gioboa/hooks-are-here-now-you-can-intercept-and-direct-the-path-of-the-gemini-cli-oeh)]​
        
- “Golden rule”: only print final JSON to stdout; use stderr for debug logs and for human‑readable rejection reasons on exit 2.[[dev](https://dev.to/gioboa/hooks-are-here-now-you-can-intercept-and-direct-the-path-of-the-gemini-cli-oeh)]​
    

So **exit code 2 definitively blocks actions** for the relevant events.

## Windsurf / Cascade Hooks

**Cascade Hooks confirmation**

- Official and community docs describe **Cascade Hooks** as an advanced but somewhat “hidden” feature of Windsurf’s Cascade agent, configured via `.windsurf/hooks.json` with scripts under `.windsurf/hooks/`.[digitalapplied+3](https://www.digitalapplied.com/blog/cursor-vs-windsurf-vs-google-antigravity-ai-ide-comparison-2026)
    

**Event types**

Main events documented:[windsurf+2](https://docs.windsurf.com/zh/windsurf/cascade/hooks)

- `pre_read_code` / `post_read_code` – when the agent reads files.
    
- `pre_write_code` / `post_write_code` – when the agent edits code.
    
- `pre_run_command` / `post_run_command` – when the agent runs terminal commands.
    

**Exit code 2 behavior**

- Windsurf’s docs explicitly state:
    
    - Hooks receive JSON describing the action.
        
    - For **pre‑hooks**, exit code 2 **blocks the action**, making them suitable for security or validation policies (e.g., forbidding certain commands or file writes).[[docs.windsurf](https://docs.windsurf.com/windsurf/cascade/hooks)]​
        
- Performance guidance: hooks run synchronously and should be fast; pre‑hooks in particular must avoid noticeable latency.[zenn+1](https://zenn.dev/imohuke/articles/windsurf-cascade-hooks-general)
    

So yes, **pre‑hooks use exit code 2 as a blocking mechanism**.

## Codex CLI

- There is **no documented hooks system**: official Codex CLI docs describe approvals, sandbox modes, MCP integration, and `exec`, but nothing about hook events or a hooks config directory.[developers.openai+2](https://developers.openai.com/codex/cli/features/)
    
- Community threads from late 2025 / early 2026 explicitly note that:
    
    - “Codex currently lacks hook support,” despite some initial confusion.[[reddit](https://www.reddit.com/r/codex/comments/1p6lzqg/autoload_skills_with_userpromptsubmit_hook_in/)]​
        
    - Users trying to emulate Claude’s `UserPromptSubmit` hooks find that Codex knows nothing about `~/.codex/hooks` or `.on_user_prompt_submit` patterns.[[reddit](https://www.reddit.com/r/codex/comments/1p6lzqg/autoload_skills_with_userpromptsubmit_hook_in/)]​
        
- Some workflows use Codex CLI from _outside_ (e.g., via Claude Code skills) but that is not a hook system inside Codex itself.[[claude-plugins](https://claude-plugins.dev/skills/@Lucklyric/cc-dev-tools/codex)]​
    

So the picture remains: **Codex CLI still has no blocking hooks** as of Feb 2026, and lack of hook support is itself a common complaint.[reddit+1](https://www.reddit.com/r/OpenAI/comments/1qvnwwg/hooks_system_for_codex_cli_looking_for/)

## Other notable hook systems you might care about

You are _not_ missing major players—your mental list is basically complete—but a few worth noting:

- **Letta Code hooks**: as above, multiple lifecycle events, command/prompt hooks, exit code 2 as blocking.[letta+1](https://docs.letta.com/letta-code/hooks/)
    
- **Claude Code “Hook: MCP Security Scanner”** and similar templates, using `PreToolUse` with matcher `mcp__.*` to scan MCP calls for secrets and block them via exit code 2.[[thedocumentation](https://thedocumentation.org/claude-code-development-kit/hooks/security_scanner/)]​
    
- **Gemini CLI extensions with hooks**: extensions can bundle hooks (e.g. the “Ralph loop” extension intercepts `AfterAgent` to force continuous iterative loops, and other extensions use `BeforeTool` hooks for security policies).[[developers.googleblog](https://developers.googleblog.com/tailor-gemini-cli-to-your-workflow-with-hooks/)]​
    
- **GitHub Copilot MCP policies**: Copilot enforces MCP server allowlists and registry policies (not a scriptable hook system, but a governance layer over MCP usage).[github+1](https://docs.github.com/en/copilot/reference/mcp-allowlist-enforcement)
    

But in terms of **scriptable, synchronous, exit‑code‑based hook systems** for agentic coding, the main “club” is: Claude Code, Cursor, Gemini CLI, Windsurf, Letta Code. Codex is the outlier with no hooks.

---

## 3. Buyer persona & demand for enforcement

## Who is actually complaining about AI‑driven code decay?

The public discourse in late 2024–early 2026 is dominated by **senior/experienced developers, tech leads, and architects** worrying about:

- **“AI slop” and code review overload** – r/ExperiencedDevs threads describe AI‑generated code as “quite common” but often poor enough that reviewers struggle to keep up; people worry that the volume of low‑quality code “outstrips our ability to evaluate it”.[reddit+1](https://www.reddit.com/r/ExperiencedDevs/comments/1nx21xo/how_to_maintain_code_quality_with_ai_slop/)
    
- **Maintainability and technical debt** – multiple posts argue that AI code can be harder to change and adapt, even if it works initially, with more duplication and one‑off patterns.[reddit+5](https://www.reddit.com/r/ChatGPTCoding/comments/1jsw8sq/ai_generated_code_maintainability/)
    
- **Architecture/entropy concerns** – blog posts and newsletters describe:
    
    - “Architecture Decay” as AI introduces generic, model‑driven patterns that erode bespoke architectures over time.[[newsletter.theseriouscto](https://newsletter.theseriouscto.com/p/ai-coding-assistants)]​
        
    - “Every AI‑generated feature adds architectural entropy,” as systems drift from cohesive design to a patchwork of local changes.[pauleasterbrooks+2](https://pauleasterbrooks.com/articles/technology/software-entropy)
        
    - LinkedIn posts about “code entropy rising with AI‑assisted development” where codebases change faster than humans can keep a mental model, explicitly calling out the need for new tooling to keep architectural understanding intact.[linkedin+1](https://www.linkedin.com/posts/marc-hanson-579469a_softwaredevelopment-aitools-technicalleadership-activity-7420200637105278976-_9wn)
        
- **AI as “junior dev” without long‑term learning** – HN and blogs push back on the “LLM = junior dev” metaphor but consistently frame LLMs as **very fast, context‑amnesic implementers** that need strong guardrails and deterministic checks.[news.ycombinator+3](https://news.ycombinator.com/item?id=46577503)
    

There is also a parallel stream of **optimistic but cautious** commentary: “with enough deterministic guardrails, AI coding can produce production‑ready code; the challenge is putting those guardrails in place.”[sonarsource+2](https://www.sonarsource.com/blog/seven-habits-of-highly-effective-ai-coding/)

## Implicit buyers from current tools

Looking at tools that already sell into this space:

- **Greptile** markets AI code review and custom rules at **$30/dev/month** with clear messaging toward **CTOs and engineering leaders** (“Tell your CTO about Greptile”).[greptile+2](https://www.greptile.com/pricing)
    
- **Sourcegraph Cody and Amp** target **engineering teams and enterprises** with per‑user enterprise pricing ($49–59/user/month) and “Talk to sales” flows.[sourcegraph+3](https://sourcegraph.com/amp)
    
- **Snyk, Semgrep, CodeQL, SonarSource**: while not AI‑specific, their buyers are typically:
    
    - AppSec / security teams,
        
    - Platform/DevEx groups integrating security and quality checks,
        
    - VP Eng / CTO for larger rollouts.[spendflo+2](https://www.spendflo.com/blog/snyk-pricing-plans-features)
        
- Cursor’s hooks partners (Runlayer, Corridor, Semgrep) are explicitly targeting **security and platform teams** who want central policy enforcement through hooks.[[cursor](https://cursor.com/blog/hooks-partners)]​
    
- MXCP/MCP policy servers also clearly target **enterprise platform/security** functions that want policy as code around AI tool access.[github+3](https://github.blog/changelog/2025-11-18-internal-mcp-registry-and-allowlist-controls-for-vs-code-stable-in-public-preview/)
    

## Mapping to your proposed personas

Your candidate buyers line up with what the ecosystem already shows:

1. **Engineering leads at companies using AI coding tools heavily**
    
    - These are the people posting “state of AI code quality” threads, longform “how to work with AI agents” essays, and architecture/entropy articles.[nmn+3](https://nmn.gl/blog/ai-code-review)
        
    - They control team standards and can justify paying for a guardrail tool if it keeps velocity while preventing architecture debt and security incidents.
        
2. **Platform / DevEx teams standardizing AI coding tooling**
    
    - This matches:
        
        - The enterprise side of Sourcegraph, Snyk, MXCP, and MCP governance tooling.[sourcegraph+3](https://sourcegraph.com/pricing)
            
        - Cursor’s security/platform‑oriented hook partners.[[cursor](https://cursor.com/blog/hooks-partners)]​
            
    - These teams have the mandate and ability to roll out a CLI or hook‑based layer across multiple agents (Claude, Cursor, Gemini, Letta, etc.).
        
3. **Individual senior developers frustrated by LLM code quality**
    
    - Reddit’s r/ExperiencedDevs, r/automation, and r/ChatGPTCoding are full of seniors and staff‑level devs complaining about AI code quality and maintainability, but they tend to be **influencers/champions, not buyers**.[loufranco+7](https://loufranco.com/blog/category/code-reviews)
        
    - They will adopt a CLI locally and champion it, but a $500/month price point is multi‑order‑of‑magnitude higher than their typical personal spend. They are the **land** part of a land‑and‑expand, not the economic decision makers.
        
4. **AI‑native development companies (Lovable, Replit, Bolt, etc.)**
    
    - Public pieces on AI‑native dev (agents that implement full tasks) increasingly emphasize the need for **governance, testing, and policy enforcement**; some use agents in CI to enforce architecture and standards.[davepatten.substack+2](https://davepatten.substack.com/p/using-ai-agents-to-enforce-architectural)
        
    - These teams often roll their own enforcement, but as the Claude contract system shows, they are willing to adopt external frameworks if it saves time and extends across agents.[thedocumentation+2](https://thedocumentation.org/claude-code-development-kit/hooks/security_scanner/)
        
    - For AI‑native shops, **embedding enforcement** is a product differentiator (safer, more reliable agents). They are plausible early design partners or OEM users of an enforcement engine.
        

So the persona stack you propose is consistent with the evidence, with a nuance:

- **Primary economic buyers**: engineering directors / VP Eng / CTO + platform/DevEx owners in AI‑forward orgs.
    
- **Key champions and daily power users**: senior/staff developers and AI‑native teams who feel the pain of “AI slop” and architecture decay in their daily workflow.
    

---

## 4. Augment Code Context Engine MCP + keel: is the composition credible?

## What Augment Context Engine MCP actually does

From Augment’s launch posts and docs:[augmentcode+3](https://docs.augmentcode.com/context-services/mcp/overview)[[youtube](https://www.youtube.com/watch?v=w8LgDJoi8-E)]​

- **Purpose**: Provide “industry‑leading semantic search” and “context architecture” for AI coding agents.
    
- **Integration surface**: entirely via MCP:
    
    - Local server mode: run Auggie CLI as an MCP server (stdio) bound to your working directory; indexes and updates in real time.[[docs.augmentcode](https://docs.augmentcode.com/context-services/mcp/overview)]​
        
    - Remote mode: connect to Augment’s hosted MCP (`https://api.augmentcode.com/mcp`) for cross‑repo and GitHub‑backed context.[augmentcode+1](https://www.augmentcode.com/blog/context-engine-mcp-now-live)
        
- **Capabilities**:
    
    - Semantic search over code, dependencies, documentation, and multi‑repo code graphs.
        
    - Precise context selection — “surface exactly what’s relevant to the task, nothing more”.[[augmentcode](https://www.augmentcode.com/blog/context-engine-mcp-now-live)]​
        
    - Significant quality uplift across agents:
        
        - Cursor + Claude Opus 4.5: ~71% improvement across completeness/correctness metrics.[augmentcode+1](https://www.augmentcode.com/changelog/context-engine-mcp-in-ga)
            
        - Claude Code + Opus 4.5: ~80% improvement.
            
        - Cursor + weaker models: smaller but still meaningful gains.[[augmentcode](https://www.augmentcode.com/blog/context-engine-mcp-now-live)]​
            
    - Faster and cheaper, not just better: fewer turns, fewer tool calls, reduced token usage because the agent doesn’t have to “grep around blindly”.[augmentcode+1](https://www.augmentcode.com/product/context-engine-mcp)
        

The messaging is extremely consistent: **context is the problem**; Augment solves context; it does _not_ attempt to solve governance or enforcement.

## Does Augment do any enforcement itself?

From their docs and launch material:

- No mention of:
    
    - Pre‑ or post‑write validation,
        
    - Blocking actions based on rules,
        
    - Contract DSLs,
        
    - Architecture or policy enforcement.
        
- Everything is phrased in terms of **semantic retrieval, indexing, and context routing**.[augmentcode+3](https://www.augmentcode.com/product/context-engine-mcp)
    

Augment is explicitly a **context engine**. Enforcement is left up to:

- The agent (Claude Code, Cursor, etc.),
    
- Hook systems (which you’re targeting),
    
- Downstream scanners (Semgrep, Snyk, etc.).
    

## Is “Augment for context + keel for enforcement” a credible stack?

Technically and product‑wise, yes:

- Both Augment and your proposed tool rely on:
    
    - **MCP and hooks** as primary integration points.
        
    - The same agents (Claude Code, Cursor, Windsurf, Gemini CLI, Letta).
        
- Augment specialises in:
    
    - Selecting _what the agent sees_ (semantically relevant code, patterns, docs).
        
- keel (enforcement) would specialise in:
    
    - Controlling _what the agent is allowed to do_ with that context—enforcing per‑team contracts on types, interfaces, architecture, security, etc.
        

For an Augment user:

- Pain already acknowledged: even with better context, AI can still:
    
    - Introduce violations of existing patterns,
        
    - Drift architecture over time,
        
    - Repeat security/quality mistakes that aren’t captured in context alone.[augmentcode+2](https://www.augmentcode.com/guides/ai-coding-agents-vs-autocomplete-6-key-architecture-gaps)
        
- Hooks and CI‑level tools today partially address this, but they require heavy custom scripting and are primarily security‑ or style‑focused.
    
- An enforcement engine that:
    
    - Works across Claude/Cursor/Gemini/Windsurf/Letta,
        
    - Is **MCP‑compatible**,
        
    - Can reason about type/signature/adjacency contracts,
        
    - Exposes a declarative configuration similar in spirit to Augment’s connectors,  
        would feel like a natural **complement**: “Augment gives the agent the right context; keel ensures the resulting changes honor our contracts.”
        

So “Augment for context + keel for enforcement” is not just marketing cute; it maps onto the current ecosystem separation: **context vs control**.

---

## 5. FSL license reception (2‑year Apache/MIT sunset)

## Sentry’s perspective

- Sentry introduced the **Functional Source License (FSL)** as an evolution of the Business Source License (BUSL), aiming for “freedom without harmful free‑riding”.[sentry+2](https://blog.sentry.io/introducing-the-functional-source-license-freedom-without-free-riding/)
    
- Key properties:
    
    - Source‑available from day one for non‑competing uses.
        
    - Converts automatically to Apache 2.0 or MIT after **two years**.[heathermeeker+3](https://heathermeeker.com/2023/11/18/sentry-launches-functional-source-license-a-new-twist-on-delayed-open-source-release/)
        
    - Non‑compete clause aimed at preventing hosted clones that economically undermine the original vendor.[infoq+2](https://www.infoq.com/news/2023/12/functional-source-license/)
        
- Sentry claims:
    
    - BUSL’s 4‑year default is too long; FSL’s 2‑year window is a more honest “head start”, not pseudo‑open source.[news.ycombinator+1](https://news.ycombinator.com/item?id=38331173)
        
    - FSL’s more opinionated, less configurable approach makes it easier for compliance and for other companies to adopt.[sentry+1](https://blog.sentry.io/introducing-the-functional-source-license-freedom-without-free-riding/)
        

## Community reception

From HN discussions and commentary around both Sentry and GitButler’s adoption of FSL:[news.ycombinator+6](https://news.ycombinator.com/item?id=41184037)

- **Positive / pragmatic camp**:
    
    - Sees FSL as a **meaningful improvement** over BUSL and ad‑hoc “source‑available” licenses.
        
    - Appreciates the **fixed 2‑year Apache/MIT sunset**, contrasting it favorably with BUSL’s 4‑year horizon.[heathermeeker+2](https://heathermeeker.com/2023/11/18/sentry-launches-functional-source-license-a-new-twist-on-delayed-open-source-release/)
        
    - Views it as a realistic way for SaaS companies to share code without enabling hyperscaler clones; fair for businesses that were unlikely to go full OSS anyway.[fsl+2](https://fsl.software/)
        
- **Critical / OSS‑purist camp**:
    
    - Emphasizes that FSL is **not OSI‑approved**, not “open source,” and is fundamentally about preventing forks and competitive hosting.[theregister+2](https://www.theregister.com/2023/11/20/sentry_introduces_the_functional_source/)
        
    - Worries that “fair source” branding is effectively **lobbying/PR** that conflates source‑available with freedom, and warns against uncritically adopting the new taxonomy.[infoq+2](https://www.infoq.com/news/2023/12/functional-source-license/)
        
    - Raises practical concerns: vendor lock‑in during the 2‑year window, difficulty for third‑party hosting/support vendors, and ambiguity about what counts as “competing”.[news.ycombinator+2](https://news.ycombinator.com/item?id=38331173)
        
- **GitButler specific**:
    
    - GitButler openly describes FSL as “source available,” not OSS, and positions it as necessary to sustain the business.[gitbutler+1](https://blog.gitbutler.com/gitbutler-is-now-fair-source)
        
    - HN discourse recognizes FSL as plausible for such a product, but still flags lock‑in and philosophical misalignment with FOSS.[[news.ycombinator](https://news.ycombinator.com/item?id=41184037)]​
        

## Net assessment for “FSL with 2‑year Apache sunset”

- **Accepted enough** that:
    
    - Sentry and GitButler use it without catastrophic backlash,
        
    - TechCrunch and others describe it as the **“main recommended fair source license”** for SaaS‑style projects avoiding traditional OSS pitfalls.[techcrunch+1](https://techcrunch.com/2024/09/22/some-startups-are-going-fair-source-to-avoid-the-pitfalls-of-open-source-licensing/)
        
- **Still contentious** in:
    
    - OSS communities, which continue to stress that it is not free/open source and that delayed open sourcing can be misused (e.g., by constantly shipping incompatible minor versions to avoid effective forks).[lucumr.pocoo+2](https://lucumr.pocoo.org/2023/11/19/cathedral-and-bazaaar-licensing/)
        
- Practically:
    
    - For **enterprise buyers**, FSL is increasingly something legal/compliance has seen before, especially in Sentry’s wake.
        
    - For **developer adoption and contributions**, you should expect a split:
        
        - Pragmatic teams are fine with it, especially if they see the 2‑year sunset as genuine.
            
        - OSS‑aligned engineers will criticize or avoid contributing; some will call out “fair source” marketing explicitly.
            

So “FSL with 2‑year Apache sunset” is **not universally accepted but is now a recognizable, somewhat normalized option** in the SaaS devtools world. It buys you more goodwill than BUSL but doesn’t remove licensing controversy.

---

## 6. Pricing validation: $500 / $2,000 / enterprise for a CLI enforcement tool

You’re proposing:

- **Startup**: $500/month
    
- **Growth**: $2,000/month
    
- **Enterprise**: custom
    

Assuming these cover some reasonable band of developers (e.g., 5–25 devs for Startup, 25–100 for Growth), the implied cost per dev per month is in the same ballpark as other dev infrastructure:

**Greptile (AI code review / rules)**

- Cloud plan: **$30/active dev/month** with unlimited repos, reviews, and custom rules.[greptile+2](https://www.greptile.com/blog/greptile-update)
    
- Code review bot historically priced at **up to $50/dev/month** (cap after that).[[github](https://github.com/greptileai/docs/blob/main/pricing.mdx)]​
    

**Sourcegraph Cody & Enterprise Search**

- Cody Enterprise: around **$59/user/month** per G2 and other sources for enterprise plans.[g2+1](https://www.g2.com/fr/products/sourcegraph-sourcegraph-cody/pricing)
    
- Sourcegraph Enterprise Search: **$49/user/month** for code search suite.[[sourcegraph](https://sourcegraph.com/pricing)]​
    

**Snyk**

- Team plan middle tier: about **$25/month per project**; overall enterprise contracts range from **$5,000 to $70,000/year** depending on footprint and modules.[trustradius+2](https://www.trustradius.com/products/snyk/pricing)
    
- Effective per‑dev costs in practice are often in the **$50–$100+/dev/month** range in midsize orgs.
    

Rough equivalence:

- For a 10‑developer team:
    
    - Your **Startup** tier ($500/mo) → **$50/dev/month**.
        
    - Greptile → $300/mo; Cody Enterprise → ~$590/mo; Snyk Team + extras likely similar magnitude.[greptile+3](https://www.greptile.com/pricing)
        
- For a 40‑developer team:
    
    - Your **Growth** tier ($2,000/mo) → **$50/dev/month** again.
        
    - Comparable to stacking Greptile ($1,200/mo) + Snyk + Sourcegraph; many teams already pay more than this blended.
        

So your pricing shape is:

- **Credible and in‑line** with similar dev infrastructure that:
    
    - Integrates into CI/editor/agents,
        
    - Addresses cross‑cutting concerns (security, quality, context),
        
    - Is sold to teams, not individuals.
        
- Important nuance:
    
    - For **very small teams** (1–3 devs), $500/mo is a heavy lift; you might need:
        
        - A lower entry tier,
            
        - Usage‑based pricing,
            
        - Or a “personal/studio” edition at significantly lower cost to seed adoption.
            
    - For **larger orgs**, per‑dev effective cost is fine; the question becomes ROI framing (e.g., “one prevented outage covers N years of license”).
        

---

## 7. Overall positioning: strengths, weaknesses, blind spots

Bringing this together around the implicit positioning for “keel”:

## What’s strongest

1. **Real, demonstrable enforcement gap**
    
    - Vendors have converged on **hooks + MCP** as the primitives, but there is **no mainstream, vendor‑supplied system that enforces rich, team‑defined software contracts in real time** during code generation.
        
    - The closest examples are:
        
        - Pattern‑based contract frameworks on top of Claude hooks,[[karlkratz](https://karlkratz.de/contracts-rules-claude-code)]​
            
        - Semgrep/SAST integrations into Cursor hooks,[[cursor](https://cursor.com/blog/hooks-partners)]​
            
        - Policy MCP servers enforcing tool/data access.[mcpmarket+1](https://mcpmarket.com/zh/server/policy)
            
    - That reinforces your thesis: **hooks exist; enforcement is still primitive and bespoke.**
        
2. **Timing with hook ecosystem maturity**
    
    - Claude, Cursor, Gemini CLI, Windsurf, and Letta all have:
        
        - Synchronous hooks,
            
        - Exit code 2 as canonical blocking semantics,
            
        - JSON schemas for tooling.
            
    - This makes it technically straightforward for a CLI to:
        
        - Plug into multiple agents at once,
            
        - Offer a **single enforcement layer** that teams can standardize on.
            
    - The fact that Codex CLI still has no hooks actually **differentiates** you: being the enforcement layer for the “hook‑rich” tools.
        
3. **Persona alignment with public pain**
    
    - Senior devs and engineering leaders are **already calling out architecture decay, code entropy, and AI slop**; they explicitly ask for deterministic guardrails.[reddit+6](https://www.reddit.com/r/automation/comments/1q9y3lt/can_aigenerated_code_actually_be_clean_and/nyyz2bm/)
        
    - Platform/DevEx/security teams are already buying **Greptile, Sourcegraph, Snyk, Semgrep, MCP policy servers**, and exploring hooks as their enforcement surface.[mxcp+4](https://mxcp.dev/docs/features/policies/)
        
    - Your proposed buyers mirror the actual buyers of similar tools.
        
4. **Augment + keel is a natural “context + control” story**
    
    - Augment has clearly staked out “context is the problem; we solve context via MCP”.[augmentcode+3](https://www.augmentcode.com/changelog/context-engine-mcp-in-ga)
        
    - Enforcement via a separate, MCP/hook‑compatible CLI is **obviously composable** and technically non‑overlapping.
        
    - Augment users—by definition early adopters of agentic coding—are likely to be the same teams worrying about AI‑driven architecture drift; offering them enforcement atop Augment makes sense.
        
5. **Pricing looks reasonable relative to similar dev infra**
    
    - At team scale, your tiers translate to **~$50/dev/month**, in‑line with Snyk/Greptile/Sourcegraph for serious teams.
        
    - That makes it straightforward to frame ROI: e.g., if you can prevent one serious production incident or slash code‑review time by X%, the spend is trivial.
        

## What’s weakest / riskiest

1. **Enforcement as “nice‑to‑have” without a killer metric**
    
    - Many teams today mitigate AI code risk through:
        
        - Human review,
            
        - Existing SAST/DAST tools,
            
        - “Use AI like a junior dev” practices.[reddit+3](https://www.reddit.com/r/ExperiencedDevs/comments/1glzxui/code_llms_vs_junior_engineers/)
            
    - To become a **must‑have**, keel needs:
        
        - Clear, quantitative evidence (e.g., reduction in post‑merge defects, blocked vulnerabilities, or architectural violations),
            
        - Tight integration into existing workflows (hooks + CI),
            
        - Minimal friction in rule authoring (a contract DSL that doesn’t feel like writing another linter).
            
    - Otherwise it risks being viewed as “just more linting” rather than a distinct enforcement layer for agents.
        
2. **Complexity of true type/signature/adjacency enforcement**
    
    - Simple pattern matching and static checks (like the Claude contract system) are tractable; deep, language‑aware type and adjacency contracts across multiple languages and frameworks are **hard**.
        
    - Without careful scoping (e.g., TypeScript+Python first, contracts at module/API boundaries, not every function), you could:
        
        - Overpromise and underdeliver,
            
        - Create brittle checks that frustrate developers and get disabled.
            
    - The strongest value might come from **architectural and API‑boundary contracts** rather than “full type system replacement”.
        
3. **FSL perception and ecosystem building**
    
    - FSL with a 2‑year Apache/MIT sunset is improved vs BUSL, but:
        
        - **Open‑source‑oriented teams will be skeptical** and may avoid adopting or contributing.
            
        - Some will be wary of depending on FSL software in **core development workflow** until they see longevity and a clear commitment to open‑sourcing major versions on schedule.[theregister+4](https://www.theregister.com/2023/11/20/sentry_introduces_the_functional_source/)
            
    - If community contracts, rule sets, or integrations are part of your story, a source‑available license may slow ecosystem growth compared to permissive licensing.
        
4. **CLI‑only positioning risk**
    
    - While CLI integration is powerful and cross‑agent, many developers live in:
        
        - VS Code or VSCode forks (Cursor, Windsurf, Antigravity),
            
        - JetBrains IDEs,
            
        - Cloud IDEs.
            
    - Your enforcement engine likely needs:
        
        - First‑class **hook bundles** for Claude Code, Cursor, Gemini, Windsurf, Letta,
            
        - Possibly thin IDE extensions that configure and surface enforcement results, not just a standalone CLI.
            
    - If the story is “edit config + wire up hooks + run keel CLI,” that’s fine for senior engineers and platform teams but may be a barrier for lower‑leverage teams.
        

## Blind spots / things to think harder about

1. **Where in the lifecycle do you enforce?**
    
    - Hooks can enforce **at generation time** (PreToolUse/PreWrite), but:
        
        - Some invariants are easier to check in CI (e.g., whole‑program type constraints).
            
        - Some are best enforced on **diffs** rather than per‑edit operations.
            
    - A compelling product likely:
        
        - Uses hooks for **fast, local, interactive enforcement** of common contracts (architecture boundaries, obvious type/contract violations, security patterns),
            
        - Uses CI/PR integration for **heavier, global checks** (e.g., whole‑repo type/contract invariants).
            
2. **Who authors and maintains contracts?**
    
    - The Claude contract article works because one person hand‑crafted ~142 rules for a specific stack.[[karlkratz](https://karlkratz.de/contracts-rules-claude-code)]​
        
    - At scale, teams will need:
        
        - A **library of prebuilt contracts** (e.g., “hexagonal architecture”, “no direct DB calls in controllers”, common API patterns),
            
        - A **reasonable contract DSL** that architects and senior engineers can extend without writing raw regex or AST visitors,
            
        - Tooling to handle false positives and contract evolution as architectures change.
            
3. **Interoperability with existing tools**
    
    - Many teams already run:
        
        - Type checkers (tsc, mypy),
            
        - Linters (ESLint, Ruff, flake8),
            
        - SAST (Semgrep, CodeQL, Snyk Code).
            
    - keel needs to be complementary:
        
        - Either orchestrating/integrating these tools under a contract umbrella (e.g., “this contract is enforced by Semgrep rule set X, ESLint config Y, and type checks Z”),
            
        - Or operating at a level they don’t touch (architecture, cross‑module invariants, type/signature consistency across service boundaries).
            
4. **Positioning vs “just Semgrep + hooks”**
    
    - Cursor+Semgrep and Claude+Semgrep+canned hooks are already emerging patterns.[github+2](https://github.com/ibm/mcp-context-forge/issues/415)
        
    - Your differentiation needs to be more than “we wired Semgrep to hooks”:
        
        - Contract abstraction layer,
            
        - Multi‑agent integration,
            
        - Language‑aware adjacency/architecture constraints,
            
        - Better feedback to agents (turning violations into actionable guidance that LLMs can fix automatically).
            

---

## Bottom line: does the positioning hold up?

- **Enforcement gap thesis**: Mostly correct. Hooks and policy servers exist; serious third‑party experiments in contract enforcement are emerging. But no widely‑used product offers **language‑ and architecture‑aware contracts enforced in real time across Claude/Cursor/Gemini/Windsurf/Letta**. That gap is real and visible in public complaints.
    
- **Buyer personas**: Well aligned with current reality. The strongest buyers are **engineering leadership and platform/DevEx teams** in AI‑forward orgs; senior devs and AI‑native companies are natural champions and design partners.
    
- **Augment complementarity**: Very strong. Augment is squarely “context”; it doesn’t touch enforcement. A “context + contracts” story via MCP and hooks is technically sound and narratively compelling.
    
- **FSL licensing**: Pragmatic but not uncontroversial. It is increasingly familiar to enterprise buyers thanks to Sentry and GitButler, but will be criticized by OSS‑aligned developers. If your go‑to‑market leans heavily on community and open contribution of contracts, that tension needs managing.
    
- **Pricing**: In line with comparable dev infrastructure for teams. For solo/small teams, an entry‑level tier or usage‑based model may be needed; for 10–50‑dev shops, your proposed tiers look reasonable.
    

The strongest part of your story is the **intersection of mature hook ecosystems + real, loudly‑voiced concerns about AI‑driven architecture/quality decay**. The weakest parts are **how deep your enforcement actually goes beyond pattern matching**, and **whether you can make enforcement feel like an enabling tool rather than a friction‑heavy gate**. Addressing those two will matter more than any incremental tuning of pricing or the Augment tagline.