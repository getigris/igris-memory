; Function/method definitions
(function_item name: (identifier) @name.function) @definition.function

; Calls
(call_expression
  function: (identifier) @name.call) @reference.call

; Imports
(use_declaration argument: (_) @name.import) @reference.import
