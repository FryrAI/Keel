//! Pure graph and rate calculations used by the quality reading.

use std::collections::{HashMap, HashSet};

use keel_core::types::QualityInputs;

/// `count` per thousand lines, rounded to two decimals.
pub(super) fn rate_per_kloc(count: u32, lines: u64) -> f64 {
    if lines == 0 {
        return 0.0;
    }
    (count as f64 * 100_000.0 / lines as f64).round() / 100.0
}

/// `part / whole` rounded to two decimals; 0 when `whole` is 0.
///
/// Two decimals because the series is read as a direction, and more precision
/// manufactures movement out of a single re-resolved edge.
pub(super) fn ratio(part: f64, whole: f64) -> f64 {
    if whole == 0.0 {
        return 0.0;
    }
    (part / whole * 100.0).round() / 100.0
}

/// `reachable_pairs / n²`: for every module, how many *other* modules it can
/// transitively reach via calls/imports, summed over modules.
///
/// Self-pairs are excluded from the numerator — a module trivially "reaches"
/// itself, and that is not propagation. Unjudged: a rise may be legitimate
/// shared infrastructure or spreading coupling, the same stance
/// `cross_module_edge_ratio` takes.
///
/// O(n·(V+E)): a DFS per module. Fine at the hundreds-to-low-thousands of
/// files `keel quality`'s budget targets.
pub(super) fn propagation_cost(graph: &HashMap<String, HashSet<String>>) -> f64 {
    let n = graph.len();
    if n == 0 {
        return 0.0;
    }
    let reachable_pairs: usize = graph
        .keys()
        .map(|start| {
            let mut seen: HashSet<&str> = HashSet::new();
            let mut stack: Vec<&str> = vec![start.as_str()];
            while let Some(module) = stack.pop() {
                for next in graph.get(module).into_iter().flatten() {
                    if next != start && seen.insert(next.as_str()) {
                        stack.push(next.as_str());
                    }
                }
            }
            seen.len()
        })
        .sum();
    ratio(reachable_pairs as f64, (n * n) as f64)
}

/// Build the module dependency graph in the shape Tarjan's SCC expects.
///
/// Both endpoints get a key: the cycle finder skips neighbors that are not
/// keys, so a target file with no entry would hide the cycle it closes.
pub(super) fn module_dependency_graph(inputs: &QualityInputs) -> HashMap<String, HashSet<String>> {
    let mut graph: HashMap<String, HashSet<String>> = HashMap::new();
    for (path, _) in &inputs.file_lines {
        graph.entry(path.clone()).or_default();
    }
    for (source, target) in &inputs.module_deps {
        graph.entry(target.clone()).or_default();
        graph
            .entry(source.clone())
            .or_default()
            .insert(target.clone());
    }
    graph
}
