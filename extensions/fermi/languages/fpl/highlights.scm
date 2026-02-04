; Highlights for FPL syntax

; Keywords
[
  "forecast"
  "driver"
  "estimate"
] @keyword

; Distribution functions
[
  "triangular"
  "normal"
  "lognormal"
  "uniform"
  "beta"
] @function.builtin

; Operators
[
  "+"
  "-"
  "*"
  "/"
  "^"
  "="
  ">"
  "<"
  ">="
  "<="
  "!="
] @operator

; Punctuation
[
  "("
  ")"
  "{"
  "}"
  ","
] @punctuation.bracket

; Literals
(number) @number
(probability) @number.special
(string) @string

; Identifiers
(identifier) @variable

; Function calls
(function_call
  function: (identifier) @function)

; Comments
(comment) @comment

; Forecast title
(forecast_statement
  title: (string) @string.special)

; Driver name
(driver_statement
  name: (identifier) @variable.parameter)
