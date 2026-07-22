; Keel tree-sitter queries: JSX element references (TSX grammar only)
; Captures: @ref.jsx, @ref.jsx.name, plus @ref.value/@ref.value.name (shared
; with typescript.scm) for identifiers inside jsx_expression containers.
;
; Compiled ONLY against the TSX grammar (see queries/mod.rs) — `jsx_*` node
; kinds do not exist in the plain TypeScript grammar, so this must never be
; folded into typescript.scm.
;
; A component used only as `<Comp />` produces no call/value reference under
; typescript.scm and reads as W005 dead code. These patterns capture the
; element name so JSX usage counts as a reference. Only the opening tag is
; captured (`jsx_opening_element` / `jsx_self_closing_element`) — the closing
; tag of a paired element is deliberately not, to avoid double-counting.
;
; Intrinsic HTML elements (`<div>`) start lowercase; tree-sitter has no
; `#match?` support in this extractor, so the uppercase-vs-lowercase filter is
; applied in Rust (treesitter/mod.rs), not here.

(jsx_self_closing_element
  name: (identifier) @ref.jsx.name) @ref.jsx

(jsx_opening_element
  name: (identifier) @ref.jsx.name) @ref.jsx

; Namespaced/member form: <Foo.Bar />, <Foo.Bar>...</Foo.Bar> — the whole
; dotted expression is captured as one name (`Foo.Bar`).
(jsx_self_closing_element
  name: (member_expression) @ref.jsx.name) @ref.jsx

(jsx_opening_element
  name: (member_expression) @ref.jsx.name) @ref.jsx

; The root of a namespaced name is a usage in its own right (e.g. `Foo` used
; elsewhere as `<Foo.Bar />` where `Foo` is the imported namespace) — captured
; alongside the full dotted name above when trivially available (a bare
; identifier object; deeper chains like `Foo.Bar.Baz` are not unpacked).
(jsx_self_closing_element
  name: (member_expression
    object: (identifier) @ref.jsx.name))

(jsx_opening_element
  name: (member_expression
    object: (identifier) @ref.jsx.name))

; --- Bare identifiers inside a `{...}` JSX expression container ---
; Covers BOTH an attribute value (`<C onClick={handler} />`) and a JSX child
; (`<div>{child}</div>`) with one pattern: `jsx_expression` (the `{...}`
; container) is the node kind for both positions — it never appears anywhere
; else in the grammar (plain `{}` in JS parses as object/block, not this node)
; — and here it wraps nothing but a bare identifier.
;
; `jsx_attribute` has no `name`/`value` fields (verified against the vendored
; TSX grammar's node-types.json and a parse dump): its children are just the
; attribute-name `property_identifier` followed by the value node, so scoping
; to `jsx_expression`'s own identifier child is what keeps the attribute NAME
; out of this capture, not a field constraint.
;
; Reuses the shared `ref.value` / `ref.value.name` capture names (see
; typescript.scm) — the treesitter walker already turns these into
; `ReferenceKind::Value` refs with no uppercase filter, which is correct here:
; `handler` is a legitimate lowercase reference, unlike a `ref.jsx.name`
; element name.
;
; A member-expression value (`{obj.handler}`) or anything else non-bare
; (a call, an arrow function, ...) does not match `(identifier)` here and is
; deliberately left uncaptured by this pattern.
(jsx_expression (identifier) @ref.value.name) @ref.value
