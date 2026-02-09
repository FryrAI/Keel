; Keel tree-sitter queries for TypeScript/JavaScript
; Captures: @def.func, @def.class, @def.method, @ref.call, @ref.import

; --- Function definitions ---
(function_declaration
  name: (identifier) @def.func.name
  parameters: (formal_parameters) @def.func.params
  return_type: (type_annotation)? @def.func.return_type
  body: (statement_block) @def.func.body) @def.func

; Arrow functions assigned to const/let/var
(lexical_declaration
  (variable_declarator
    name: (identifier) @def.func.name
    value: (arrow_function
      parameters: (formal_parameters) @def.func.params
      return_type: (type_annotation)? @def.func.return_type
      body: (_) @def.func.body))) @def.func

; --- Class definitions ---
(class_declaration
  name: (type_identifier) @def.class.name
  body: (class_body) @def.class.body) @def.class

; --- Method definitions ---
(method_definition
  name: (property_identifier) @def.method.name
  parameters: (formal_parameters) @def.method.params
  return_type: (type_annotation)? @def.method.return_type
  body: (statement_block) @def.method.body) @def.method

; --- Function calls ---
(call_expression
  function: (identifier) @ref.call.name) @ref.call

; Method calls
(call_expression
  function: (member_expression
    object: (_) @ref.call.receiver
    property: (property_identifier) @ref.call.name)) @ref.call

; --- Import statements ---
; Import with named imports: import { X, Y } from 'source'
(import_statement
  (import_clause
    (named_imports
      (import_specifier
        name: (identifier) @ref.import.name)))
  source: (string) @ref.import.source) @ref.import

; Default import: import X from 'source'
(import_statement
  (import_clause
    (identifier) @ref.import.name)
  source: (string) @ref.import.source) @ref.import

; Side-effect import: import 'source' (no names)
(import_statement
  source: (string) @ref.import.source) @ref.import

; --- Export statements ---
(export_statement) @def.export
