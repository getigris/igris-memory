(function_declaration name: (identifier) @name.function) @definition.function
(call_expression function: (identifier) @name.call) @reference.call
(call_expression
  function: (identifier) @_fn
  arguments: (arguments (string (string_fragment) @name.import))
  (#eq? @_fn "require"))
