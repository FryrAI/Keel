# Keel Logo — "The Structural Line"

```yaml
concept: Abstract keel profile, minimal
style: Single continuous line with subtle curve
wordmark: "keel" lowercase, Geist Sans Medium
colors: Single-color only (never multicolor)
```

## Concept

The keel logo is a single horizontal line with a subtle downward curve — the profile of a ship's keel viewed from the side. It communicates:

- **Foundation** — the line everything else rests on
- **Structure** — deliberate geometry, not decoration
- **Subtlety** — you have to know what a keel looks like to see it

The curve is slight. It's not a smile, not a wave, not a swoosh. It's the hydrodynamic profile of a structural beam — engineered, not stylized.

## The Mark

### Geometry

```
                        ___________________________
                   ____/                           \____
             _____/                                     \_____
       _____/                                                 \_____
______/                                                             \___

  ^                                                                    ^
  Bow (left)                                                   Stern (right)
```

**Construction details:**
- Start from left, rising gently to a high point ~40% from left
- Long, flat run across the center (the structural backbone)
- Gentle descent to the right
- Total aspect ratio: approximately **8:1** (wide and low)
- The curve is a single cubic bezier or quadratic — no inflection points, no sharp corners
- Stroke weight: 2-3px at default size (scales proportionally)
- Stroke cap: round
- No fill — stroke only

### Clear Space

Minimum clear space = **1x the height of the mark** on all sides. The mark's height is measured from the highest point of the curve to the baseline.

```
         ┌─────────────────────────────────────┐
         │              1x margin              │
         │    ┌───────────────────────────┐    │
         │ 1x │    ___________________    │ 1x │
         │    │___/                   \___|    │
         │    └───────────────────────────┘    │
         │              1x margin              │
         └─────────────────────────────────────┘
```

### Minimum Size
- Digital: 80px wide minimum
- Print: 20mm wide minimum

---

## The Wordmark

### Typography

- Font: **Geist Sans Medium** (weight 500)
- Case: **all lowercase** — always `keel`, never `Keel` or `KEEL`
- Letter-spacing: **0.05em** (generous, slightly open)
- The wide spacing echoes the horizontal nature of the mark

### Lockup Options

#### 1. Horizontal (Primary)

```
    ___________________
___/                   \___    keel
```

Mark on the left, wordmark right-aligned to the mark's baseline. The text baseline aligns with the lowest point of the curve.

#### 2. Stacked (Secondary)

```
         ___________________
    ____/                   \____

                keel
```

Mark above, wordmark centered below. Used when horizontal space is limited (social avatars, narrow sidebars).

#### 3. Wordmark Only

```
keel
```

Used in tight spaces where the mark would be too small to read (favicons at smallest sizes, inline text references).

#### 4. Mark Only

```
    ___________________
___/                   \___
```

Used for favicons, app icons, and any context where the name is already present nearby.

---

## Color Variants

The logo is **single-color only**. Never use two colors, never use gradients.

| Context | Mark Color | Wordmark Color |
|---------|-----------|---------------|
| On dark background (`#0A0D11`) | `#2DD4A8` (teal-500) | `#F8FAFC` (text-heading) |
| On dark background (alt) | `#F8FAFC` (white) | `#F8FAFC` (white) |
| On light background (`#FFFFFF`) | `#0D9488` (teal-700) | `#0F172A` (text-heading-light) |
| On light background (alt) | `#0F172A` (near-black) | `#0F172A` (near-black) |
| Monochrome (print, stamps) | Black | Black |

### Favicon

- Shape: The curved line mark only, no wordmark
- Color: `#2DD4A8` on transparent, or white on `#0D9488`
- Sizes: 16x16, 32x32, 180x180 (Apple Touch), 512x512 (PWA)
- The curve should be visible even at 16x16 — this constrains how subtle the curve can be

---

## Construction Notes for Designers

### SVG Guidelines
- Single `<path>` element for the curve
- `stroke-linecap="round"` and `stroke-linejoin="round"`
- No `fill` on the mark path
- Wordmark as `<text>` with Geist Sans embedded, or as outlined `<path>` for the final asset
- ViewBox should include clear space

### The Curve Must Feel Engineered
- Use precise bezier control points, not hand-drawn paths
- The curve should be smooth enough to feel mathematical
- If it looks "organic" or "hand-lettered," it's wrong
- If it looks like a Nike swoosh, it's too dramatic — flatten it
- If it looks like a straight line, the curve is too subtle — add 10% more

### Test at Scale
The mark must work at:
- **Hero size** (800px wide on a landing page)
- **Nav size** (120px wide in a navigation bar)
- **Favicon size** (16px as a browser tab icon)
- **Print size** (20mm on a business card)

At small sizes, the stroke weight may need to increase proportionally. Define 2-3 optical size variants if needed.

---

## Don'ts

1. Never rotate the mark (it must always be horizontal)
2. Never add a second color or gradient
3. Never add a hull, mast, or other nautical elements — the line IS the keel
4. Never enclose the mark in a shape (circle, square, shield)
5. Never use the mark as a pattern or repeating element
6. Never animate the mark drawing itself (too cliche) — see [visual-language.md](visual-language.md) for approved motion
7. Never capitalize the wordmark — it's always `keel`
8. Never change the aspect ratio of the mark (no vertical stretching)
9. Never add a shadow, glow, or 3D effect to the mark
10. Never place the mark on a busy background without a solid backing
