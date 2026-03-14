//! Braille-based radar chart and visual audit display formatting.

use keel_enforce::types::{AuditDimension, AuditResult, AuditSeverity};

// ── Constants ──────────────────────────────────────────────────────────────────

const CANVAS_COLS: usize = 21;
const CANVAS_ROWS: usize = 9;
const PIXEL_W: usize = CANVAS_COLS * 2; // 42
const PIXEL_H: usize = CANVAS_ROWS * 4; // 36
const CENTER_X: f64 = 20.0;
const CENTER_Y: f64 = 17.0;
const MAX_RADIUS: f64 = 15.0;
const INNER_W: usize = 55;

/// Braille dot bit positions: [row_in_cell 0..4][col_in_cell 0..2]
const BRAILLE_DOT: [[u8; 2]; 4] = [
    [0x01, 0x08],
    [0x02, 0x10],
    [0x04, 0x20],
    [0x40, 0x80],
];

// ── BrailleCanvas ──────────────────────────────────────────────────────────────

struct BrailleCanvas {
    pixels: [[bool; PIXEL_W]; PIXEL_H],
}

impl BrailleCanvas {
    fn new() -> Self {
        Self {
            pixels: [[false; PIXEL_W]; PIXEL_H],
        }
    }

    fn set_pixel(&mut self, x: i32, y: i32) {
        if x >= 0 && (x as usize) < PIXEL_W && y >= 0 && (y as usize) < PIXEL_H {
            self.pixels[y as usize][x as usize] = true;
        }
    }

    /// Bresenham's line algorithm.
    fn draw_line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx: i32 = if x0 < x1 { 1 } else { -1 };
        let sy: i32 = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        let (mut cx, mut cy) = (x0, y0);
        loop {
            self.set_pixel(cx, cy);
            if cx == x1 && cy == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                cx += sx;
            }
            if e2 <= dx {
                err += dx;
                cy += sy;
            }
        }
    }

    fn render(&self) -> Vec<String> {
        (0..CANVAS_ROWS)
            .map(|row| {
                (0..CANVAS_COLS)
                    .map(|col| {
                        let mut bits: u8 = 0;
                        for py in 0..4 {
                            for px in 0..2 {
                                if self.pixels[row * 4 + py][col * 2 + px] {
                                    bits |= BRAILLE_DOT[py][px];
                                }
                            }
                        }
                        char::from_u32(0x2800 + bits as u32).unwrap_or(' ')
                    })
                    .collect()
            })
            .collect()
    }
}

// ── Radar rendering ────────────────────────────────────────────────────────────

fn diamond_at(r: f64) -> [(i32, i32); 4] {
    let cx = CENTER_X as i32;
    let cy = CENTER_Y as i32;
    let ri = r as i32;
    [
        (cx, cy - ri),
        (cx + ri, cy),
        (cx, cy + ri),
        (cx - ri, cy),
    ]
}

/// Render a radar/spider chart. Axis order: [N, E, S, W].
fn render_radar(scores: [(u32, u32); 4]) -> Vec<String> {
    let mut canvas = BrailleCanvas::new();
    let max = scores.iter().map(|s| s.1).max().unwrap_or(3).max(1);
    let (cx, cy) = (CENTER_X as i32, CENTER_Y as i32);

    // Dotted axis lines
    for y in 2i32..=(PIXEL_H as i32 - 3) {
        if y % 3 == 0 {
            canvas.set_pixel(cx, y);
        }
    }
    for x in 5i32..=(PIXEL_W as i32 - 6) {
        if x % 3 == 0 {
            canvas.set_pixel(x, cy);
        }
    }

    // Reference diamonds at each score level
    for level in 1..=max {
        let r = MAX_RADIUS * level as f64 / max as f64;
        let pts = diamond_at(r);
        for i in 0..4 {
            canvas.draw_line(pts[i].0, pts[i].1, pts[(i + 1) % 4].0, pts[(i + 1) % 4].1);
        }
    }

    // Score polygon
    let radii: [f64; 4] = std::array::from_fn(|i| {
        if scores[i].1 == 0 {
            0.0
        } else {
            (MAX_RADIUS * scores[i].0 as f64 / scores[i].1 as f64).min(MAX_RADIUS)
        }
    });
    let sp = [
        (cx, (CENTER_Y - radii[0]) as i32),
        ((CENTER_X + radii[1]) as i32, cy),
        (cx, (CENTER_Y + radii[2]) as i32),
        ((CENTER_X - radii[3]) as i32, cy),
    ];

    // Double-thick score polygon lines
    for i in 0..4 {
        let j = (i + 1) % 4;
        canvas.draw_line(sp[i].0, sp[i].1, sp[j].0, sp[j].1);
        canvas.draw_line(sp[i].0 + 1, sp[i].1, sp[j].0 + 1, sp[j].1);
        canvas.draw_line(sp[i].0, sp[i].1 + 1, sp[j].0, sp[j].1 + 1);
    }

    // Vertex markers (3x3 blocks)
    for p in &sp {
        for dx in -1..=1i32 {
            for dy in -1..=1i32 {
                canvas.set_pixel(p.0 + dx, p.1 + dy);
            }
        }
    }
    canvas.set_pixel(cx, cy);

    canvas.render()
}

