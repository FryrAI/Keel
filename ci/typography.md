# Keel Typography

```yaml
headings: Geist Sans
body: Inter
code: JetBrains Mono
philosophy: "Engineering precision in every glyph"
```

## Font Stack

### Geist Sans — Headings & Display

- **Source:** [Vercel Geist](https://vercel.com/font) (SIL Open Font License)
- **Weights:** 500 (Medium), 600 (SemiBold), 700 (Bold)
- **Usage:** h1-h4, hero text, navigation, button labels, card titles
- **Why Geist:** Designed for developer tools. Geometric precision, excellent at large sizes, tight metrics that feel technical without being cold. The slightly squared terminals echo a monospace sensibility.

```css
font-family: 'Geist Sans', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
```

### Inter — Body & UI

- **Source:** [Google Fonts](https://fonts.google.com/specimen/Inter) (SIL Open Font License)
- **Weights:** 400 (Regular), 500 (Medium)
- **Usage:** Paragraphs, descriptions, labels, form inputs, tooltips
- **Why Inter:** Optimized for screens at small sizes. Tall x-height, tabular figures, and excellent legibility at 14-16px. Pairs naturally with Geist (both are geometric, both prioritize clarity).

```css
font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
```

### JetBrains Mono — Code

- **Source:** [JetBrains](https://www.jetbrains.com/lp/mono/) (SIL Open Font License)
- **Weights:** 400 (Regular), 500 (Medium)
- **Features:** Ligatures ON for `=>`, `!=`, `>=`, `<=`, `->`, `::`
- **Usage:** Code blocks, terminal output, inline code, hash values, file paths
- **Why JetBrains Mono:** Increased letter height for better readability, distinctive character shapes (l/1/I disambiguation), and ligatures that make code more scannable.

```css
font-family: 'JetBrains Mono', 'Fira Code', 'Cascadia Code', 'Consolas', monospace;
```

---

## Type Scale

Base size: **16px** (1rem). Scale ratio: **1.25** (Major Third).

### Dark Mode (Landing Page)

| Level | Size | Weight | Font | Line Height | Letter Spacing | Usage |
|-------|------|--------|------|-------------|----------------|-------|
| Display | 64px / 4rem | 700 | Geist | 1.1 | -0.03em | Hero headline only |
| h1 | 48px / 3rem | 700 | Geist | 1.15 | -0.025em | Section titles |
| h2 | 36px / 2.25rem | 600 | Geist | 1.2 | -0.02em | Subsection titles |
| h3 | 28px / 1.75rem | 600 | Geist | 1.3 | -0.015em | Card titles |
| h4 | 22px / 1.375rem | 500 | Geist | 1.4 | -0.01em | Labels, small titles |
| Body L | 18px / 1.125rem | 400 | Inter | 1.65 | 0 | Lead paragraphs |
| Body | 16px / 1rem | 400 | Inter | 1.6 | 0 | Default body text |
| Body S | 14px / 0.875rem | 400 | Inter | 1.5 | 0.005em | Captions, meta |
| Code L | 16px / 1rem | 400 | JetBrains | 1.7 | 0 | Code blocks |
| Code S | 14px / 0.875rem | 400 | JetBrains | 1.5 | 0.01em | Inline code |
| Overline | 12px / 0.75rem | 500 | Inter | 1.4 | 0.08em | Section labels (uppercase) |

### Responsive Adjustments

| Breakpoint | Display | h1 | h2 | Body |
|------------|---------|----|----|------|
| Desktop (>1024px) | 64px | 48px | 36px | 16px |
| Tablet (768-1024px) | 48px | 40px | 30px | 16px |
| Mobile (<768px) | 36px | 32px | 24px | 16px |

---

## Pairing Rationale

The three fonts form a hierarchy of warmth:

```
Geist Sans (geometric, tight)    → Structural authority → Headings
     ↓ pairs with
Inter (geometric, friendly)      → Readable clarity    → Body
     ↓ pairs with
JetBrains Mono (monospace, open) → Technical precision  → Code
```

All three share geometric DNA — they feel like members of the same family without being identical. The progression from Geist's squared terminals to JetBrains Mono's fixed width mirrors keel's journey from architecture to code.

---

## Text Color Pairings

See [colors.md](colors.md) for full token table. Quick reference:

| Context | Color | Token |
|---------|-------|-------|
| Headings on dark bg | `#F8FAFC` | `--text-heading` |
| Body on dark bg | `#E2E8F0` | `--text-body` |
| Muted / captions | `#6B7280` | `--text-muted` |
| Links | `#2DD4A8` | `--teal-500` |
| Inline code bg | `#1A1E25` | `--bg-raised` |
| Code text | `#E2E8F0` | `--text-body` |
| Code keywords | `#2DD4A8` | `--teal-500` |
| Code strings | `#E8734A` | `--coral-500` |

---

## Rules

1. **Never use more than 2 weights per font in a single view.** Geist: 600+700 for headings. Inter: 400+500 for body. JetBrains: 400 only.
2. **Headings are always Geist Sans.** Never set a heading in Inter or JetBrains Mono.
3. **Body is always Inter.** Never set body paragraphs in Geist or JetBrains.
4. **Code is always JetBrains Mono.** This includes hash values (`kXt9mRp2v4L`), file paths, CLI commands, and any technical identifier.
5. **Negative letter-spacing on headings, zero or positive on body.** Headings tighten to feel dense and structural. Body stays open for readability.
6. **Line height increases as size decreases.** Display at 1.1, body at 1.6, code at 1.7. Small text needs more breathing room.
7. **Uppercase is reserved for overlines.** Section labels ("HOW IT WORKS", "INTEGRATIONS") use 12px Inter Medium, `letter-spacing: 0.08em`, uppercase. Nothing else should be uppercase.
