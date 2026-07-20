//! Per-grammar test-context marking for Python and TypeScript (issue #42).
//!
//! These mirror the Rust `#[cfg(test)]`/`#[test]` tests in `tests.rs`, but for
//! the two languages whose marking lives entirely in the shared ancestry walk
//! (`definition_contexts`): Python (`test_*` names + `TestCase` ancestry) and
//! TypeScript (`describe`/`it`/`test` block ancestry). Go's marking is a Tier-2
//! string check tested in `go/tests.rs`.
//!
//! Python sources use raw strings so their significant indentation survives —
//! a `\`-continued string literal would strip the leading whitespace.

use std::path::Path;

use super::*;

/// Look up a definition by name and return its `in_test_context` flag.
fn in_test(defs: &[Definition], name: &str) -> bool {
    defs.iter()
        .find(|d| d.name == name)
        .unwrap_or_else(|| panic!("missing def {name}"))
        .in_test_context
}

#[test]
fn python_pytest_test_function_is_test_context() {
    let mut parser = TreeSitterParser::new();
    // A top-level `test_*` function and a nested helper inside it. The helper's
    // own name does NOT start with `test_`; it is test code purely by ancestry,
    // mirroring `nested_fn_inside_test_fn_is_still_test_context` for Rust.
    let source = r#"
def test_login():
    def _inner_helper():
        return 1
    assert _inner_helper() == 1

def build_widget():
    return 2
"#;
    let defs = parser
        .parse_file("python", Path::new("mod_a.py"), source)
        .unwrap()
        .definitions;

    assert!(
        in_test(&defs, "test_login"),
        "pytest test_* is test context"
    );
    assert!(
        in_test(&defs, "_inner_helper"),
        "a helper nested inside a test_* fn is still test context"
    );
    assert!(
        !in_test(&defs, "build_widget"),
        "an ordinary top-level fn is not test context"
    );
}

#[test]
fn python_unittest_testcase_methods_are_test_context() {
    let mut parser = TreeSitterParser::new();
    // Every method of a `unittest.TestCase` subclass is harness-invoked —
    // including non-`test_`-prefixed lifecycle hooks like `setUp`.
    let source = r#"
import unittest

class WidgetTest(unittest.TestCase):
    def setUp(self):
        self.x = 1
    def test_render(self):
        self.assertEqual(self.x, 1)

class Widget:
    def render(self):
        return 1
"#;
    let defs = parser
        .parse_file("python", Path::new("widget.py"), source)
        .unwrap()
        .definitions;

    assert!(
        in_test(&defs, "setUp"),
        "a non-test_ method of a TestCase subclass is test context"
    );
    assert!(in_test(&defs, "test_render"), "TestCase test method too");
    assert!(
        !in_test(&defs, "render"),
        "a method of a plain (non-TestCase) class is not test context"
    );
}

#[test]
fn python_bare_testcase_import_is_test_context() {
    let mut parser = TreeSitterParser::new();
    // A bare `TestCase` base (from `from unittest import TestCase`) still marks.
    let source = r#"
from unittest import TestCase

class T(TestCase):
    def helper(self):
        return 1
"#;
    let defs = parser
        .parse_file("python", Path::new("t.py"), source)
        .unwrap()
        .definitions;
    assert!(
        in_test(&defs, "helper"),
        "bare `TestCase` base marks methods"
    );
}

#[test]
fn typescript_named_helper_inside_describe_is_test_context() {
    let mut parser = TreeSitterParser::new();
    // A NAMED function declared inside a `describe` callback body is test code.
    // The anonymous callback arrow itself is not captured as a Definition by
    // typescript.scm, so only the named helper is asserted here.
    let source = r#"
describe("widget", () => {
  function makeFixture() {
    return 1;
  }
  it("renders", () => {
    expect(makeFixture()).toBe(1);
  });
});

function topLevel() {
  return 2;
}
"#;
    let defs = parser
        .parse_file("typescript", Path::new("widget.ts"), source)
        .unwrap()
        .definitions;

    assert!(
        in_test(&defs, "makeFixture"),
        "a named fn inside a describe block is test context"
    );
    assert!(
        !in_test(&defs, "topLevel"),
        "a top-level fn with no test-block ancestor is not test context"
    );
}

#[test]
fn typescript_named_const_arrow_inside_it_is_test_context() {
    let mut parser = TreeSitterParser::new();
    // A named `const` arrow declared inside an `it` callback body is test code.
    let source = r#"
it("does a thing", () => {
  const setup = () => {
    return 1;
  };
  setup();
});
"#;
    let defs = parser
        .parse_file("typescript", Path::new("thing.test.ts"), source)
        .unwrap()
        .definitions;
    assert!(
        in_test(&defs, "setup"),
        "a named const arrow inside an it() block is test context"
    );
}

#[test]
fn typescript_it_only_member_call_marks_test_context() {
    let mut parser = TreeSitterParser::new();
    // `.only`/`.skip`/`.each` are member calls on `describe`/`it`/`test`; the
    // callee object drives the match.
    let source = r#"
describe.only("suite", () => {
  function helper() {
    return 1;
  }
  helper();
});
"#;
    let defs = parser
        .parse_file("typescript", Path::new("suite.ts"), source)
        .unwrap()
        .definitions;
    assert!(
        in_test(&defs, "helper"),
        "a helper inside describe.only(...) is test context"
    );
}
