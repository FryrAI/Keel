; Keel tree-sitter queries for Python
; Captures: @def.func, @def.class, @ref.call, @ref.import

; --- Function definitions ---
(function_definition
  name: (identifier) @def.func.name
  parameters: (parameters) @def.func.params
  return_type: (type)? @def.func.return_type
  body: (block) @def.func.body) @def.func

; --- Class definitions ---
(class_definition
  name: (identifier) @def.class.name
  body: (block) @def.class.body) @def.class

; --- Decorated function/class ---
; Capture the inner function_definition (not the decorated_definition wrapper)
; so the line_start points at `def ...`, not the `@decorator` line.
; Dedup in extract_definitions removes the duplicate from the standalone pattern.
(decorated_definition
  (function_definition
    name: (identifier) @def.func.name
    parameters: (parameters) @def.func.params
    return_type: (type)? @def.func.return_type
    body: (block) @def.func.body) @def.func)

(decorated_definition
  (class_definition
    name: (identifier) @def.class.name
    body: (block) @def.class.body) @def.class)

; --- Function calls ---
(call
  function: (identifier) @ref.call.name) @ref.call

; Method calls
(call
  function: (attribute
    object: (_) @ref.call.receiver
    attribute: (identifier) @ref.call.name)) @ref.call

; --- Import statements ---
(import_statement
  name: (dotted_name) @ref.import.name) @ref.import

; From imports
(import_from_statement
  module_name: (dotted_name) @ref.import.source
  name: (dotted_name) @ref.import.name) @ref.import

(import_from_statement
  module_name: (relative_import) @ref.import.source
  name: (dotted_name) @ref.import.name) @ref.import

; Star/wildcard imports: from X import *
(import_from_statement
  module_name: (dotted_name) @ref.import.source
  (wildcard_import) @ref.import.star) @ref.import

(import_from_statement
  module_name: (relative_import) @ref.import.source
  (wildcard_import) @ref.import.star) @ref.import

; --- Functions named as values (not invoked) ---
; A bare identifier in argument position is a function reference, not a call:
; `sorted(xs, key=rank)`, `register(handler)`. Real usage for W005 dead-code
; analysis, but never a `calls` edge.
(argument_list (identifier) @ref.value.name) @ref.value

; Keyword-argument value: `sorted(xs, key=sort_key)` — the identifier is a
; child of keyword_argument, not of argument_list, so the pattern above misses it.
(keyword_argument value: (identifier) @ref.value.name) @ref.value

; Dispatch-table value: `HANDLERS = {"evt": on_event}`.
(pair value: (identifier) @ref.value.name) @ref.value

; Container element: `STEPS = [validate, publish]`.
(list (identifier) @ref.value.name) @ref.value

; Container element: `STEPS = (validate, publish)`.
(tuple (identifier) @ref.value.name) @ref.value

; Container element: `STEPS = {validate, publish}`.
(set (identifier) @ref.value.name) @ref.value

; Returned function/closure: `return deco`.
(return_statement (identifier) @ref.value.name) @ref.value

; Bare decorator name: `@register` (the call form `@register("evt")` stays a
; Call reference — its decorator child is a `call`, not an identifier).
(decorator (identifier) @ref.value.name) @ref.value

; Alias binding: `handler = my_fn`.
(assignment right: (identifier) @ref.value.name) @ref.value

; --- Boundary dispatch keys (string literals) ---
; Three positions only: call argument, dict key, and `match`-case pattern. Only
; literals whose text exactly equals a known boundary name survive extraction —
; see the same block in rust.scm.
(argument_list (string) @ref.literal.name)

(pair key: (string) @ref.literal.name)

(case_pattern (string) @ref.literal.name)
