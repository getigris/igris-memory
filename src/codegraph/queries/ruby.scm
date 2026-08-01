(method name: (identifier) @name.function) @definition.function
(call method: (identifier) @name.call) @reference.call
(call
  method: (identifier) @_fn
  arguments: (argument_list (string (string_content) @name.import))
  (#eq? @_fn "require"))
