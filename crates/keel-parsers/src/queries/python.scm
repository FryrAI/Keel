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