// ── Display helpers ────────────────────────────────────────────────────────────

fn compute_grade(total: u32, max: u32) -> &'static str {
    if max == 0 {
        return "N/A";
    }
    let pct = total as f64 / max as f64 * 100.0;
    if pct >= 95.0 {
        "A+"
    } else if pct >= 85.0 {
        "A"
    } else if pct >= 75.0 {
        "B+"
    } else if pct >= 65.0 {
        "B"
    } else if pct >= 50.0 {
        "C"
    } else if pct >= 35.0 {
        "D"
    } else {
        "F"
    }
}

fn format_bar(score: u32, max: u32, width: usize) -> String {
    if max == 0 {
        return "\u{2591}".repeat(width);
    }
    let filled = ((width as f64 * score as f64 / max as f64).round() as usize).min(width);
    format!(
        "{}{}",
        "\u{2588}".repeat(filled),
        "\u{2591}".repeat(width - filled)
    )
}

fn dim_label(name: &str) -> &str {
    match name {
        "structure" => "Structure",
        "discoverability" => "Discoverability",
        "navigation" => "Navigation",
        "config" => "Agent Config",
        _ => name,
    }
}

fn short_label(name: &str) -> &str {
    match name {
        "discoverability" => "Discover",
        "config" => "Config",
        _ => dim_label(name),
    }
}

fn center_str(text: &str, w: usize) -> String {
    let len = text.chars().count();
    if len >= w {
        return text.to_string();
    }
    let left = (w - len) / 2;
    format!(
        "{}{}{}",
        " ".repeat(left),
        text,
        " ".repeat(w - len - left)
    )
}

fn truncate(s: &str, max_w: usize) -> String {
    if s.chars().count() <= max_w {
        s.to_string()
    } else {
        format!(
            "{}...",
            s.chars().take(max_w.saturating_sub(3)).collect::<String>()
        )
    }
}

fn box_top(w: usize) -> String {
    format!("\u{250c}{}\u{2510}\n", "\u{2500}".repeat(w))
}
fn box_sep(w: usize) -> String {
    format!("\u{251c}{}\u{2524}\n", "\u{2500}".repeat(w))
}
fn box_bot(w: usize) -> String {
    format!("\u{2514}{}\u{2518}\n", "\u{2500}".repeat(w))
}

fn box_row(content: &str, w: usize) -> String {
    let len = content.chars().count();
    if len >= w {
        let truncated: String = content.chars().take(w).collect();
        format!("\u{2502}{}\u{2502}\n", truncated)
    } else {
        format!("\u{2502}{}{}\u{2502}\n", content, " ".repeat(w - len))
    }
}

// ── Radar dimension lookup ─────────────────────────────────────────────────────

fn find_radar_dims<'a>(dims: &'a [AuditDimension]) -> Option<[&'a AuditDimension; 4]> {
    let order = ["structure", "discoverability", "navigation", "config"];
    let found: Vec<_> = order
        .iter()
        .map(|n| dims.iter().find(|d| d.name == *n))
        .collect();
    match (found[0], found[1], found[2], found[3]) {
        (Some(a), Some(b), Some(c), Some(d)) => Some([a, b, c, d]),
        _ => None,
    }
}

// ── Main display compositor ────────────────────────────────────────────────────

