/**
 * Tree-sitter grammar for FPL (Forecasting Programming Language)
 *
 * This grammar defines the syntax structure for Fermi forecasts.
 */

module.exports = grammar({
  name: 'fpl',

  extras: $ => [
    /\s/,
    $.comment,
  ],

  rules: {
    source_file: $ => repeat($._statement),

    _statement: $ => choice(
      $.forecast_statement,
      $.driver_statement,
      $.estimate_statement,
    ),

    forecast_statement: $ => seq(
      'forecast',
      field('title', $.string),
      field('body', $.block),
    ),

    block: $ => seq(
      '{',
      repeat($._statement),
      '}',
    ),

    driver_statement: $ => seq(
      'driver',
      field('name', $.identifier),
      field('distribution', $.distribution),
    ),

    estimate_statement: $ => seq(
      'estimate',
      field('expression', $.expression),
    ),

    distribution: $ => choice(
      $.triangular_distribution,
      $.normal_distribution,
      $.lognormal_distribution,
      $.uniform_distribution,
      $.beta_distribution,
    ),

    triangular_distribution: $ => seq(
      'triangular',
      '(',
      field('p5', $.expression),
      ',',
      field('p50', $.expression),
      ',',
      field('p95', $.expression),
      ')',
    ),

    normal_distribution: $ => seq(
      'normal',
      '(',
      field('mean', $.expression),
      ',',
      field('stddev', $.expression),
      ')',
    ),

    lognormal_distribution: $ => seq(
      'lognormal',
      '(',
      field('median', $.expression),
      ',',
      field('sigma', $.expression),
      ')',
    ),

    uniform_distribution: $ => seq(
      'uniform',
      '(',
      field('low', $.expression),
      ',',
      field('high', $.expression),
      ')',
    ),

    beta_distribution: $ => seq(
      'beta',
      '(',
      field('alpha', $.expression),
      ',',
      field('beta', $.expression),
      optional(seq(
        ',',
        field('min', $.expression),
        ',',
        field('max', $.expression),
      )),
      ')',
    ),

    expression: $ => choice(
      $.binary_expression,
      $.unary_expression,
      $.function_call,
      $.identifier,
      $.number,
      $.probability,
      $.parenthesized_expression,
    ),

    binary_expression: $ => choice(
      prec.left(4, seq(field('left', $.expression), '*', field('right', $.expression))),
      prec.left(4, seq(field('left', $.expression), '/', field('right', $.expression))),
      prec.left(3, seq(field('left', $.expression), '+', field('right', $.expression))),
      prec.left(3, seq(field('left', $.expression), '-', field('right', $.expression))),
      prec.left(2, seq(field('left', $.expression), '^', field('right', $.expression))),
    ),

    unary_expression: $ => prec(5, seq(
      choice('-', '!'),
      field('operand', $.expression),
    )),

    function_call: $ => seq(
      field('function', $.identifier),
      '(',
      optional(seq(
        field('argument', $.expression),
        repeat(seq(',', field('argument', $.expression))),
      )),
      ')',
    ),

    parenthesized_expression: $ => seq(
      '(',
      $.expression,
      ')',
    ),

    // Terminals
    identifier: $ => /[a-zA-Z_][a-zA-Z0-9_]*/,

    number: $ => /\d+(\.\d+)?/,

    probability: $ => choice(
      /p\d+/,           // p50, p95
      /\d+%/,           // 50%, 95%
    ),

    string: $ => seq(
      '"',
      repeat(choice(
        /[^"\\]/,
        seq('\\', /./),
      )),
      '"',
    ),

    comment: $ => token(choice(
      seq('//', /.*/),
      seq('/*', /[^*]*\*+([^/*][^*]*\*+)*/, '/'),
    )),
  },
});
