//! Per-definition cyclomatic complexity (issue #58): every language's
//! decision-node table, and the nested-definition boundary that keeps an
//! enclosing function from inheriting its children's branches.

use std::path::Path;

use super::*;
use crate::resolver::ParseResult;

/// The `complexity` recorded for the definition named `name`.
fn complexity_of(result: &ParseResult, name: &str) -> u32 {
    result
        .definitions
        .iter()
        .find(|d| d.name == name)
        .unwrap_or_else(|| panic!("definition `{name}` extracted"))
        .complexity
}

fn parse(lang: &str, file: &str, src: &str) -> ParseResult {
    TreeSitterParser::new()
        .parse_file(lang, Path::new(file), src)
        .expect("parse")
}

/// A function with no decision point is one path — the formula's floor.
#[test]
fn a_straight_line_function_scores_one() {
    let rust = parse(
        "rust",
        "a.rs",
        "fn plain() {\n    let x = 1;\n    println!(\"{x}\");\n}\n",
    );
    assert_eq!(complexity_of(&rust, "plain"), 1);

    let py = parse("python", "a.py", "def plain():\n    x = 1\n    return x\n");
    assert_eq!(complexity_of(&py, "plain"), 1);

    let ts = parse(
        "typescript",
        "a.ts",
        "function plain() {\n  const x = 1;\n  return x;\n}\n",
    );
    assert_eq!(complexity_of(&ts, "plain"), 1);

    let go = parse(
        "go",
        "a.go",
        "package p\n\nfunc plain() int {\n\tx := 1\n\treturn x\n}\n",
    );
    assert_eq!(complexity_of(&go, "plain"), 1);
}

/// Rust: `if`, `match` arms, loops, `&&`, and the `?` operator each add a path.
/// `if let` is an ordinary `if_expression` in the grammar and must count once,
/// not twice.
#[test]
fn rust_counts_branches_arms_loops_and_try() {
    let src = "\
fn branchy(v: Option<u32>) -> Result<u32, E> {
    if v.is_some() && v != Some(0) {
        for i in 0..3 {
            work(i);
        }
    }
    while let Some(x) = next() {
        drop(x);
    }
    let n = compute()?;
    match n {
        0 => 1,
        1 => 2,
        _ => 3,
    }
}
";
    // 1 base + if + && + for + while-let + ? + 3 match arms = 9
    assert_eq!(complexity_of(&parse("rust", "a.rs", src), "branchy"), 9);
}

/// Python: `elif` is its own clause, `except` arms and `and`/`or` each count,
/// and a conditional expression is a branch.
#[test]
fn python_counts_elif_except_and_boolean_operators() {
    let src = "\
def branchy(items):
    if items and len(items) > 1:
        pass
    elif items:
        pass
    for i in items:
        pass
    try:
        risky()
    except ValueError:
        pass
    except KeyError:
        pass
    return 1 if items else 0
";
    // 1 base + if + and + elif + for + 2 except + conditional = 8
    assert_eq!(complexity_of(&parse("python", "a.py", src), "branchy"), 8);
}

/// TypeScript: `switch` cases count but the default does not (it is the
/// fall-through path, not a decision), and `catch`/ternary/`||` each add one.
#[test]
fn typescript_counts_switch_cases_catch_and_ternary() {
    let src = "\
function branchy(n: number): number {
  if (n > 0 || n < -1) {
    for (const x of []) {}
  }
  switch (n) {
    case 1: break;
    case 2: break;
    default: break;
  }
  try { risky(); } catch (e) {}
  return n > 0 ? 1 : 0;
}
";
    // 1 base + if + || + for-in + 2 cases + catch + ternary = 8
    assert_eq!(
        complexity_of(&parse("typescript", "a.ts", src), "branchy"),
        8
    );
}

/// Go: `for` covers every loop form, and switch/select arms are their own
/// node kinds per switch flavour.
#[test]
fn go_counts_ifs_loops_and_switch_arms() {
    let src = "\
package p

func branchy(n int) int {
\tif n > 0 && n < 10 {
\t\tfor i := 0; i < n; i++ {
\t\t}
\t}
\tswitch n {
\tcase 1:
\tcase 2:
\t}
\treturn n
}
";
    // 1 base + if + && + for + 2 expression cases = 6
    assert_eq!(complexity_of(&parse("go", "a.go", src), "branchy"), 6);
}

/// A nested named definition carries its own branches; the enclosing
/// definition must not inherit them, or every outer function would score as
/// the sum of everything it happens to contain.
#[test]
fn nested_definitions_do_not_fold_into_their_parent() {
    let rust = parse(
        "rust",
        "a.rs",
        "fn outer() {\n    fn inner(x: u32) {\n        if x > 0 {}\n        if x > 1 {}\n    }\n    inner(1);\n}\n",
    );
    assert_eq!(complexity_of(&rust, "outer"), 1);
    assert_eq!(complexity_of(&rust, "inner"), 3);

    let py = parse(
        "python",
        "a.py",
        "def outer():\n    def inner(x):\n        if x:\n            pass\n        if x:\n            pass\n    return inner\n",
    );
    assert_eq!(complexity_of(&py, "outer"), 1);
    assert_eq!(complexity_of(&py, "inner"), 3);

    let ts = parse(
        "typescript",
        "a.ts",
        "function outer() {\n  function inner(x: number) {\n    if (x) {}\n    if (x) {}\n  }\n  return inner;\n}\n",
    );
    assert_eq!(complexity_of(&ts, "outer"), 1);
    assert_eq!(complexity_of(&ts, "inner"), 3);

    let method = parse(
        "typescript",
        "a.ts",
        "class C {\n  outer() {\n    return 1;\n  }\n  inner(x: number) {\n    if (x) {}\n    return x;\n  }\n}\n",
    );
    assert_eq!(complexity_of(&method, "outer"), 1);
    assert_eq!(complexity_of(&method, "inner"), 2);
}
