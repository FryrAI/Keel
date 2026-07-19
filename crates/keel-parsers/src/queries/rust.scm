; Keel tree-sitter queries for Rust
; Captures: @def.func, @def.struct, @def.impl, @def.trait, @def.macro,
;           @ref.call, @ref.use, @ref.macro_invocation

; --- Function definitions ---
(function_item
  name: (identifier) @def.func.name
  parameters: (parameters) @def.func.params
  return_type: (_)? @def.func.return_type
  body: (block) @def.func.body) @def.func

; --- Struct definitions ---
(struct_item
  name: (type_identifier) @def.struct.name
  body: (_) @def.struct.body) @def.struct

; --- Enum definitions ---
(enum_item
  name: (type_identifier) @def.enum.name
  body: (enum_variant_list) @def.enum.body) @def.enum

; --- Trait definitions ---
(trait_item
  name: (type_identifier) @def.trait.name
  body: (declaration_list) @def.trait.body) @def.trait

; --- Impl blocks ---
(impl_item
  type: (_) @def.impl.type
  body: (declaration_list) @def.impl.body) @def.impl

; Methods inside impl blocks
(impl_item
  body: (declaration_list
    (function_item
      name: (identifier) @def.method.name
      parameters: (parameters) @def.method.params
      return_type: (_)? @def.method.return_type
      body: (block) @def.method.body) @def.method)) @def.method.parent

; --- Function calls ---
(call_expression
  function: (identifier) @ref.call.name) @ref.call

; Method calls
(call_expression
  function: (field_expression
    value: (_) @ref.call.receiver
    field: (field_identifier) @ref.call.name)) @ref.call

; Qualified path calls (e.g., Vec::new())
(call_expression
  function: (scoped_identifier
    path: (_) @ref.call.receiver
    name: (identifier) @ref.call.name)) @ref.call

; --- Use declarations ---
(use_declaration
  argument: (_) @ref.use.path) @ref.use

; --- Macro definitions (macro_rules!) ---
(macro_definition
  name: (identifier) @def.macro.name) @def.macro

; --- Macro invocations (my_vec!()) ---
(macro_invocation
  macro: (identifier) @ref.macro_invocation.name) @ref.macro_invocation

; --- Trait impl blocks (impl Trait for Type) ---
(impl_item
  trait: (_) @def.trait_impl.trait_name
  type: (_) @def.trait_impl.type_name
  body: (declaration_list) @def.trait_impl.body) @def.trait_impl

; --- Mod declarations ---
(mod_item
  name: (identifier) @def.mod.name) @def.mod

; --- Functions named as values (not invoked) ---
; A bare identifier in argument position is a function reference, not a call:
; `.map(render_file)`, `.and_then(first_line)`, `get(health)`. Real usage, so
; W005 dead-code analysis must see it — but it carries no argument list, so it
; is captured as @ref.value and never becomes a `calls` edge.
(arguments (identifier) @ref.value.name) @ref.value

; Function names referenced from attribute strings, e.g. serde's
; `#[serde(default = "default_true")]` / `deserialize_with = "..."`.
(attribute (token_tree (string_literal) @ref.attr.name)) @ref.attr
