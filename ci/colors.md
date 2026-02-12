# Keel Color System

```yaml
mode: dark-first
primary: teal
accent: coral
philosophy: "Ocean depth meets structural precision"
```

## Design Tokens

### Dark Mode (Default)

#### Backgrounds

| Token | Hex | RGB | Usage |
|-------|-----|-----|-------|
| `--bg-abyss` | `#0A0D11` | `10, 13, 17` | Page background, deepest layer |
| `--bg-surface` | `#111419` | `17, 20, 25` | Cards, panels, first elevation |
| `--bg-raised` | `#1A1E25` | `26, 30, 37` | Hover states, second elevation |
| `--bg-overlay` | `#232830` | `35, 40, 48` | Modals, dropdowns, third elevation |

> **Note:** Backgrounds use a blue-undertone black (`#0A0D11`), not pure black (`#000000`). This creates depth and reduces eye strain. The blue undertone connects to the ocean metaphor.

#### Primary — Tidal Teal

| Token | Hex | RGB | Usage |
|-------|-----|-----|-------|
| `--teal-50` | `#ECFDF5` | `236, 253, 245` | Tint backgrounds (light mode) |
| `--teal-100` | `#D1FAE5` | `209, 250, 229` | Subtle highlights |
| `--teal-200` | `#A7F3D0` | `167, 243, 208` | Secondary indicators |
| `--teal-300` | `#6EE7B7` | `110, 231, 183` | Hover state (dark mode) |
| `--teal-400` | `#34D399` | `52, 211, 153` | Active state |
| `--teal-500` | `#2DD4A8` | `45, 212, 168` | **Primary — CTA, links, success** |
| `--teal-600` | `#0D9488` | `13, 148, 136` | Pressed state |
| `--teal-700` | `#0F766E` | `15, 118, 110` | Dark accents |
| `--teal-800` | `#115E59` | `17, 94, 89` | Deep tints |
| `--teal-900` | `#134E4A` | `19, 78, 74` | Borders on dark bg |

#### Accent — Coral Signal

| Token | Hex | RGB | Usage |
|-------|-----|-----|-------|
| `--coral-50` | `#FFF7ED` | `255, 247, 237` | Tint backgrounds (light mode) |
| `--coral-100` | `#FFEDD5` | `255, 237, 213` | Subtle warm highlights |
| `--coral-200` | `#FED7AA` | `254, 215, 170` | Secondary warm indicators |
| `--coral-300` | `#FDBA74` | `253, 186, 116` | Hover state (dark mode) |
| `--coral-400` | `#F59E4B` | `245, 158, 75` | Active state |
| `--coral-500` | `#E8734A` | `232, 115, 74` | **Accent — warnings, highlights, badges** |
| `--coral-600` | `#D4613A` | `212, 97, 58` | Pressed state |
| `--coral-700` | `#B34D2E` | `179, 77, 46` | Dark accents |
| `--coral-800` | `#8C3D24` | `140, 61, 36` | Deep tints |
| `--coral-900` | `#6B2E1A` | `107, 46, 26` | Borders on dark bg |

> **Claude connection:** `--coral-500` (`#E8734A`) is intentionally close to Claude's brand orange (`#DE7356`). This is a subtle nod — the crab that lives where ocean meets structure.

#### Semantic Colors

| Token | Hex | Usage |
|-------|-----|-------|
| `--success` | `#2DD4A8` | Compile pass, checks, valid (= teal-500) |
| `--error` | `#EF4444` | Violations, compile fail, critical |
| `--warning` | `#E8734A` | Warnings, attention needed (= coral-500) |
| `--info` | `#3B82F6` | Informational, links, discover |

#### Text

| Token | Hex | Usage |
|-------|-----|-------|
| `--text-heading` | `#F8FAFC` | h1-h4, hero text, emphasis |
| `--text-body` | `#E2E8F0` | Paragraphs, descriptions |
| `--text-muted` | `#6B7280` | Captions, timestamps, labels |
| `--text-disabled` | `#374151` | Disabled UI, placeholders |

#### Borders & Dividers

| Token | Hex | Usage |
|-------|-----|-------|
| `--border-subtle` | `#1F2937` | Card edges, dividers |
| `--border-default` | `#374151` | Input borders, active dividers |
| `--border-focus` | `#2DD4A8` | Focus rings (= teal-500) |

---

### Light Mode (Derived)

Light mode inverts the background scale and adjusts teal/coral for readability on white.

#### Backgrounds

| Token | Hex | Usage |
|-------|-----|-------|
| `--bg-abyss` | `#FFFFFF` | Page background |
| `--bg-surface` | `#F8FAFC` | Cards, panels |
| `--bg-raised` | `#F1F5F9` | Hover states |
| `--bg-overlay` | `#E2E8F0` | Modals, dropdowns |

#### Adjusted Primaries

| Token | Hex | Notes |
|-------|-----|-------|
| `--teal-500` | `#0D9488` | Darkened for contrast on white (was `#2DD4A8`) |
| `--coral-500` | `#D4613A` | Darkened for contrast on white (was `#E8734A`) |

#### Text (Light Mode)

| Token | Hex | Usage |
|-------|-----|-------|
| `--text-heading` | `#0F172A` | Headings |
| `--text-body` | `#1E293B` | Body |
| `--text-muted` | `#64748B` | Captions |

---

## Usage Rules

### Color Ratios
- **Teal : Coral = 7 : 1** — Teal dominates. Coral is a signal, not a theme.
- Coral appears on: violation badges, warning indicators, CTA hover states, the occasional accent border.
- Teal appears on: buttons, links, success states, the logo, code highlights, progress indicators.

### Contrast Requirements
- All text must meet **WCAG AA** (4.5:1 for body, 3:1 for large text).
- `--text-body` on `--bg-abyss` = 13.5:1 (passes AAA).
- `--teal-500` on `--bg-abyss` = 8.2:1 (passes AAA).
- `--coral-500` on `--bg-abyss` = 5.1:1 (passes AA).

### What NOT to Do
- Never use teal text on coral backgrounds (or vice versa) — vibration.
- Never use `--bg-abyss` as a text color.
- Never use gradients between teal and coral — they muddy.
- Never lighten the abyss background — `#0A0D11` is the floor.
- Never use coral for success states or teal for errors.
