; Keel tree-sitter queries for Go
; Captures: @def.func, @def.method, @def.type, @ref.call, @ref.import

; --- Function definitions ---
(function_declaration
  name: (identifier) @def.func.name
  parameters: (parameter_list) @def.func.params
  result: (_)? @def.func.return_type
  body: (block) @def.func.body) @def.func

; --- Method definitions (receiver functions) ---
(method_declaration
  receiver: (parameter_list) @def.method.receiver
  name: (field_identifier) @def.method.name
  parameters: (parameter_list) @def.method.params
  result: (_)? @def.method.return_type
  body: (block) @def.method.body) @def.method

; --- Type definitions (struct, interface) ---
(type_declaration
  (type_spec
    name: (type_identifier) @def.type.name
    type: (struct_type) @def.type.body)) @def.type

(type_declaration
  (type_spec
    name: (type_identifier) @def.type.name
    type: (interface_type) @def.type.body)) @def.type

; --- Function calls ---
(call_expression
  function: (identifier) @ref.call.name) @ref.call

; Method calls / qualified calls
(call_expression
  function: (selector_expression
    operand: (_) @ref.call.receiver
    field: (field_identifier) @ref.call.name)) @ref.call

; --- Import statements ---
(import_declaration
  (import_spec
    path: (interpreted_string_literal) @ref.import.source)) @ref.import

(import_declaration
  (import_spec_list
    (import_spec
      path: (interpreted_string_literal) @ref.import.source))) @ref.import

; Import with alias
(import_declaration
  (import_spec_list
    (import_spec
      name: (package_identifier) @ref.import.name
      path: (interpreted_string_literal) @ref.import.source))) @ref.import

; Blank import (side-effect only) — single
(import_declaration
  (import_spec
    name: (blank_identifier) @ref.import.blank
    path: (interpreted_string_literal) @ref.import.source)) @ref.import

; Blank import (side-effect only) — grouped
(import_declaration
  (import_spec_list
    (import_spec
      name: (blank_identifier) @ref.import.blank
      path: (interpreted_string_literal) @ref.import.source))) @ref.import

; Dot import (all exported names into scope) — single
(import_declaration
  (import_spec
    name: (dot) @ref.import.dot
    path: (interpreted_string_literal) @ref.import.source)) @ref.import

; Dot import (all exported names into scope) — grouped
(import_declaration
  (import_spec_list
    (import_spec
      name: (dot) @ref.import.dot
      path: (interpreted_string_literal) @ref.import.source))) @ref.import
