# Keel Visual Language

```yaml
style: Technical precision, terminal-authentic
icons: Phosphor (thin weight)
illustrations: Structural line art, never decorative
motion: Subtle, functional, no bounce
```

## Iconography

### Icon Set: Phosphor

- **Library:** [Phosphor Icons](https://phosphoricons.com/) (MIT License)
- **Weight:** Thin (1.5px stroke) — matches the keel line mark's minimal weight
- **Size grid:** 24px default, 20px compact, 16px inline
- **Color:** `--text-muted` (`#6B7280`) by default, `--teal-500` (`#2DD4A8`) for active/selected

### Recommended Icons

| Concept | Phosphor Icon | Usage |
|---------|--------------|-------|
| Map / Graph | `graph` | keel map, structural graph |
| Compile / Check | `check-circle` | keel compile, passing |
| Error / Violation | `x-circle` | Compile failure |
| Warning | `warning` | Warnings, attention |
| Discover | `magnifying-glass` | keel discover |
| Where / Location | `map-pin` | keel where |
| Explain | `chat-text` | keel explain |
| Performance | `lightning` | Speed stats |
| Integration | `plug` | Tool integrations |
| Code | `code` | Code blocks, snippets |
| Terminal | `terminal-window` | CLI references |
| Ship / Deploy | `paper-plane-right` | Deployment, shipping |

### Icon Rules
1. Never use filled icons — always thin/stroke weight
2. Never colorize icons with coral unless indicating a warning
3. Icons are functional, not decorative — every icon must have a purpose
4. Never use emoji in place of icons

---

## Illustration Approach

### Style: Structural Diagrams

Illustrations in the keel brand are **technical diagrams**, not artistic illustrations. They explain structure, flow, and relationships.

#### What illustrations look like:
- Node-and-edge graphs showing function relationships
- Flowcharts showing Map → Compile → Fix
- Terminal screenshots with real keel output
- Architectural diagrams with clean boxes and arrows
- Diff-style before/after comparisons

#### What illustrations do NOT look like:
- Cartoon characters or mascots
- Abstract blobs or gradients
- Stock illustration styles (isometric offices, etc.)
- Hand-drawn or sketch aesthetics

### Diagram Construction Rules

1. **Lines:** 1.5px stroke, `--border-default` (`#374151`) for structure, `--teal-500` for highlighted paths
2. **Nodes:** Rounded rectangle, 8px radius, `--bg-surface` fill, `--border-subtle` stroke
3. **Labels:** Inter 14px Medium, `--text-body`
4. **Arrows:** Open arrowheads (not filled), same stroke weight as lines
5. **Background:** Always `--bg-abyss` or transparent — diagrams float on the page background

### Terminal Screenshots

Terminal output is a first-class illustration type. Rules:

1. Use a dark terminal theme matching `--bg-abyss` / `--bg-surface`
2. Show real keel output, never mocked text
3. Syntax highlighting:
   - Commands: `--text-heading` (`#F8FAFC`)
   - Output keywords: `--teal-500` (`#2DD4A8`)
   - Errors: `--error` (`#EF4444`)
   - Warnings: `--coral-500` (`#E8734A`)
   - File paths: `--text-muted` (`#6B7280`)
4. Include the prompt character (`$` or `>`) in `--text-muted`
5. Window chrome: minimal — just a top bar with 3 dots (macOS style) or no chrome at all
6. Corner radius: 12px on the container
7. Padding: 24px inside the terminal block

---

## Code Block Styling

### Inline Code

```
Background: --bg-raised (#1A1E25)
Text: --text-body (#E2E8F0)
Font: JetBrains Mono 14px
Padding: 2px 6px
Border-radius: 4px
Border: 1px solid --border-subtle (#1F2937)
```

### Code Blocks

```
Background: --bg-surface (#111419)
Text: --text-body (#E2E8F0)
Font: JetBrains Mono 14px, line-height 1.7
Padding: 24px
Border-radius: 12px
Border: 1px solid --border-subtle (#1F2937)
```

### Syntax Highlighting Theme (Dark)

| Token Type | Color | Token |
|-----------|-------|-------|
| Keywords | `#2DD4A8` | `--teal-500` |
| Strings | `#E8734A` | `--coral-500` |
| Comments | `#6B7280` | `--text-muted` |
| Functions | `#F8FAFC` | `--text-heading` |
| Variables | `#E2E8F0` | `--text-body` |
| Numbers | `#3B82F6` | `--info` |
| Types/Classes | `#34D399` | `--teal-400` |
| Operators | `#9CA3AF` | gray-400 |
| Line numbers | `#374151` | gray-700 |

---

## Motion & Animation

### Philosophy
Motion in keel is **functional, not decorative**. It communicates state changes, not personality. Think of it as the visual equivalent of a well-formatted log line — concise, informative, no wasted energy.

### Approved Animations

| Element | Animation | Duration | Easing |
|---------|-----------|----------|--------|
| Page sections | Fade in + translate up 16px | 400ms | `ease-out` |
| Cards | Fade in + translate up 8px, staggered 80ms | 300ms | `ease-out` |
| Buttons (hover) | Background color shift | 150ms | `ease-in-out` |
| Links (hover) | Underline reveal (left to right) | 200ms | `ease-out` |
| Terminal typing | Character-by-character reveal | 30ms/char | linear |
| Numbers counting | Count up to final value | 1200ms | `ease-out` |
| Graph nodes | Fade in + scale from 0.9 | 300ms, staggered 50ms | `ease-out` |
| Error flash | Red border pulse (1 cycle) | 600ms | `ease-in-out` |

### Motion Rules
1. **No bounce.** No spring physics, no overshoot. keel is precise.
2. **No slide-in from edges.** Elements appear in place, not from offscreen.
3. **No infinite animations.** Everything has a defined end state. No pulsing loaders, no rotating spinners — use a determinate progress bar or static state.
4. **Maximum 400ms for any single animation.** If it takes longer, it's too complex.
5. **Respect `prefers-reduced-motion`.** All animations should be skippable via the OS accessibility setting.
6. **Stagger, don't synchronize.** When multiple elements animate, stagger by 50-80ms. Synchronized animation feels mechanical.

---

## Spacing System

Base unit: **4px**. All spacing uses multiples of 4.

| Token | Value | Usage |
|-------|-------|-------|
| `--space-1` | 4px | Tight: icon-to-label, inline elements |
| `--space-2` | 8px | Compact: list items, small gaps |
| `--space-3` | 12px | Default: card padding (small), form gaps |
| `--space-4` | 16px | Standard: card padding, section internal |
| `--space-6` | 24px | Comfortable: card padding (large), group gaps |
| `--space-8` | 32px | Generous: section padding |
| `--space-12` | 48px | Section gaps |
| `--space-16` | 64px | Major section separators |
| `--space-24` | 96px | Hero padding, page-level spacing |
| `--space-32` | 128px | Top/bottom page margins |
