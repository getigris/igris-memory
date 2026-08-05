(function_declaration name: (identifier) @name.function) @definition.function
(call_expression function: (identifier) @name.call) @reference.call
(import_spec path: (interpreted_string_literal) @name.import)
