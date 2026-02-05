/**
 * Tree-sitter grammar for FPL (Forecasting Programming Language)
 *
 * This grammar defines the syntax structure for Fermi forecasts.
 */

module.exports = grammar({
  name: "fpl",

  extras: ($) => [/\s/, $.comment],

  rules: {
    source_file: ($) => repeat($._statement),

    _statement: ($) =>
      choice(
        $.question_statement,
        $.driver_statement,
        $.evidence_statement,
        $.agent_statement,
        $.model_statement,
        $.simulate_statement,
      ),

    question_statement: ($) => seq("question", field("text", $.string)),

    driver_statement: ($) =>
      seq(
        "driver",
        field("name", $.identifier),
        field("type", choice("continuous", "binary", "discrete")),
        field("body", $.driver_block),
      ),

    driver_block: ($) => seq("{", repeat($.driver_property), "}"),

    driver_property: ($) =>
      choice(
        $.distribution_property,
        $.probability_property,
        $.unit_property,
        $.rationale_property,
        $.impact_multiplier_property,
      ),

    distribution_property: ($) =>
      seq("distribution", ":", field("distribution", $.distribution)),

    probability_property: ($) =>
      seq("probability", ":", field("value", choice($.probability, $.number))),

    unit_property: ($) => seq("unit", ":", field("value", $.string)),

    rationale_property: ($) => seq("rationale", ":", field("value", $.string)),

    impact_multiplier_property: ($) =>
      seq("impact_multiplier", ":", field("value", $.number)),

    evidence_statement: ($) =>
      seq(
        "evidence",
        field("name", $.identifier),
        field("body", $.evidence_block),
      ),

    evidence_block: ($) => seq("{", repeat($.evidence_property), "}"),

    evidence_property: ($) =>
      choice(
        seq("source", ":", field("value", $.string)),
        seq("summary", ":", field("value", $.string)),
        seq("relevance", ":", field("value", choice($.probability, $.number))),
        seq("date", ":", field("value", $.date)),
      ),

    agent_statement: ($) =>
      seq("agent", field("name", $.identifier), field("body", $.agent_block)),

    agent_block: ($) => seq("{", repeat($.agent_property), "}"),

    agent_property: ($) =>
      choice(
        seq("query", ":", field("value", $.string)),
        seq(
          "schedule",
          ":",
          "every",
          field("interval", $.number),
          field("unit", choice("day", "days", "week", "weeks")),
        ),
      ),

    model_statement: ($) =>
      seq("model", ":", field("expression", $.expression)),

    simulate_statement: ($) =>
      seq("simulate", field("iterations", $.number), "iterations"),

    distribution: ($) =>
      choice(
        $.triangular_distribution,
        $.normal_distribution,
        $.lognormal_distribution,
        $.uniform_distribution,
        $.beta_distribution,
      ),

    triangular_distribution: ($) =>
      seq(
        "triangular",
        "(",
        field("p5", $.expression),
        ",",
        field("p50", $.expression),
        ",",
        field("p95", $.expression),
        ")",
      ),

    normal_distribution: ($) =>
      seq(
        "normal",
        "(",
        field("mean", $.expression),
        ",",
        field("stddev", $.expression),
        ")",
      ),

    lognormal_distribution: ($) =>
      seq(
        "lognormal",
        "(",
        field("median", $.expression),
        ",",
        field("sigma", $.expression),
        ")",
      ),

    uniform_distribution: ($) =>
      seq(
        "uniform",
        "(",
        field("low", $.expression),
        ",",
        field("high", $.expression),
        ")",
      ),

    beta_distribution: ($) =>
      seq(
        "beta",
        "(",
        field("alpha", $.expression),
        ",",
        field("beta_param", $.expression),
        optional(
          seq(",", field("min", $.expression), ",", field("max", $.expression)),
        ),
        ")",
      ),

    expression: ($) =>
      choice(
        $.binary_expression,
        $.unary_expression,
        $.conditional_expression,
        $.function_call,
        $.identifier,
        $.number,
        $.probability,
        $.parenthesized_expression,
      ),

    binary_expression: ($) =>
      choice(
        prec.left(
          4,
          seq(field("left", $.expression), "*", field("right", $.expression)),
        ),
        prec.left(
          4,
          seq(field("left", $.expression), "/", field("right", $.expression)),
        ),
        prec.left(
          3,
          seq(field("left", $.expression), "+", field("right", $.expression)),
        ),
        prec.left(
          3,
          seq(field("left", $.expression), "-", field("right", $.expression)),
        ),
        prec.left(
          2,
          seq(field("left", $.expression), "^", field("right", $.expression)),
        ),
      ),

    unary_expression: ($) =>
      prec(5, seq(choice("-", "!"), field("operand", $.expression))),

    conditional_expression: ($) =>
      prec.right(
        1,
        seq(
          "if",
          field("condition", $.expression),
          "then",
          field("then_expr", $.expression),
          "else",
          field("else_expr", $.expression),
        ),
      ),

    function_call: ($) =>
      seq(
        field("function", $.identifier),
        "(",
        optional(
          seq(
            field("argument", $.expression),
            repeat(seq(",", field("argument", $.expression))),
          ),
        ),
        ")",
      ),

    parenthesized_expression: ($) => seq("(", $.expression, ")"),

    // Terminals
    identifier: ($) => /[a-zA-Z_][a-zA-Z0-9_]*/,

    number: ($) => /\d+(\.\d+)?/,

    probability: ($) =>
      choice(
        /\d+\.\d+p/, // 0.65p
        /p\d+/, // p50, p95
        /\d+%/, // 50%, 95%
      ),

    date: ($) => /\d{4}-\d{2}-\d{2}/,

    string: ($) => seq('"', repeat(choice(/[^"\\]/, seq("\\", /./))), '"'),

    comment: ($) =>
      token(
        choice(
          seq("//", /.*/),
          seq("#", /.*/),
          seq("/*", /[^*]*\*+([^/*][^*]*\*+)*/, "/"),
        ),
      ),
  },
});
