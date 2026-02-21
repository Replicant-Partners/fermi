; Statement keywords - match as part of statement nodes
(question_statement "question" @keyword.control)
(driver_statement "driver" @keyword.control)
(evidence_statement "evidence" @keyword.control)
(agent_statement "agent" @keyword.control)
(model_statement "model" @keyword.control)
(simulate_statement "simulate" @keyword.control)
(simulate_statement "iterations" @keyword.control)

; Control flow keywords
(conditional_expression "if" @keyword.control)
(conditional_expression "then" @keyword.control)
(conditional_expression "else" @keyword.control)

; Driver type keywords
(driver_statement type: ["continuous" "binary" "discrete"] @keyword.control)

; Property keywords
(distribution_property "distribution" @keyword.control)
(probability_property "probability" @keyword.control)
(unit_property "unit" @keyword.control)
(rationale_property "rationale" @keyword.control)
(impact_multiplier_property "impact_multiplier" @keyword.control)

; Evidence properties
(evidence_property "source" @keyword.control)
(evidence_property "summary" @keyword.control)
(evidence_property "relevance" @keyword.control)
(evidence_property "date" @keyword.control)

; Agent properties
(agent_property "query" @keyword.control)
(agent_property "schedule" @keyword.control)
(agent_property "every" @keyword.control)

; Question properties
(question_property "target_date" @keyword.control)
(question_property "resolution_criteria" @keyword.control)

; Base rate keywords
(base_rate_property "base_rate" @keyword.control)
(base_rate_field "reference_class" @keyword.control)
(base_rate_field "historical_frequency" @keyword.control)
(base_rate_field "sample_size" @keyword.control)
(base_rate_field "source" @keyword.control)
(base_rate_field "reasoning" @keyword.control)
(base_rate_field "generated_by" @keyword.control)

; Base rate values
(base_rate_field value: "human" @constant.builtin)

; Distribution function names
(triangular_distribution "triangular" @function.builtin)
(normal_distribution "normal" @function.builtin)
(lognormal_distribution "lognormal" @function.builtin)
(uniform_distribution "uniform" @function.builtin)
(beta_distribution "beta" @function.builtin)

; Operators
[
  "+"
  "-"
  "*"
  "/"
  "^"
  "!"
] @operator

; Punctuation
[
  ":"
  ","
  "("
  ")"
  "{"
  "}"
] @punctuation.delimiter

; Time units
[
  "day"
  "days"
  "week"
  "weeks"
] @constant.builtin

; Field names (for drivers, evidence, etc.) - specific rules before general
(driver_statement
  name: (identifier) @variable.parameter)

(evidence_statement
  name: (identifier) @variable.parameter)

(agent_statement
  name: (identifier) @variable.parameter)

; Built-in functions
(function_call
  function: (identifier) @function)

; Literals - these come last to avoid overriding more specific rules
(string) @string
(number) @number
(probability) @number.float
(date) @string.special

; Comments
(comment) @comment

; Identifiers - last as catch-all
(identifier) @variable