pub fn format_audit_display(result: &AuditResult) -> String {
    let w = INNER_W;
    let mut out = String::with_capacity(2048);

    // Header
    out.push_str(&box_top(w));
    out.push_str(&box_row(
        &center_str("keel audit \u{2014} AI Readiness", w),
        w,
    ));
    out.push_str(&box_sep(w));

    // Radar section (only when all 4 standard dimensions are present)
    if let Some(dims) = find_radar_dims(&result.dimensions) {
        let scores = [
            (dims[0].score, dims[0].max_score),
            (dims[1].score, dims[1].max_score),
            (dims[2].score, dims[2].max_score),
            (dims[3].score, dims[3].max_score),
        ];
        let chart = render_radar(scores);
        let chart_col = (w - CANVAS_COLS) / 2;
        let mid = CANVAS_ROWS / 2;

        // North axis label
        let n_lbl = format!(
            "{} ({}/{})",
            dim_label(&dims[0].name),
            dims[0].score,
            dims[0].max_score
        );
        out.push_str(&box_row(&center_str(&n_lbl, w), w));
        out.push_str(&box_row("", w));

        // Chart rows
        for (i, row) in chart.iter().enumerate() {
            if i == mid {
                // West + chart + East labels on middle row
                let wl = format!(
                    "  {} ({}/{})",
                    short_label(&dims[3].name),
                    dims[3].score,
                    dims[3].max_score
                );
                let el = format!(
                    "{} ({}/{})",
                    short_label(&dims[1].name),
                    dims[1].score,
                    dims[1].max_score
                );
                let left = format!("{:<width$}", wl, width = chart_col);
                let rem = w - chart_col - CANVAS_COLS;
                let right = format!("{:>width$}", el, width = rem);
                out.push_str(&box_row(&format!("{}{}{}", left, row, right), w));
            } else {
                let left = " ".repeat(chart_col);
                let right = " ".repeat(w - chart_col - CANVAS_COLS);
                out.push_str(&box_row(&format!("{}{}{}", left, row, right), w));
            }
        }

        // South axis label
        out.push_str(&box_row("", w));
        let s_lbl = format!(
            "{} ({}/{})",
            dim_label(&dims[2].name),
            dims[2].score,
            dims[2].max_score
        );
        out.push_str(&box_row(&center_str(&s_lbl, w), w));
    }

    // Bars section
    out.push_str(&box_sep(w));
    for dim in &result.dimensions {
        let label = dim_label(&dim.name);
        let bar = format_bar(dim.score, dim.max_score, 15);
        let check = if dim.score == dim.max_score {
            "  \u{2713}"
        } else {
            "   "
        };
        let line = format!(
            "  {:<16}  {}  {}/{}{}",
            label, bar, dim.score, dim.max_score, check
        );
        out.push_str(&box_row(&line, w));
    }

    // Total + Grade
    out.push_str(&box_sep(w));
    let grade = compute_grade(result.total_score, result.max_score);
    let t_left = format!("  Total: {}/{}", result.total_score, result.max_score);
    let t_right = format!("Grade: {}  ", grade);
    let gap = w.saturating_sub(t_left.chars().count() + t_right.chars().count());
    out.push_str(&box_row(
        &format!("{}{}{}", t_left, " ".repeat(gap), t_right),
        w,
    ));

    // Findings (skip if none)
    let has_findings = result
        .dimensions
        .iter()
        .flat_map(|d| &d.findings)
        .any(|f| f.severity != AuditSeverity::Pass);

    if has_findings {
        out.push_str(&box_sep(w));
        for dim in &result.dimensions {
            for f in &dim.findings {
                if f.severity == AuditSeverity::Pass {
                    continue;
                }
                let tag = match f.severity {
                    AuditSeverity::Tip => "[TIP] ",
                    AuditSeverity::Warn => "[WARN]",
                    AuditSeverity::Fail => "[FAIL]",
                    AuditSeverity::Pass => unreachable!(),
                };
                let file_part = match &f.file {
                    Some(p) => format!("{}: ", p),
                    None => String::new(),
                };
                let msg = format!("  {} {} \u{2014} {}{}", tag, f.check, file_part, f.message);
                out.push_str(&box_row(&truncate(&msg, w), w));
                if let Some(ref tip) = f.tip {
                    let tip_line = format!("    Tip: {}", tip);
                    out.push_str(&box_row(&truncate(&tip_line, w), w));
                }
            }
        }
    }

    out.push_str(&box_bot(w));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_braille_canvas_empty() {
        let canvas = BrailleCanvas::new();
        let lines = canvas.render();
        assert_eq!(lines.len(), CANVAS_ROWS);
        // Empty canvas = all U+2800 (blank braille)
        for line in &lines {
            assert_eq!(line.chars().count(), CANVAS_COLS);
            assert!(line.chars().all(|c| c == '\u{2800}'));
        }
    }

    #[test]
    fn test_braille_canvas_single_pixel() {
        let mut canvas = BrailleCanvas::new();
        canvas.set_pixel(0, 0);
        let lines = canvas.render();
        let first_char = lines[0].chars().next().unwrap();
        assert_ne!(first_char, '\u{2800}', "pixel should be visible");
    }

    #[test]
    fn test_compute_grade() {
        assert_eq!(compute_grade(12, 12), "A+");
        assert_eq!(compute_grade(11, 12), "A");
        assert_eq!(compute_grade(9, 12), "B+");
        assert_eq!(compute_grade(8, 12), "B");
        assert_eq!(compute_grade(6, 12), "C");
        assert_eq!(compute_grade(4, 12), "F");
        assert_eq!(compute_grade(2, 12), "F");
        assert_eq!(compute_grade(0, 0), "N/A");
    }

    #[test]
    fn test_format_bar() {
        let bar = format_bar(3, 3, 15);
        assert_eq!(bar.chars().count(), 15);
        assert!(bar.chars().all(|c| c == '\u{2588}'));

        let bar = format_bar(0, 3, 15);
        assert_eq!(bar.chars().count(), 15);
        assert!(bar.chars().all(|c| c == '\u{2591}'));
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("this is too long", 10), "this is...");
    }

    #[test]
    fn test_render_radar_does_not_panic() {
        let scores = [(3, 3), (2, 3), (1, 3), (0, 3)];
        let lines = render_radar(scores);
        assert_eq!(lines.len(), CANVAS_ROWS);
        for line in &lines {
            assert_eq!(line.chars().count(), CANVAS_COLS);
        }
    }

    #[test]
    fn test_render_radar_all_zero() {
        let scores = [(0, 3), (0, 3), (0, 3), (0, 3)];
        let lines = render_radar(scores);
        assert_eq!(lines.len(), CANVAS_ROWS);
    }
}
