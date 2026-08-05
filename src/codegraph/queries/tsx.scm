(function_declaration name: (identifier) @name.function) @definition.function
(call_expression function: (identifier) @name.call) @reference.call
(import_statement source: (string (string_fragment) @name.import))
