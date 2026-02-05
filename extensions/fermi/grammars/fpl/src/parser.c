#include "tree_sitter/parser.h"

#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic ignored "-Wmissing-field-initializers"
#endif

#define LANGUAGE_VERSION 14
#define STATE_COUNT 143
#define LARGE_STATE_COUNT 2
#define SYMBOL_COUNT 97
#define ALIAS_COUNT 0
#define TOKEN_COUNT 58
#define EXTERNAL_TOKEN_COUNT 0
#define FIELD_COUNT 31
#define MAX_ALIAS_SEQUENCE_LENGTH 10
#define PRODUCTION_ID_COUNT 23

enum ts_symbol_identifiers {
  anon_sym_question = 1,
  anon_sym_driver = 2,
  anon_sym_continuous = 3,
  anon_sym_binary = 4,
  anon_sym_discrete = 5,
  anon_sym_LBRACE = 6,
  anon_sym_RBRACE = 7,
  anon_sym_distribution = 8,
  anon_sym_COLON = 9,
  anon_sym_probability = 10,
  anon_sym_unit = 11,
  anon_sym_rationale = 12,
  anon_sym_impact_multiplier = 13,
  anon_sym_evidence = 14,
  anon_sym_source = 15,
  anon_sym_summary = 16,
  anon_sym_relevance = 17,
  anon_sym_date = 18,
  anon_sym_agent = 19,
  anon_sym_query = 20,
  anon_sym_schedule = 21,
  anon_sym_every = 22,
  anon_sym_day = 23,
  anon_sym_days = 24,
  anon_sym_week = 25,
  anon_sym_weeks = 26,
  anon_sym_model = 27,
  anon_sym_simulate = 28,
  anon_sym_iterations = 29,
  anon_sym_triangular = 30,
  anon_sym_LPAREN = 31,
  anon_sym_COMMA = 32,
  anon_sym_RPAREN = 33,
  anon_sym_normal = 34,
  anon_sym_lognormal = 35,
  anon_sym_uniform = 36,
  anon_sym_beta = 37,
  anon_sym_STAR = 38,
  anon_sym_SLASH = 39,
  anon_sym_PLUS = 40,
  anon_sym_DASH = 41,
  anon_sym_CARET = 42,
  anon_sym_BANG = 43,
  anon_sym_if = 44,
  anon_sym_then = 45,
  anon_sym_else = 46,
  sym_identifier = 47,
  sym_number = 48,
  aux_sym_probability_token1 = 49,
  aux_sym_probability_token2 = 50,
  aux_sym_probability_token3 = 51,
  sym_date = 52,
  anon_sym_DQUOTE = 53,
  aux_sym_string_token1 = 54,
  anon_sym_BSLASH = 55,
  aux_sym_string_token2 = 56,
  sym_comment = 57,
  sym_source_file = 58,
  sym__statement = 59,
  sym_question_statement = 60,
  sym_driver_statement = 61,
  sym_driver_block = 62,
  sym_driver_property = 63,
  sym_distribution_property = 64,
  sym_probability_property = 65,
  sym_unit_property = 66,
  sym_rationale_property = 67,
  sym_impact_multiplier_property = 68,
  sym_evidence_statement = 69,
  sym_evidence_block = 70,
  sym_evidence_property = 71,
  sym_agent_statement = 72,
  sym_agent_block = 73,
  sym_agent_property = 74,
  sym_model_statement = 75,
  sym_simulate_statement = 76,
  sym_distribution = 77,
  sym_triangular_distribution = 78,
  sym_normal_distribution = 79,
  sym_lognormal_distribution = 80,
  sym_uniform_distribution = 81,
  sym_beta_distribution = 82,
  sym_expression = 83,
  sym_binary_expression = 84,
  sym_unary_expression = 85,
  sym_conditional_expression = 86,
  sym_function_call = 87,
  sym_parenthesized_expression = 88,
  sym_probability = 89,
  sym_string = 90,
  aux_sym_source_file_repeat1 = 91,
  aux_sym_driver_block_repeat1 = 92,
  aux_sym_evidence_block_repeat1 = 93,
  aux_sym_agent_block_repeat1 = 94,
  aux_sym_function_call_repeat1 = 95,
  aux_sym_string_repeat1 = 96,
};

static const char * const ts_symbol_names[] = {
  [ts_builtin_sym_end] = "end",
  [anon_sym_question] = "question",
  [anon_sym_driver] = "driver",
  [anon_sym_continuous] = "continuous",
  [anon_sym_binary] = "binary",
  [anon_sym_discrete] = "discrete",
  [anon_sym_LBRACE] = "{",
  [anon_sym_RBRACE] = "}",
  [anon_sym_distribution] = "distribution",
  [anon_sym_COLON] = ":",
  [anon_sym_probability] = "probability",
  [anon_sym_unit] = "unit",
  [anon_sym_rationale] = "rationale",
  [anon_sym_impact_multiplier] = "impact_multiplier",
  [anon_sym_evidence] = "evidence",
  [anon_sym_source] = "source",
  [anon_sym_summary] = "summary",
  [anon_sym_relevance] = "relevance",
  [anon_sym_date] = "date",
  [anon_sym_agent] = "agent",
  [anon_sym_query] = "query",
  [anon_sym_schedule] = "schedule",
  [anon_sym_every] = "every",
  [anon_sym_day] = "day",
  [anon_sym_days] = "days",
  [anon_sym_week] = "week",
  [anon_sym_weeks] = "weeks",
  [anon_sym_model] = "model",
  [anon_sym_simulate] = "simulate",
  [anon_sym_iterations] = "iterations",
  [anon_sym_triangular] = "triangular",
  [anon_sym_LPAREN] = "(",
  [anon_sym_COMMA] = ",",
  [anon_sym_RPAREN] = ")",
  [anon_sym_normal] = "normal",
  [anon_sym_lognormal] = "lognormal",
  [anon_sym_uniform] = "uniform",
  [anon_sym_beta] = "beta",
  [anon_sym_STAR] = "*",
  [anon_sym_SLASH] = "/",
  [anon_sym_PLUS] = "+",
  [anon_sym_DASH] = "-",
  [anon_sym_CARET] = "^",
  [anon_sym_BANG] = "!",
  [anon_sym_if] = "if",
  [anon_sym_then] = "then",
  [anon_sym_else] = "else",
  [sym_identifier] = "identifier",
  [sym_number] = "number",
  [aux_sym_probability_token1] = "probability_token1",
  [aux_sym_probability_token2] = "probability_token2",
  [aux_sym_probability_token3] = "probability_token3",
  [sym_date] = "date",
  [anon_sym_DQUOTE] = "\"",
  [aux_sym_string_token1] = "string_token1",
  [anon_sym_BSLASH] = "\\",
  [aux_sym_string_token2] = "string_token2",
  [sym_comment] = "comment",
  [sym_source_file] = "source_file",
  [sym__statement] = "_statement",
  [sym_question_statement] = "question_statement",
  [sym_driver_statement] = "driver_statement",
  [sym_driver_block] = "driver_block",
  [sym_driver_property] = "driver_property",
  [sym_distribution_property] = "distribution_property",
  [sym_probability_property] = "probability_property",
  [sym_unit_property] = "unit_property",
  [sym_rationale_property] = "rationale_property",
  [sym_impact_multiplier_property] = "impact_multiplier_property",
  [sym_evidence_statement] = "evidence_statement",
  [sym_evidence_block] = "evidence_block",
  [sym_evidence_property] = "evidence_property",
  [sym_agent_statement] = "agent_statement",
  [sym_agent_block] = "agent_block",
  [sym_agent_property] = "agent_property",
  [sym_model_statement] = "model_statement",
  [sym_simulate_statement] = "simulate_statement",
  [sym_distribution] = "distribution",
  [sym_triangular_distribution] = "triangular_distribution",
  [sym_normal_distribution] = "normal_distribution",
  [sym_lognormal_distribution] = "lognormal_distribution",
  [sym_uniform_distribution] = "uniform_distribution",
  [sym_beta_distribution] = "beta_distribution",
  [sym_expression] = "expression",
  [sym_binary_expression] = "binary_expression",
  [sym_unary_expression] = "unary_expression",
  [sym_conditional_expression] = "conditional_expression",
  [sym_function_call] = "function_call",
  [sym_parenthesized_expression] = "parenthesized_expression",
  [sym_probability] = "probability",
  [sym_string] = "string",
  [aux_sym_source_file_repeat1] = "source_file_repeat1",
  [aux_sym_driver_block_repeat1] = "driver_block_repeat1",
  [aux_sym_evidence_block_repeat1] = "evidence_block_repeat1",
  [aux_sym_agent_block_repeat1] = "agent_block_repeat1",
  [aux_sym_function_call_repeat1] = "function_call_repeat1",
  [aux_sym_string_repeat1] = "string_repeat1",
};

static const TSSymbol ts_symbol_map[] = {
  [ts_builtin_sym_end] = ts_builtin_sym_end,
  [anon_sym_question] = anon_sym_question,
  [anon_sym_driver] = anon_sym_driver,
  [anon_sym_continuous] = anon_sym_continuous,
  [anon_sym_binary] = anon_sym_binary,
  [anon_sym_discrete] = anon_sym_discrete,
  [anon_sym_LBRACE] = anon_sym_LBRACE,
  [anon_sym_RBRACE] = anon_sym_RBRACE,
  [anon_sym_distribution] = anon_sym_distribution,
  [anon_sym_COLON] = anon_sym_COLON,
  [anon_sym_probability] = anon_sym_probability,
  [anon_sym_unit] = anon_sym_unit,
  [anon_sym_rationale] = anon_sym_rationale,
  [anon_sym_impact_multiplier] = anon_sym_impact_multiplier,
  [anon_sym_evidence] = anon_sym_evidence,
  [anon_sym_source] = anon_sym_source,
  [anon_sym_summary] = anon_sym_summary,
  [anon_sym_relevance] = anon_sym_relevance,
  [anon_sym_date] = anon_sym_date,
  [anon_sym_agent] = anon_sym_agent,
  [anon_sym_query] = anon_sym_query,
  [anon_sym_schedule] = anon_sym_schedule,
  [anon_sym_every] = anon_sym_every,
  [anon_sym_day] = anon_sym_day,
  [anon_sym_days] = anon_sym_days,
  [anon_sym_week] = anon_sym_week,
  [anon_sym_weeks] = anon_sym_weeks,
  [anon_sym_model] = anon_sym_model,
  [anon_sym_simulate] = anon_sym_simulate,
  [anon_sym_iterations] = anon_sym_iterations,
  [anon_sym_triangular] = anon_sym_triangular,
  [anon_sym_LPAREN] = anon_sym_LPAREN,
  [anon_sym_COMMA] = anon_sym_COMMA,
  [anon_sym_RPAREN] = anon_sym_RPAREN,
  [anon_sym_normal] = anon_sym_normal,
  [anon_sym_lognormal] = anon_sym_lognormal,
  [anon_sym_uniform] = anon_sym_uniform,
  [anon_sym_beta] = anon_sym_beta,
  [anon_sym_STAR] = anon_sym_STAR,
  [anon_sym_SLASH] = anon_sym_SLASH,
  [anon_sym_PLUS] = anon_sym_PLUS,
  [anon_sym_DASH] = anon_sym_DASH,
  [anon_sym_CARET] = anon_sym_CARET,
  [anon_sym_BANG] = anon_sym_BANG,
  [anon_sym_if] = anon_sym_if,
  [anon_sym_then] = anon_sym_then,
  [anon_sym_else] = anon_sym_else,
  [sym_identifier] = sym_identifier,
  [sym_number] = sym_number,
  [aux_sym_probability_token1] = aux_sym_probability_token1,
  [aux_sym_probability_token2] = aux_sym_probability_token2,
  [aux_sym_probability_token3] = aux_sym_probability_token3,
  [sym_date] = sym_date,
  [anon_sym_DQUOTE] = anon_sym_DQUOTE,
  [aux_sym_string_token1] = aux_sym_string_token1,
  [anon_sym_BSLASH] = anon_sym_BSLASH,
  [aux_sym_string_token2] = aux_sym_string_token2,
  [sym_comment] = sym_comment,
  [sym_source_file] = sym_source_file,
  [sym__statement] = sym__statement,
  [sym_question_statement] = sym_question_statement,
  [sym_driver_statement] = sym_driver_statement,
  [sym_driver_block] = sym_driver_block,
  [sym_driver_property] = sym_driver_property,
  [sym_distribution_property] = sym_distribution_property,
  [sym_probability_property] = sym_probability_property,
  [sym_unit_property] = sym_unit_property,
  [sym_rationale_property] = sym_rationale_property,
  [sym_impact_multiplier_property] = sym_impact_multiplier_property,
  [sym_evidence_statement] = sym_evidence_statement,
  [sym_evidence_block] = sym_evidence_block,
  [sym_evidence_property] = sym_evidence_property,
  [sym_agent_statement] = sym_agent_statement,
  [sym_agent_block] = sym_agent_block,
  [sym_agent_property] = sym_agent_property,
  [sym_model_statement] = sym_model_statement,
  [sym_simulate_statement] = sym_simulate_statement,
  [sym_distribution] = sym_distribution,
  [sym_triangular_distribution] = sym_triangular_distribution,
  [sym_normal_distribution] = sym_normal_distribution,
  [sym_lognormal_distribution] = sym_lognormal_distribution,
  [sym_uniform_distribution] = sym_uniform_distribution,
  [sym_beta_distribution] = sym_beta_distribution,
  [sym_expression] = sym_expression,
  [sym_binary_expression] = sym_binary_expression,
  [sym_unary_expression] = sym_unary_expression,
  [sym_conditional_expression] = sym_conditional_expression,
  [sym_function_call] = sym_function_call,
  [sym_parenthesized_expression] = sym_parenthesized_expression,
  [sym_probability] = sym_probability,
  [sym_string] = sym_string,
  [aux_sym_source_file_repeat1] = aux_sym_source_file_repeat1,
  [aux_sym_driver_block_repeat1] = aux_sym_driver_block_repeat1,
  [aux_sym_evidence_block_repeat1] = aux_sym_evidence_block_repeat1,
  [aux_sym_agent_block_repeat1] = aux_sym_agent_block_repeat1,
  [aux_sym_function_call_repeat1] = aux_sym_function_call_repeat1,
  [aux_sym_string_repeat1] = aux_sym_string_repeat1,
};

static const TSSymbolMetadata ts_symbol_metadata[] = {
  [ts_builtin_sym_end] = {
    .visible = false,
    .named = true,
  },
  [anon_sym_question] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_driver] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_continuous] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_binary] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_discrete] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_LBRACE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_RBRACE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_distribution] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_COLON] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_probability] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_unit] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_rationale] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_impact_multiplier] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_evidence] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_source] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_summary] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_relevance] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_date] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_agent] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_query] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_schedule] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_every] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_day] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_days] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_week] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_weeks] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_model] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_simulate] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_iterations] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_triangular] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_LPAREN] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_COMMA] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_RPAREN] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_normal] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_lognormal] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_uniform] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_beta] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_STAR] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_SLASH] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_PLUS] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_DASH] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_CARET] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_BANG] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_if] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_then] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_else] = {
    .visible = true,
    .named = false,
  },
  [sym_identifier] = {
    .visible = true,
    .named = true,
  },
  [sym_number] = {
    .visible = true,
    .named = true,
  },
  [aux_sym_probability_token1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_probability_token2] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_probability_token3] = {
    .visible = false,
    .named = false,
  },
  [sym_date] = {
    .visible = true,
    .named = true,
  },
  [anon_sym_DQUOTE] = {
    .visible = true,
    .named = false,
  },
  [aux_sym_string_token1] = {
    .visible = false,
    .named = false,
  },
  [anon_sym_BSLASH] = {
    .visible = true,
    .named = false,
  },
  [aux_sym_string_token2] = {
    .visible = false,
    .named = false,
  },
  [sym_comment] = {
    .visible = true,
    .named = true,
  },
  [sym_source_file] = {
    .visible = true,
    .named = true,
  },
  [sym__statement] = {
    .visible = false,
    .named = true,
  },
  [sym_question_statement] = {
    .visible = true,
    .named = true,
  },
  [sym_driver_statement] = {
    .visible = true,
    .named = true,
  },
  [sym_driver_block] = {
    .visible = true,
    .named = true,
  },
  [sym_driver_property] = {
    .visible = true,
    .named = true,
  },
  [sym_distribution_property] = {
    .visible = true,
    .named = true,
  },
  [sym_probability_property] = {
    .visible = true,
    .named = true,
  },
  [sym_unit_property] = {
    .visible = true,
    .named = true,
  },
  [sym_rationale_property] = {
    .visible = true,
    .named = true,
  },
  [sym_impact_multiplier_property] = {
    .visible = true,
    .named = true,
  },
  [sym_evidence_statement] = {
    .visible = true,
    .named = true,
  },
  [sym_evidence_block] = {
    .visible = true,
    .named = true,
  },
  [sym_evidence_property] = {
    .visible = true,
    .named = true,
  },
  [sym_agent_statement] = {
    .visible = true,
    .named = true,
  },
  [sym_agent_block] = {
    .visible = true,
    .named = true,
  },
  [sym_agent_property] = {
    .visible = true,
    .named = true,
  },
  [sym_model_statement] = {
    .visible = true,
    .named = true,
  },
  [sym_simulate_statement] = {
    .visible = true,
    .named = true,
  },
  [sym_distribution] = {
    .visible = true,
    .named = true,
  },
  [sym_triangular_distribution] = {
    .visible = true,
    .named = true,
  },
  [sym_normal_distribution] = {
    .visible = true,
    .named = true,
  },
  [sym_lognormal_distribution] = {
    .visible = true,
    .named = true,
  },
  [sym_uniform_distribution] = {
    .visible = true,
    .named = true,
  },
  [sym_beta_distribution] = {
    .visible = true,
    .named = true,
  },
  [sym_expression] = {
    .visible = true,
    .named = true,
  },
  [sym_binary_expression] = {
    .visible = true,
    .named = true,
  },
  [sym_unary_expression] = {
    .visible = true,
    .named = true,
  },
  [sym_conditional_expression] = {
    .visible = true,
    .named = true,
  },
  [sym_function_call] = {
    .visible = true,
    .named = true,
  },
  [sym_parenthesized_expression] = {
    .visible = true,
    .named = true,
  },
  [sym_probability] = {
    .visible = true,
    .named = true,
  },
  [sym_string] = {
    .visible = true,
    .named = true,
  },
  [aux_sym_source_file_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_driver_block_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_evidence_block_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_agent_block_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_function_call_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_string_repeat1] = {
    .visible = false,
    .named = false,
  },
};

enum ts_field_identifiers {
  field_alpha = 1,
  field_argument = 2,
  field_beta_param = 3,
  field_body = 4,
  field_condition = 5,
  field_distribution = 6,
  field_else_expr = 7,
  field_expression = 8,
  field_function = 9,
  field_high = 10,
  field_interval = 11,
  field_iterations = 12,
  field_left = 13,
  field_low = 14,
  field_max = 15,
  field_mean = 16,
  field_median = 17,
  field_min = 18,
  field_name = 19,
  field_operand = 20,
  field_p5 = 21,
  field_p50 = 22,
  field_p95 = 23,
  field_right = 24,
  field_sigma = 25,
  field_stddev = 26,
  field_text = 27,
  field_then_expr = 28,
  field_type = 29,
  field_unit = 30,
  field_value = 31,
};

static const char * const ts_field_names[] = {
  [0] = NULL,
  [field_alpha] = "alpha",
  [field_argument] = "argument",
  [field_beta_param] = "beta_param",
  [field_body] = "body",
  [field_condition] = "condition",
  [field_distribution] = "distribution",
  [field_else_expr] = "else_expr",
  [field_expression] = "expression",
  [field_function] = "function",
  [field_high] = "high",
  [field_interval] = "interval",
  [field_iterations] = "iterations",
  [field_left] = "left",
  [field_low] = "low",
  [field_max] = "max",
  [field_mean] = "mean",
  [field_median] = "median",
  [field_min] = "min",
  [field_name] = "name",
  [field_operand] = "operand",
  [field_p5] = "p5",
  [field_p50] = "p50",
  [field_p95] = "p95",
  [field_right] = "right",
  [field_sigma] = "sigma",
  [field_stddev] = "stddev",
  [field_text] = "text",
  [field_then_expr] = "then_expr",
  [field_type] = "type",
  [field_unit] = "unit",
  [field_value] = "value",
};

static const TSFieldMapSlice ts_field_map_slices[PRODUCTION_ID_COUNT] = {
  [1] = {.index = 0, .length = 1},
  [2] = {.index = 1, .length = 2},
  [3] = {.index = 3, .length = 1},
  [4] = {.index = 4, .length = 1},
  [5] = {.index = 5, .length = 3},
  [6] = {.index = 8, .length = 1},
  [7] = {.index = 9, .length = 1},
  [8] = {.index = 10, .length = 2},
  [9] = {.index = 12, .length = 1},
  [10] = {.index = 13, .length = 2},
  [11] = {.index = 15, .length = 1},
  [12] = {.index = 16, .length = 1},
  [13] = {.index = 17, .length = 3},
  [14] = {.index = 20, .length = 2},
  [15] = {.index = 22, .length = 2},
  [16] = {.index = 24, .length = 3},
  [17] = {.index = 27, .length = 2},
  [18] = {.index = 29, .length = 2},
  [19] = {.index = 31, .length = 2},
  [20] = {.index = 33, .length = 2},
  [21] = {.index = 35, .length = 3},
  [22] = {.index = 38, .length = 4},
};

static const TSFieldMapEntry ts_field_map_entries[] = {
  [0] =
    {field_text, 1},
  [1] =
    {field_body, 2},
    {field_name, 1},
  [3] =
    {field_expression, 2},
  [4] =
    {field_iterations, 1},
  [5] =
    {field_body, 3},
    {field_name, 1},
    {field_type, 2},
  [8] =
    {field_operand, 1},
  [9] =
    {field_function, 0},
  [10] =
    {field_left, 0},
    {field_right, 2},
  [12] =
    {field_value, 2},
  [13] =
    {field_argument, 2},
    {field_function, 0},
  [15] =
    {field_distribution, 2},
  [16] =
    {field_argument, 1},
  [17] =
    {field_argument, 2},
    {field_argument, 3, .inherited = true},
    {field_function, 0},
  [20] =
    {field_argument, 0, .inherited = true},
    {field_argument, 1, .inherited = true},
  [22] =
    {field_interval, 3},
    {field_unit, 4},
  [24] =
    {field_condition, 1},
    {field_else_expr, 5},
    {field_then_expr, 3},
  [27] =
    {field_mean, 2},
    {field_stddev, 4},
  [29] =
    {field_median, 2},
    {field_sigma, 4},
  [31] =
    {field_high, 4},
    {field_low, 2},
  [33] =
    {field_alpha, 2},
    {field_beta_param, 4},
  [35] =
    {field_p5, 2},
    {field_p50, 4},
    {field_p95, 6},
  [38] =
    {field_alpha, 2},
    {field_beta_param, 4},
    {field_max, 8},
    {field_min, 6},
};

static const TSSymbol ts_alias_sequences[PRODUCTION_ID_COUNT][MAX_ALIAS_SEQUENCE_LENGTH] = {
  [0] = {0},
};

static const uint16_t ts_non_terminal_alias_map[] = {
  0,
};

static const TSStateId ts_primary_state_ids[STATE_COUNT] = {
  [0] = 0,
  [1] = 1,
  [2] = 2,
  [3] = 3,
  [4] = 4,
  [5] = 5,
  [6] = 6,
  [7] = 7,
  [8] = 8,
  [9] = 9,
  [10] = 10,
  [11] = 11,
  [12] = 12,
  [13] = 13,
  [14] = 14,
  [15] = 15,
  [16] = 16,
  [17] = 17,
  [18] = 18,
  [19] = 19,
  [20] = 20,
  [21] = 21,
  [22] = 22,
  [23] = 23,
  [24] = 24,
  [25] = 25,
  [26] = 26,
  [27] = 27,
  [28] = 28,
  [29] = 29,
  [30] = 30,
  [31] = 31,
  [32] = 32,
  [33] = 33,
  [34] = 34,
  [35] = 35,
  [36] = 36,
  [37] = 37,
  [38] = 38,
  [39] = 39,
  [40] = 40,
  [41] = 41,
  [42] = 42,
  [43] = 43,
  [44] = 44,
  [45] = 45,
  [46] = 46,
  [47] = 47,
  [48] = 48,
  [49] = 49,
  [50] = 50,
  [51] = 51,
  [52] = 52,
  [53] = 53,
  [54] = 54,
  [55] = 55,
  [56] = 56,
  [57] = 57,
  [58] = 58,
  [59] = 59,
  [60] = 60,
  [61] = 61,
  [62] = 62,
  [63] = 63,
  [64] = 64,
  [65] = 65,
  [66] = 66,
  [67] = 67,
  [68] = 68,
  [69] = 69,
  [70] = 70,
  [71] = 71,
  [72] = 72,
  [73] = 73,
  [74] = 74,
  [75] = 75,
  [76] = 76,
  [77] = 77,
  [78] = 78,
  [79] = 79,
  [80] = 80,
  [81] = 81,
  [82] = 82,
  [83] = 83,
  [84] = 84,
  [85] = 85,
  [86] = 86,
  [87] = 87,
  [88] = 88,
  [89] = 89,
  [90] = 90,
  [91] = 91,
  [92] = 92,
  [93] = 93,
  [94] = 94,
  [95] = 95,
  [96] = 96,
  [97] = 97,
  [98] = 98,
  [99] = 99,
  [100] = 100,
  [101] = 101,
  [102] = 102,
  [103] = 103,
  [104] = 104,
  [105] = 105,
  [106] = 106,
  [107] = 107,
  [108] = 108,
  [109] = 109,
  [110] = 110,
  [111] = 111,
  [112] = 112,
  [113] = 113,
  [114] = 114,
  [115] = 115,
  [116] = 116,
  [117] = 117,
  [118] = 118,
  [119] = 119,
  [120] = 120,
  [121] = 121,
  [122] = 122,
  [123] = 123,
  [124] = 124,
  [125] = 125,
  [126] = 126,
  [127] = 127,
  [128] = 128,
  [129] = 129,
  [130] = 130,
  [131] = 131,
  [132] = 132,
  [133] = 133,
  [134] = 134,
  [135] = 135,
  [136] = 136,
  [137] = 137,
  [138] = 138,
  [139] = 139,
  [140] = 140,
  [141] = 141,
  [142] = 142,
};

static bool ts_lex(TSLexer *lexer, TSStateId state) {
  START_LEXER();
  eof = lexer->eof(lexer);
  switch (state) {
    case 0:
      if (eof) ADVANCE(188);
      ADVANCE_MAP(
        '!', 231,
        '"', 247,
        '#', 258,
        '(', 219,
        ')', 221,
        '*', 226,
        '+', 228,
        ',', 220,
        '-', 229,
        '/', 227,
        ':', 197,
        '\\', 252,
        '^', 230,
        'a', 65,
        'b', 39,
        'c', 121,
        'd', 12,
        'e', 87,
        'i', 63,
        'l', 120,
        'm', 117,
        'n', 118,
        'p', 140,
        'q', 165,
        'r', 15,
        's', 30,
        't', 69,
        'u', 109,
        'w', 57,
        '{', 194,
        '}', 195,
      );
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(0);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(239);
      END_STATE();
    case 1:
      if (lookahead == '\n') SKIP(1);
      if (lookahead == '#') ADVANCE(256);
      if (lookahead == '/') ADVANCE(255);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') ADVANCE(254);
      if (lookahead != 0) ADVANCE(253);
      END_STATE();
    case 2:
      ADVANCE_MAP(
        '!', 231,
        '#', 258,
        '(', 219,
        ')', 221,
        '-', 229,
        '/', 6,
        'i', 236,
        'p', 237,
      );
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(2);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(239);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(238);
      END_STATE();
    case 3:
      if (lookahead == '"') ADVANCE(247);
      if (lookahead == '#') ADVANCE(251);
      if (lookahead == '/') ADVANCE(250);
      if (lookahead == '\\') ADVANCE(252);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') ADVANCE(249);
      if (lookahead != 0) ADVANCE(248);
      END_STATE();
    case 4:
      if (lookahead == '#') ADVANCE(258);
      if (lookahead == '/') ADVANCE(6);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(4);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(185);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(238);
      END_STATE();
    case 5:
      if (lookahead == '#') ADVANCE(258);
      if (lookahead == '/') ADVANCE(6);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(5);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(240);
      END_STATE();
    case 6:
      if (lookahead == '*') ADVANCE(8);
      if (lookahead == '/') ADVANCE(258);
      END_STATE();
    case 7:
      if (lookahead == '*') ADVANCE(7);
      if (lookahead == '/') ADVANCE(257);
      if (lookahead != 0) ADVANCE(8);
      END_STATE();
    case 8:
      if (lookahead == '*') ADVANCE(7);
      if (lookahead != 0) ADVANCE(8);
      END_STATE();
    case 9:
      if (lookahead == '-') ADVANCE(184);
      END_STATE();
    case 10:
      if (lookahead == '-') ADVANCE(187);
      END_STATE();
    case 11:
      if (lookahead == '_') ADVANCE(102);
      END_STATE();
    case 12:
      if (lookahead == 'a') ADVANCE(156);
      if (lookahead == 'i') ADVANCE(146);
      if (lookahead == 'r') ADVANCE(70);
      END_STATE();
    case 13:
      if (lookahead == 'a') ADVANCE(225);
      END_STATE();
    case 14:
      if (lookahead == 'a') ADVANCE(31);
      END_STATE();
    case 15:
      if (lookahead == 'a') ADVANCE(153);
      if (lookahead == 'e') ADVANCE(90);
      END_STATE();
    case 16:
      if (lookahead == 'a') ADVANCE(28);
      END_STATE();
    case 17:
      if (lookahead == 'a') ADVANCE(85);
      END_STATE();
    case 18:
      if (lookahead == 'a') ADVANCE(138);
      END_STATE();
    case 19:
      if (lookahead == 'a') ADVANCE(107);
      END_STATE();
    case 20:
      if (lookahead == 'a') ADVANCE(86);
      END_STATE();
    case 21:
      if (lookahead == 'a') ADVANCE(139);
      END_STATE();
    case 22:
      if (lookahead == 'a') ADVANCE(133);
      END_STATE();
    case 23:
      if (lookahead == 'a') ADVANCE(159);
      END_STATE();
    case 24:
      if (lookahead == 'a') ADVANCE(93);
      END_STATE();
    case 25:
      if (lookahead == 'a') ADVANCE(115);
      END_STATE();
    case 26:
      if (lookahead == 'a') ADVANCE(161);
      END_STATE();
    case 27:
      if (lookahead == 'b') ADVANCE(171);
      END_STATE();
    case 28:
      if (lookahead == 'b') ADVANCE(75);
      END_STATE();
    case 29:
      if (lookahead == 'b') ADVANCE(16);
      END_STATE();
    case 30:
      if (lookahead == 'c') ADVANCE(68);
      if (lookahead == 'i') ADVANCE(97);
      if (lookahead == 'o') ADVANCE(166);
      if (lookahead == 'u') ADVANCE(98);
      END_STATE();
    case 31:
      if (lookahead == 'c') ADVANCE(152);
      END_STATE();
    case 32:
      if (lookahead == 'c') ADVANCE(144);
      if (lookahead == 't') ADVANCE(143);
      END_STATE();
    case 33:
      if (lookahead == 'c') ADVANCE(44);
      END_STATE();
    case 34:
      if (lookahead == 'c') ADVANCE(46);
      END_STATE();
    case 35:
      if (lookahead == 'c') ADVANCE(50);
      END_STATE();
    case 36:
      if (lookahead == 'd') ADVANCE(170);
      END_STATE();
    case 37:
      if (lookahead == 'd') ADVANCE(53);
      END_STATE();
    case 38:
      if (lookahead == 'd') ADVANCE(61);
      END_STATE();
    case 39:
      if (lookahead == 'e') ADVANCE(150);
      if (lookahead == 'i') ADVANCE(106);
      END_STATE();
    case 40:
      if (lookahead == 'e') ADVANCE(135);
      END_STATE();
    case 41:
      if (lookahead == 'e') ADVANCE(83);
      END_STATE();
    case 42:
      if (lookahead == 'e') ADVANCE(206);
      END_STATE();
    case 43:
      if (lookahead == 'e') ADVANCE(235);
      END_STATE();
    case 44:
      if (lookahead == 'e') ADVANCE(203);
      END_STATE();
    case 45:
      if (lookahead == 'e') ADVANCE(193);
      END_STATE();
    case 46:
      if (lookahead == 'e') ADVANCE(202);
      END_STATE();
    case 47:
      if (lookahead == 'e') ADVANCE(209);
      END_STATE();
    case 48:
      if (lookahead == 'e') ADVANCE(216);
      END_STATE();
    case 49:
      if (lookahead == 'e') ADVANCE(200);
      END_STATE();
    case 50:
      if (lookahead == 'e') ADVANCE(205);
      END_STATE();
    case 51:
      if (lookahead == 'e') ADVANCE(36);
      END_STATE();
    case 52:
      if (lookahead == 'e') ADVANCE(131);
      if (lookahead == 'i') ADVANCE(38);
      END_STATE();
    case 53:
      if (lookahead == 'e') ADVANCE(84);
      END_STATE();
    case 54:
      if (lookahead == 'e') ADVANCE(173);
      END_STATE();
    case 55:
      if (lookahead == 'e') ADVANCE(142);
      END_STATE();
    case 56:
      if (lookahead == 'e') ADVANCE(108);
      END_STATE();
    case 57:
      if (lookahead == 'e') ADVANCE(41);
      END_STATE();
    case 58:
      if (lookahead == 'e') ADVANCE(158);
      END_STATE();
    case 59:
      if (lookahead == 'e') ADVANCE(103);
      END_STATE();
    case 60:
      if (lookahead == 'e') ADVANCE(132);
      END_STATE();
    case 61:
      if (lookahead == 'e') ADVANCE(114);
      END_STATE();
    case 62:
      if (lookahead == 'e') ADVANCE(134);
      END_STATE();
    case 63:
      if (lookahead == 'f') ADVANCE(232);
      if (lookahead == 'm') ADVANCE(129);
      if (lookahead == 't') ADVANCE(55);
      END_STATE();
    case 64:
      if (lookahead == 'f') ADVANCE(126);
      if (lookahead == 't') ADVANCE(199);
      END_STATE();
    case 65:
      if (lookahead == 'g') ADVANCE(56);
      END_STATE();
    case 66:
      if (lookahead == 'g') ADVANCE(116);
      END_STATE();
    case 67:
      if (lookahead == 'g') ADVANCE(168);
      END_STATE();
    case 68:
      if (lookahead == 'h') ADVANCE(51);
      END_STATE();
    case 69:
      if (lookahead == 'h') ADVANCE(59);
      if (lookahead == 'r') ADVANCE(74);
      END_STATE();
    case 70:
      if (lookahead == 'i') ADVANCE(172);
      END_STATE();
    case 71:
      if (lookahead == 'i') ADVANCE(64);
      END_STATE();
    case 72:
      if (lookahead == 'i') ADVANCE(130);
      END_STATE();
    case 73:
      if (lookahead == 'i') ADVANCE(27);
      END_STATE();
    case 74:
      if (lookahead == 'i') ADVANCE(19);
      END_STATE();
    case 75:
      if (lookahead == 'i') ADVANCE(88);
      END_STATE();
    case 76:
      if (lookahead == 'i') ADVANCE(155);
      END_STATE();
    case 77:
      if (lookahead == 'i') ADVANCE(111);
      END_STATE();
    case 78:
      if (lookahead == 'i') ADVANCE(127);
      END_STATE();
    case 79:
      if (lookahead == 'i') ADVANCE(123);
      END_STATE();
    case 80:
      if (lookahead == 'i') ADVANCE(62);
      END_STATE();
    case 81:
      if (lookahead == 'i') ADVANCE(124);
      END_STATE();
    case 82:
      if (lookahead == 'i') ADVANCE(125);
      END_STATE();
    case 83:
      if (lookahead == 'k') ADVANCE(213);
      END_STATE();
    case 84:
      if (lookahead == 'l') ADVANCE(215);
      END_STATE();
    case 85:
      if (lookahead == 'l') ADVANCE(222);
      END_STATE();
    case 86:
      if (lookahead == 'l') ADVANCE(223);
      END_STATE();
    case 87:
      if (lookahead == 'l') ADVANCE(149);
      if (lookahead == 'v') ADVANCE(52);
      END_STATE();
    case 88:
      if (lookahead == 'l') ADVANCE(76);
      END_STATE();
    case 89:
      if (lookahead == 'l') ADVANCE(23);
      END_STATE();
    case 90:
      if (lookahead == 'l') ADVANCE(54);
      END_STATE();
    case 91:
      if (lookahead == 'l') ADVANCE(80);
      END_STATE();
    case 92:
      if (lookahead == 'l') ADVANCE(47);
      END_STATE();
    case 93:
      if (lookahead == 'l') ADVANCE(49);
      END_STATE();
    case 94:
      if (lookahead == 'l') ADVANCE(157);
      END_STATE();
    case 95:
      if (lookahead == 'l') ADVANCE(22);
      END_STATE();
    case 96:
      if (lookahead == 'm') ADVANCE(224);
      END_STATE();
    case 97:
      if (lookahead == 'm') ADVANCE(164);
      END_STATE();
    case 98:
      if (lookahead == 'm') ADVANCE(101);
      END_STATE();
    case 99:
      if (lookahead == 'm') ADVANCE(17);
      END_STATE();
    case 100:
      if (lookahead == 'm') ADVANCE(20);
      END_STATE();
    case 101:
      if (lookahead == 'm') ADVANCE(21);
      END_STATE();
    case 102:
      if (lookahead == 'm') ADVANCE(169);
      END_STATE();
    case 103:
      if (lookahead == 'n') ADVANCE(234);
      END_STATE();
    case 104:
      if (lookahead == 'n') ADVANCE(189);
      END_STATE();
    case 105:
      if (lookahead == 'n') ADVANCE(196);
      END_STATE();
    case 106:
      if (lookahead == 'n') ADVANCE(18);
      END_STATE();
    case 107:
      if (lookahead == 'n') ADVANCE(67);
      END_STATE();
    case 108:
      if (lookahead == 'n') ADVANCE(151);
      END_STATE();
    case 109:
      if (lookahead == 'n') ADVANCE(71);
      END_STATE();
    case 110:
      if (lookahead == 'n') ADVANCE(148);
      END_STATE();
    case 111:
      if (lookahead == 'n') ADVANCE(167);
      END_STATE();
    case 112:
      if (lookahead == 'n') ADVANCE(24);
      END_STATE();
    case 113:
      if (lookahead == 'n') ADVANCE(154);
      END_STATE();
    case 114:
      if (lookahead == 'n') ADVANCE(34);
      END_STATE();
    case 115:
      if (lookahead == 'n') ADVANCE(35);
      END_STATE();
    case 116:
      if (lookahead == 'n') ADVANCE(128);
      END_STATE();
    case 117:
      if (lookahead == 'o') ADVANCE(37);
      END_STATE();
    case 118:
      if (lookahead == 'o') ADVANCE(137);
      END_STATE();
    case 119:
      if (lookahead == 'o') ADVANCE(29);
      END_STATE();
    case 120:
      if (lookahead == 'o') ADVANCE(66);
      END_STATE();
    case 121:
      if (lookahead == 'o') ADVANCE(113);
      END_STATE();
    case 122:
      if (lookahead == 'o') ADVANCE(163);
      END_STATE();
    case 123:
      if (lookahead == 'o') ADVANCE(104);
      END_STATE();
    case 124:
      if (lookahead == 'o') ADVANCE(110);
      END_STATE();
    case 125:
      if (lookahead == 'o') ADVANCE(105);
      END_STATE();
    case 126:
      if (lookahead == 'o') ADVANCE(141);
      END_STATE();
    case 127:
      if (lookahead == 'o') ADVANCE(112);
      END_STATE();
    case 128:
      if (lookahead == 'o') ADVANCE(145);
      END_STATE();
    case 129:
      if (lookahead == 'p') ADVANCE(14);
      END_STATE();
    case 130:
      if (lookahead == 'p') ADVANCE(91);
      END_STATE();
    case 131:
      if (lookahead == 'r') ADVANCE(174);
      END_STATE();
    case 132:
      if (lookahead == 'r') ADVANCE(190);
      END_STATE();
    case 133:
      if (lookahead == 'r') ADVANCE(218);
      END_STATE();
    case 134:
      if (lookahead == 'r') ADVANCE(201);
      END_STATE();
    case 135:
      if (lookahead == 'r') ADVANCE(175);
      if (lookahead == 's') ADVANCE(160);
      END_STATE();
    case 136:
      if (lookahead == 'r') ADVANCE(33);
      END_STATE();
    case 137:
      if (lookahead == 'r') ADVANCE(99);
      END_STATE();
    case 138:
      if (lookahead == 'r') ADVANCE(176);
      END_STATE();
    case 139:
      if (lookahead == 'r') ADVANCE(177);
      END_STATE();
    case 140:
      if (lookahead == 'r') ADVANCE(119);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(244);
      END_STATE();
    case 141:
      if (lookahead == 'r') ADVANCE(96);
      END_STATE();
    case 142:
      if (lookahead == 'r') ADVANCE(26);
      END_STATE();
    case 143:
      if (lookahead == 'r') ADVANCE(73);
      END_STATE();
    case 144:
      if (lookahead == 'r') ADVANCE(58);
      END_STATE();
    case 145:
      if (lookahead == 'r') ADVANCE(100);
      END_STATE();
    case 146:
      if (lookahead == 's') ADVANCE(32);
      END_STATE();
    case 147:
      if (lookahead == 's') ADVANCE(191);
      END_STATE();
    case 148:
      if (lookahead == 's') ADVANCE(217);
      END_STATE();
    case 149:
      if (lookahead == 's') ADVANCE(43);
      END_STATE();
    case 150:
      if (lookahead == 't') ADVANCE(13);
      END_STATE();
    case 151:
      if (lookahead == 't') ADVANCE(207);
      END_STATE();
    case 152:
      if (lookahead == 't') ADVANCE(11);
      END_STATE();
    case 153:
      if (lookahead == 't') ADVANCE(78);
      END_STATE();
    case 154:
      if (lookahead == 't') ADVANCE(77);
      END_STATE();
    case 155:
      if (lookahead == 't') ADVANCE(178);
      END_STATE();
    case 156:
      if (lookahead == 't') ADVANCE(42);
      if (lookahead == 'y') ADVANCE(211);
      END_STATE();
    case 157:
      if (lookahead == 't') ADVANCE(72);
      END_STATE();
    case 158:
      if (lookahead == 't') ADVANCE(45);
      END_STATE();
    case 159:
      if (lookahead == 't') ADVANCE(48);
      END_STATE();
    case 160:
      if (lookahead == 't') ADVANCE(79);
      END_STATE();
    case 161:
      if (lookahead == 't') ADVANCE(81);
      END_STATE();
    case 162:
      if (lookahead == 't') ADVANCE(82);
      END_STATE();
    case 163:
      if (lookahead == 'u') ADVANCE(147);
      END_STATE();
    case 164:
      if (lookahead == 'u') ADVANCE(89);
      END_STATE();
    case 165:
      if (lookahead == 'u') ADVANCE(40);
      END_STATE();
    case 166:
      if (lookahead == 'u') ADVANCE(136);
      END_STATE();
    case 167:
      if (lookahead == 'u') ADVANCE(122);
      END_STATE();
    case 168:
      if (lookahead == 'u') ADVANCE(95);
      END_STATE();
    case 169:
      if (lookahead == 'u') ADVANCE(94);
      END_STATE();
    case 170:
      if (lookahead == 'u') ADVANCE(92);
      END_STATE();
    case 171:
      if (lookahead == 'u') ADVANCE(162);
      END_STATE();
    case 172:
      if (lookahead == 'v') ADVANCE(60);
      END_STATE();
    case 173:
      if (lookahead == 'v') ADVANCE(25);
      END_STATE();
    case 174:
      if (lookahead == 'y') ADVANCE(210);
      END_STATE();
    case 175:
      if (lookahead == 'y') ADVANCE(208);
      END_STATE();
    case 176:
      if (lookahead == 'y') ADVANCE(192);
      END_STATE();
    case 177:
      if (lookahead == 'y') ADVANCE(204);
      END_STATE();
    case 178:
      if (lookahead == 'y') ADVANCE(198);
      END_STATE();
    case 179:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(241);
      END_STATE();
    case 180:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(10);
      END_STATE();
    case 181:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(246);
      END_STATE();
    case 182:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(242);
      END_STATE();
    case 183:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(180);
      END_STATE();
    case 184:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(181);
      END_STATE();
    case 185:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(183);
      END_STATE();
    case 186:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(9);
      END_STATE();
    case 187:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(186);
      END_STATE();
    case 188:
      ACCEPT_TOKEN(ts_builtin_sym_end);
      END_STATE();
    case 189:
      ACCEPT_TOKEN(anon_sym_question);
      END_STATE();
    case 190:
      ACCEPT_TOKEN(anon_sym_driver);
      END_STATE();
    case 191:
      ACCEPT_TOKEN(anon_sym_continuous);
      END_STATE();
    case 192:
      ACCEPT_TOKEN(anon_sym_binary);
      END_STATE();
    case 193:
      ACCEPT_TOKEN(anon_sym_discrete);
      END_STATE();
    case 194:
      ACCEPT_TOKEN(anon_sym_LBRACE);
      END_STATE();
    case 195:
      ACCEPT_TOKEN(anon_sym_RBRACE);
      END_STATE();
    case 196:
      ACCEPT_TOKEN(anon_sym_distribution);
      END_STATE();
    case 197:
      ACCEPT_TOKEN(anon_sym_COLON);
      END_STATE();
    case 198:
      ACCEPT_TOKEN(anon_sym_probability);
      END_STATE();
    case 199:
      ACCEPT_TOKEN(anon_sym_unit);
      END_STATE();
    case 200:
      ACCEPT_TOKEN(anon_sym_rationale);
      END_STATE();
    case 201:
      ACCEPT_TOKEN(anon_sym_impact_multiplier);
      END_STATE();
    case 202:
      ACCEPT_TOKEN(anon_sym_evidence);
      END_STATE();
    case 203:
      ACCEPT_TOKEN(anon_sym_source);
      END_STATE();
    case 204:
      ACCEPT_TOKEN(anon_sym_summary);
      END_STATE();
    case 205:
      ACCEPT_TOKEN(anon_sym_relevance);
      END_STATE();
    case 206:
      ACCEPT_TOKEN(anon_sym_date);
      END_STATE();
    case 207:
      ACCEPT_TOKEN(anon_sym_agent);
      END_STATE();
    case 208:
      ACCEPT_TOKEN(anon_sym_query);
      END_STATE();
    case 209:
      ACCEPT_TOKEN(anon_sym_schedule);
      END_STATE();
    case 210:
      ACCEPT_TOKEN(anon_sym_every);
      END_STATE();
    case 211:
      ACCEPT_TOKEN(anon_sym_day);
      if (lookahead == 's') ADVANCE(212);
      END_STATE();
    case 212:
      ACCEPT_TOKEN(anon_sym_days);
      END_STATE();
    case 213:
      ACCEPT_TOKEN(anon_sym_week);
      if (lookahead == 's') ADVANCE(214);
      END_STATE();
    case 214:
      ACCEPT_TOKEN(anon_sym_weeks);
      END_STATE();
    case 215:
      ACCEPT_TOKEN(anon_sym_model);
      END_STATE();
    case 216:
      ACCEPT_TOKEN(anon_sym_simulate);
      END_STATE();
    case 217:
      ACCEPT_TOKEN(anon_sym_iterations);
      END_STATE();
    case 218:
      ACCEPT_TOKEN(anon_sym_triangular);
      END_STATE();
    case 219:
      ACCEPT_TOKEN(anon_sym_LPAREN);
      END_STATE();
    case 220:
      ACCEPT_TOKEN(anon_sym_COMMA);
      END_STATE();
    case 221:
      ACCEPT_TOKEN(anon_sym_RPAREN);
      END_STATE();
    case 222:
      ACCEPT_TOKEN(anon_sym_normal);
      END_STATE();
    case 223:
      ACCEPT_TOKEN(anon_sym_lognormal);
      END_STATE();
    case 224:
      ACCEPT_TOKEN(anon_sym_uniform);
      END_STATE();
    case 225:
      ACCEPT_TOKEN(anon_sym_beta);
      END_STATE();
    case 226:
      ACCEPT_TOKEN(anon_sym_STAR);
      END_STATE();
    case 227:
      ACCEPT_TOKEN(anon_sym_SLASH);
      if (lookahead == '*') ADVANCE(8);
      if (lookahead == '/') ADVANCE(258);
      END_STATE();
    case 228:
      ACCEPT_TOKEN(anon_sym_PLUS);
      END_STATE();
    case 229:
      ACCEPT_TOKEN(anon_sym_DASH);
      END_STATE();
    case 230:
      ACCEPT_TOKEN(anon_sym_CARET);
      END_STATE();
    case 231:
      ACCEPT_TOKEN(anon_sym_BANG);
      END_STATE();
    case 232:
      ACCEPT_TOKEN(anon_sym_if);
      END_STATE();
    case 233:
      ACCEPT_TOKEN(anon_sym_if);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(238);
      END_STATE();
    case 234:
      ACCEPT_TOKEN(anon_sym_then);
      END_STATE();
    case 235:
      ACCEPT_TOKEN(anon_sym_else);
      END_STATE();
    case 236:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'f') ADVANCE(233);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(238);
      END_STATE();
    case 237:
      ACCEPT_TOKEN(sym_identifier);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(237);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(238);
      END_STATE();
    case 238:
      ACCEPT_TOKEN(sym_identifier);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(238);
      END_STATE();
    case 239:
      ACCEPT_TOKEN(sym_number);
      if (lookahead == '%') ADVANCE(245);
      if (lookahead == '.') ADVANCE(179);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(239);
      END_STATE();
    case 240:
      ACCEPT_TOKEN(sym_number);
      if (lookahead == '.') ADVANCE(182);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(240);
      END_STATE();
    case 241:
      ACCEPT_TOKEN(sym_number);
      if (lookahead == 'p') ADVANCE(243);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(241);
      END_STATE();
    case 242:
      ACCEPT_TOKEN(sym_number);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(242);
      END_STATE();
    case 243:
      ACCEPT_TOKEN(aux_sym_probability_token1);
      END_STATE();
    case 244:
      ACCEPT_TOKEN(aux_sym_probability_token2);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(244);
      END_STATE();
    case 245:
      ACCEPT_TOKEN(aux_sym_probability_token3);
      END_STATE();
    case 246:
      ACCEPT_TOKEN(sym_date);
      END_STATE();
    case 247:
      ACCEPT_TOKEN(anon_sym_DQUOTE);
      END_STATE();
    case 248:
      ACCEPT_TOKEN(aux_sym_string_token1);
      END_STATE();
    case 249:
      ACCEPT_TOKEN(aux_sym_string_token1);
      if (lookahead == '#') ADVANCE(251);
      if (lookahead == '/') ADVANCE(250);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') ADVANCE(249);
      if (lookahead != 0 &&
          lookahead != '"' &&
          lookahead != '#' &&
          lookahead != '\\') ADVANCE(248);
      END_STATE();
    case 250:
      ACCEPT_TOKEN(aux_sym_string_token1);
      if (lookahead == '*') ADVANCE(8);
      if (lookahead == '/') ADVANCE(258);
      END_STATE();
    case 251:
      ACCEPT_TOKEN(aux_sym_string_token1);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(258);
      END_STATE();
    case 252:
      ACCEPT_TOKEN(anon_sym_BSLASH);
      END_STATE();
    case 253:
      ACCEPT_TOKEN(aux_sym_string_token2);
      END_STATE();
    case 254:
      ACCEPT_TOKEN(aux_sym_string_token2);
      if (lookahead == '#') ADVANCE(256);
      if (lookahead == '/') ADVANCE(255);
      if (lookahead == '\t' ||
          (0x0b <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') ADVANCE(254);
      if (lookahead != 0 &&
          (lookahead < '\t' || '\r' < lookahead)) ADVANCE(253);
      END_STATE();
    case 255:
      ACCEPT_TOKEN(aux_sym_string_token2);
      if (lookahead == '*') ADVANCE(8);
      if (lookahead == '/') ADVANCE(258);
      END_STATE();
    case 256:
      ACCEPT_TOKEN(aux_sym_string_token2);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(258);
      END_STATE();
    case 257:
      ACCEPT_TOKEN(sym_comment);
      END_STATE();
    case 258:
      ACCEPT_TOKEN(sym_comment);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(258);
      END_STATE();
    default:
      return false;
  }
}

static const TSLexMode ts_lex_modes[STATE_COUNT] = {
  [0] = {.lex_state = 0},
  [1] = {.lex_state = 0},
  [2] = {.lex_state = 0},
  [3] = {.lex_state = 0},
  [4] = {.lex_state = 0},
  [5] = {.lex_state = 0},
  [6] = {.lex_state = 2},
  [7] = {.lex_state = 2},
  [8] = {.lex_state = 2},
  [9] = {.lex_state = 0},
  [10] = {.lex_state = 0},
  [11] = {.lex_state = 2},
  [12] = {.lex_state = 0},
  [13] = {.lex_state = 0},
  [14] = {.lex_state = 2},
  [15] = {.lex_state = 0},
  [16] = {.lex_state = 2},
  [17] = {.lex_state = 2},
  [18] = {.lex_state = 2},
  [19] = {.lex_state = 2},
  [20] = {.lex_state = 0},
  [21] = {.lex_state = 2},
  [22] = {.lex_state = 0},
  [23] = {.lex_state = 2},
  [24] = {.lex_state = 2},
  [25] = {.lex_state = 2},
  [26] = {.lex_state = 2},
  [27] = {.lex_state = 2},
  [28] = {.lex_state = 2},
  [29] = {.lex_state = 2},
  [30] = {.lex_state = 0},
  [31] = {.lex_state = 2},
  [32] = {.lex_state = 2},
  [33] = {.lex_state = 2},
  [34] = {.lex_state = 2},
  [35] = {.lex_state = 0},
  [36] = {.lex_state = 2},
  [37] = {.lex_state = 2},
  [38] = {.lex_state = 2},
  [39] = {.lex_state = 0},
  [40] = {.lex_state = 0},
  [41] = {.lex_state = 0},
  [42] = {.lex_state = 0},
  [43] = {.lex_state = 0},
  [44] = {.lex_state = 0},
  [45] = {.lex_state = 0},
  [46] = {.lex_state = 0},
  [47] = {.lex_state = 0},
  [48] = {.lex_state = 0},
  [49] = {.lex_state = 0},
  [50] = {.lex_state = 0},
  [51] = {.lex_state = 0},
  [52] = {.lex_state = 0},
  [53] = {.lex_state = 0},
  [54] = {.lex_state = 0},
  [55] = {.lex_state = 0},
  [56] = {.lex_state = 0},
  [57] = {.lex_state = 0},
  [58] = {.lex_state = 0},
  [59] = {.lex_state = 0},
  [60] = {.lex_state = 0},
  [61] = {.lex_state = 0},
  [62] = {.lex_state = 0},
  [63] = {.lex_state = 0},
  [64] = {.lex_state = 0},
  [65] = {.lex_state = 0},
  [66] = {.lex_state = 0},
  [67] = {.lex_state = 0},
  [68] = {.lex_state = 0},
  [69] = {.lex_state = 0},
  [70] = {.lex_state = 0},
  [71] = {.lex_state = 0},
  [72] = {.lex_state = 0},
  [73] = {.lex_state = 0},
  [74] = {.lex_state = 0},
  [75] = {.lex_state = 0},
  [76] = {.lex_state = 0},
  [77] = {.lex_state = 0},
  [78] = {.lex_state = 0},
  [79] = {.lex_state = 0},
  [80] = {.lex_state = 0},
  [81] = {.lex_state = 0},
  [82] = {.lex_state = 0},
  [83] = {.lex_state = 0},
  [84] = {.lex_state = 0},
  [85] = {.lex_state = 0},
  [86] = {.lex_state = 0},
  [87] = {.lex_state = 0},
  [88] = {.lex_state = 0},
  [89] = {.lex_state = 0},
  [90] = {.lex_state = 0},
  [91] = {.lex_state = 0},
  [92] = {.lex_state = 0},
  [93] = {.lex_state = 0},
  [94] = {.lex_state = 0},
  [95] = {.lex_state = 0},
  [96] = {.lex_state = 0},
  [97] = {.lex_state = 0},
  [98] = {.lex_state = 3},
  [99] = {.lex_state = 3},
  [100] = {.lex_state = 3},
  [101] = {.lex_state = 0},
  [102] = {.lex_state = 0},
  [103] = {.lex_state = 0},
  [104] = {.lex_state = 0},
  [105] = {.lex_state = 3},
  [106] = {.lex_state = 0},
  [107] = {.lex_state = 0},
  [108] = {.lex_state = 0},
  [109] = {.lex_state = 0},
  [110] = {.lex_state = 0},
  [111] = {.lex_state = 0},
  [112] = {.lex_state = 0},
  [113] = {.lex_state = 0},
  [114] = {.lex_state = 0},
  [115] = {.lex_state = 0},
  [116] = {.lex_state = 0},
  [117] = {.lex_state = 0},
  [118] = {.lex_state = 0},
  [119] = {.lex_state = 0},
  [120] = {.lex_state = 0},
  [121] = {.lex_state = 0},
  [122] = {.lex_state = 4},
  [123] = {.lex_state = 0},
  [124] = {.lex_state = 0},
  [125] = {.lex_state = 0},
  [126] = {.lex_state = 0},
  [127] = {.lex_state = 5},
  [128] = {.lex_state = 0},
  [129] = {.lex_state = 0},
  [130] = {.lex_state = 0},
  [131] = {.lex_state = 1},
  [132] = {.lex_state = 0},
  [133] = {.lex_state = 0},
  [134] = {.lex_state = 0},
  [135] = {.lex_state = 0},
  [136] = {.lex_state = 5},
  [137] = {.lex_state = 0},
  [138] = {.lex_state = 5},
  [139] = {.lex_state = 0},
  [140] = {.lex_state = 4},
  [141] = {.lex_state = 4},
  [142] = {.lex_state = 4},
};

static const uint16_t ts_parse_table[LARGE_STATE_COUNT][SYMBOL_COUNT] = {
  [0] = {
    [ts_builtin_sym_end] = ACTIONS(1),
    [anon_sym_question] = ACTIONS(1),
    [anon_sym_driver] = ACTIONS(1),
    [anon_sym_continuous] = ACTIONS(1),
    [anon_sym_binary] = ACTIONS(1),
    [anon_sym_discrete] = ACTIONS(1),
    [anon_sym_LBRACE] = ACTIONS(1),
    [anon_sym_RBRACE] = ACTIONS(1),
    [anon_sym_distribution] = ACTIONS(1),
    [anon_sym_COLON] = ACTIONS(1),
    [anon_sym_probability] = ACTIONS(1),
    [anon_sym_unit] = ACTIONS(1),
    [anon_sym_rationale] = ACTIONS(1),
    [anon_sym_impact_multiplier] = ACTIONS(1),
    [anon_sym_evidence] = ACTIONS(1),
    [anon_sym_source] = ACTIONS(1),
    [anon_sym_summary] = ACTIONS(1),
    [anon_sym_relevance] = ACTIONS(1),
    [anon_sym_date] = ACTIONS(1),
    [anon_sym_agent] = ACTIONS(1),
    [anon_sym_query] = ACTIONS(1),
    [anon_sym_schedule] = ACTIONS(1),
    [anon_sym_every] = ACTIONS(1),
    [anon_sym_day] = ACTIONS(1),
    [anon_sym_days] = ACTIONS(1),
    [anon_sym_week] = ACTIONS(1),
    [anon_sym_weeks] = ACTIONS(1),
    [anon_sym_model] = ACTIONS(1),
    [anon_sym_simulate] = ACTIONS(1),
    [anon_sym_iterations] = ACTIONS(1),
    [anon_sym_triangular] = ACTIONS(1),
    [anon_sym_LPAREN] = ACTIONS(1),
    [anon_sym_COMMA] = ACTIONS(1),
    [anon_sym_RPAREN] = ACTIONS(1),
    [anon_sym_normal] = ACTIONS(1),
    [anon_sym_lognormal] = ACTIONS(1),
    [anon_sym_uniform] = ACTIONS(1),
    [anon_sym_beta] = ACTIONS(1),
    [anon_sym_STAR] = ACTIONS(1),
    [anon_sym_SLASH] = ACTIONS(1),
    [anon_sym_PLUS] = ACTIONS(1),
    [anon_sym_DASH] = ACTIONS(1),
    [anon_sym_CARET] = ACTIONS(1),
    [anon_sym_BANG] = ACTIONS(1),
    [anon_sym_if] = ACTIONS(1),
    [anon_sym_then] = ACTIONS(1),
    [anon_sym_else] = ACTIONS(1),
    [sym_number] = ACTIONS(1),
    [aux_sym_probability_token1] = ACTIONS(1),
    [aux_sym_probability_token2] = ACTIONS(1),
    [aux_sym_probability_token3] = ACTIONS(1),
    [anon_sym_DQUOTE] = ACTIONS(1),
    [anon_sym_BSLASH] = ACTIONS(1),
    [sym_comment] = ACTIONS(3),
  },
  [1] = {
    [sym_source_file] = STATE(137),
    [sym__statement] = STATE(41),
    [sym_question_statement] = STATE(41),
    [sym_driver_statement] = STATE(41),
    [sym_evidence_statement] = STATE(41),
    [sym_agent_statement] = STATE(41),
    [sym_model_statement] = STATE(41),
    [sym_simulate_statement] = STATE(41),
    [aux_sym_source_file_repeat1] = STATE(41),
    [ts_builtin_sym_end] = ACTIONS(5),
    [anon_sym_question] = ACTIONS(7),
    [anon_sym_driver] = ACTIONS(9),
    [anon_sym_evidence] = ACTIONS(11),
    [anon_sym_agent] = ACTIONS(13),
    [anon_sym_model] = ACTIONS(15),
    [anon_sym_simulate] = ACTIONS(17),
    [sym_comment] = ACTIONS(3),
  },
};

static const uint16_t ts_small_parse_table[] = {
  [0] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(21), 1,
      anon_sym_SLASH,
    ACTIONS(19), 25,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_RBRACE,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
      anon_sym_evidence,
      anon_sym_source,
      anon_sym_summary,
      anon_sym_relevance,
      anon_sym_date,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
      anon_sym_COMMA,
      anon_sym_RPAREN,
      anon_sym_STAR,
      anon_sym_PLUS,
      anon_sym_DASH,
      anon_sym_CARET,
      anon_sym_then,
      anon_sym_else,
  [34] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(23), 19,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_RBRACE,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
      anon_sym_evidence,
      anon_sym_source,
      anon_sym_summary,
      anon_sym_relevance,
      anon_sym_date,
      anon_sym_agent,
      anon_sym_query,
      anon_sym_schedule,
      anon_sym_model,
      anon_sym_simulate,
  [59] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(25), 19,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_RBRACE,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
      anon_sym_evidence,
      anon_sym_source,
      anon_sym_summary,
      anon_sym_relevance,
      anon_sym_date,
      anon_sym_agent,
      anon_sym_query,
      anon_sym_schedule,
      anon_sym_model,
      anon_sym_simulate,
  [84] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(29), 1,
      anon_sym_LPAREN,
    ACTIONS(31), 1,
      anon_sym_SLASH,
    ACTIONS(27), 15,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
      anon_sym_COMMA,
      anon_sym_RPAREN,
      anon_sym_STAR,
      anon_sym_PLUS,
      anon_sym_DASH,
      anon_sym_CARET,
      anon_sym_then,
      anon_sym_else,
  [111] = 11,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(33), 1,
      anon_sym_LPAREN,
    ACTIONS(35), 1,
      anon_sym_RPAREN,
    ACTIONS(39), 1,
      anon_sym_if,
    ACTIONS(41), 1,
      sym_identifier,
    ACTIONS(43), 1,
      sym_number,
    ACTIONS(47), 1,
      aux_sym_probability_token2,
    STATE(47), 1,
      sym_expression,
    ACTIONS(37), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(45), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(30), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [152] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(33), 1,
      anon_sym_LPAREN,
    ACTIONS(39), 1,
      anon_sym_if,
    ACTIONS(41), 1,
      sym_identifier,
    ACTIONS(43), 1,
      sym_number,
    ACTIONS(47), 1,
      aux_sym_probability_token2,
    STATE(67), 1,
      sym_expression,
    ACTIONS(37), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(45), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(30), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [190] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(33), 1,
      anon_sym_LPAREN,
    ACTIONS(39), 1,
      anon_sym_if,
    ACTIONS(41), 1,
      sym_identifier,
    ACTIONS(43), 1,
      sym_number,
    ACTIONS(47), 1,
      aux_sym_probability_token2,
    STATE(71), 1,
      sym_expression,
    ACTIONS(37), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(45), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(30), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [228] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(51), 1,
      anon_sym_STAR,
    ACTIONS(53), 1,
      anon_sym_SLASH,
    ACTIONS(55), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
    ACTIONS(49), 12,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
      anon_sym_COMMA,
      anon_sym_RPAREN,
      anon_sym_CARET,
      anon_sym_then,
      anon_sym_else,
  [256] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(59), 1,
      anon_sym_SLASH,
    ACTIONS(57), 15,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
      anon_sym_COMMA,
      anon_sym_RPAREN,
      anon_sym_STAR,
      anon_sym_PLUS,
      anon_sym_DASH,
      anon_sym_CARET,
      anon_sym_then,
      anon_sym_else,
  [280] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(33), 1,
      anon_sym_LPAREN,
    ACTIONS(39), 1,
      anon_sym_if,
    ACTIONS(41), 1,
      sym_identifier,
    ACTIONS(43), 1,
      sym_number,
    ACTIONS(47), 1,
      aux_sym_probability_token2,
    STATE(83), 1,
      sym_expression,
    ACTIONS(37), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(45), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(30), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [318] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(51), 1,
      anon_sym_STAR,
    ACTIONS(53), 1,
      anon_sym_SLASH,
    ACTIONS(49), 14,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
      anon_sym_COMMA,
      anon_sym_RPAREN,
      anon_sym_PLUS,
      anon_sym_DASH,
      anon_sym_CARET,
      anon_sym_then,
      anon_sym_else,
  [344] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(61), 1,
      anon_sym_SLASH,
    ACTIONS(49), 15,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
      anon_sym_COMMA,
      anon_sym_RPAREN,
      anon_sym_STAR,
      anon_sym_PLUS,
      anon_sym_DASH,
      anon_sym_CARET,
      anon_sym_then,
      anon_sym_else,
  [368] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(33), 1,
      anon_sym_LPAREN,
    ACTIONS(39), 1,
      anon_sym_if,
    ACTIONS(41), 1,
      sym_identifier,
    ACTIONS(43), 1,
      sym_number,
    ACTIONS(47), 1,
      aux_sym_probability_token2,
    STATE(13), 1,
      sym_expression,
    ACTIONS(37), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(45), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(30), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [406] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(65), 1,
      anon_sym_SLASH,
    ACTIONS(63), 15,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
      anon_sym_COMMA,
      anon_sym_RPAREN,
      anon_sym_STAR,
      anon_sym_PLUS,
      anon_sym_DASH,
      anon_sym_CARET,
      anon_sym_then,
      anon_sym_else,
  [430] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(33), 1,
      anon_sym_LPAREN,
    ACTIONS(39), 1,
      anon_sym_if,
    ACTIONS(41), 1,
      sym_identifier,
    ACTIONS(43), 1,
      sym_number,
    ACTIONS(47), 1,
      aux_sym_probability_token2,
    STATE(45), 1,
      sym_expression,
    ACTIONS(37), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(45), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(30), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [468] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(33), 1,
      anon_sym_LPAREN,
    ACTIONS(39), 1,
      anon_sym_if,
    ACTIONS(41), 1,
      sym_identifier,
    ACTIONS(43), 1,
      sym_number,
    ACTIONS(47), 1,
      aux_sym_probability_token2,
    STATE(81), 1,
      sym_expression,
    ACTIONS(37), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(45), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(30), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [506] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(33), 1,
      anon_sym_LPAREN,
    ACTIONS(39), 1,
      anon_sym_if,
    ACTIONS(41), 1,
      sym_identifier,
    ACTIONS(43), 1,
      sym_number,
    ACTIONS(47), 1,
      aux_sym_probability_token2,
    STATE(64), 1,
      sym_expression,
    ACTIONS(37), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(45), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(30), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [544] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(33), 1,
      anon_sym_LPAREN,
    ACTIONS(39), 1,
      anon_sym_if,
    ACTIONS(41), 1,
      sym_identifier,
    ACTIONS(43), 1,
      sym_number,
    ACTIONS(47), 1,
      aux_sym_probability_token2,
    STATE(82), 1,
      sym_expression,
    ACTIONS(37), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(45), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(30), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [582] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(69), 1,
      anon_sym_SLASH,
    ACTIONS(67), 15,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
      anon_sym_COMMA,
      anon_sym_RPAREN,
      anon_sym_STAR,
      anon_sym_PLUS,
      anon_sym_DASH,
      anon_sym_CARET,
      anon_sym_then,
      anon_sym_else,
  [606] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(33), 1,
      anon_sym_LPAREN,
    ACTIONS(39), 1,
      anon_sym_if,
    ACTIONS(41), 1,
      sym_identifier,
    ACTIONS(43), 1,
      sym_number,
    ACTIONS(47), 1,
      aux_sym_probability_token2,
    STATE(39), 1,
      sym_expression,
    ACTIONS(37), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(45), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(30), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [644] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(73), 1,
      anon_sym_SLASH,
    ACTIONS(71), 15,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
      anon_sym_COMMA,
      anon_sym_RPAREN,
      anon_sym_STAR,
      anon_sym_PLUS,
      anon_sym_DASH,
      anon_sym_CARET,
      anon_sym_then,
      anon_sym_else,
  [668] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(33), 1,
      anon_sym_LPAREN,
    ACTIONS(39), 1,
      anon_sym_if,
    ACTIONS(41), 1,
      sym_identifier,
    ACTIONS(43), 1,
      sym_number,
    ACTIONS(47), 1,
      aux_sym_probability_token2,
    STATE(66), 1,
      sym_expression,
    ACTIONS(37), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(45), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(30), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [706] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(33), 1,
      anon_sym_LPAREN,
    ACTIONS(39), 1,
      anon_sym_if,
    ACTIONS(41), 1,
      sym_identifier,
    ACTIONS(43), 1,
      sym_number,
    ACTIONS(47), 1,
      aux_sym_probability_token2,
    STATE(12), 1,
      sym_expression,
    ACTIONS(37), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(45), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(30), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [744] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(33), 1,
      anon_sym_LPAREN,
    ACTIONS(39), 1,
      anon_sym_if,
    ACTIONS(41), 1,
      sym_identifier,
    ACTIONS(43), 1,
      sym_number,
    ACTIONS(47), 1,
      aux_sym_probability_token2,
    STATE(70), 1,
      sym_expression,
    ACTIONS(37), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(45), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(30), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [782] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(33), 1,
      anon_sym_LPAREN,
    ACTIONS(39), 1,
      anon_sym_if,
    ACTIONS(41), 1,
      sym_identifier,
    ACTIONS(43), 1,
      sym_number,
    ACTIONS(47), 1,
      aux_sym_probability_token2,
    STATE(78), 1,
      sym_expression,
    ACTIONS(37), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(45), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(30), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [820] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(33), 1,
      anon_sym_LPAREN,
    ACTIONS(39), 1,
      anon_sym_if,
    ACTIONS(41), 1,
      sym_identifier,
    ACTIONS(43), 1,
      sym_number,
    ACTIONS(47), 1,
      aux_sym_probability_token2,
    STATE(10), 1,
      sym_expression,
    ACTIONS(37), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(45), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(30), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [858] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(33), 1,
      anon_sym_LPAREN,
    ACTIONS(39), 1,
      anon_sym_if,
    ACTIONS(41), 1,
      sym_identifier,
    ACTIONS(43), 1,
      sym_number,
    ACTIONS(47), 1,
      aux_sym_probability_token2,
    STATE(80), 1,
      sym_expression,
    ACTIONS(37), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(45), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(30), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [896] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(33), 1,
      anon_sym_LPAREN,
    ACTIONS(39), 1,
      anon_sym_if,
    ACTIONS(41), 1,
      sym_identifier,
    ACTIONS(43), 1,
      sym_number,
    ACTIONS(47), 1,
      aux_sym_probability_token2,
    STATE(49), 1,
      sym_expression,
    ACTIONS(37), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(45), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(30), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [934] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(31), 1,
      anon_sym_SLASH,
    ACTIONS(27), 15,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
      anon_sym_COMMA,
      anon_sym_RPAREN,
      anon_sym_STAR,
      anon_sym_PLUS,
      anon_sym_DASH,
      anon_sym_CARET,
      anon_sym_then,
      anon_sym_else,
  [958] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(33), 1,
      anon_sym_LPAREN,
    ACTIONS(39), 1,
      anon_sym_if,
    ACTIONS(41), 1,
      sym_identifier,
    ACTIONS(43), 1,
      sym_number,
    ACTIONS(47), 1,
      aux_sym_probability_token2,
    STATE(91), 1,
      sym_expression,
    ACTIONS(37), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(45), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(30), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [996] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(33), 1,
      anon_sym_LPAREN,
    ACTIONS(39), 1,
      anon_sym_if,
    ACTIONS(41), 1,
      sym_identifier,
    ACTIONS(43), 1,
      sym_number,
    ACTIONS(47), 1,
      aux_sym_probability_token2,
    STATE(9), 1,
      sym_expression,
    ACTIONS(37), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(45), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(30), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [1034] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(33), 1,
      anon_sym_LPAREN,
    ACTIONS(39), 1,
      anon_sym_if,
    ACTIONS(41), 1,
      sym_identifier,
    ACTIONS(43), 1,
      sym_number,
    ACTIONS(47), 1,
      aux_sym_probability_token2,
    STATE(69), 1,
      sym_expression,
    ACTIONS(37), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(45), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(30), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [1072] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(33), 1,
      anon_sym_LPAREN,
    ACTIONS(39), 1,
      anon_sym_if,
    ACTIONS(41), 1,
      sym_identifier,
    ACTIONS(43), 1,
      sym_number,
    ACTIONS(47), 1,
      aux_sym_probability_token2,
    STATE(68), 1,
      sym_expression,
    ACTIONS(37), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(45), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(30), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [1110] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(77), 1,
      anon_sym_SLASH,
    ACTIONS(75), 15,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
      anon_sym_COMMA,
      anon_sym_RPAREN,
      anon_sym_STAR,
      anon_sym_PLUS,
      anon_sym_DASH,
      anon_sym_CARET,
      anon_sym_then,
      anon_sym_else,
  [1134] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(33), 1,
      anon_sym_LPAREN,
    ACTIONS(39), 1,
      anon_sym_if,
    ACTIONS(41), 1,
      sym_identifier,
    ACTIONS(43), 1,
      sym_number,
    ACTIONS(47), 1,
      aux_sym_probability_token2,
    STATE(52), 1,
      sym_expression,
    ACTIONS(37), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(45), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(30), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [1172] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(33), 1,
      anon_sym_LPAREN,
    ACTIONS(39), 1,
      anon_sym_if,
    ACTIONS(41), 1,
      sym_identifier,
    ACTIONS(43), 1,
      sym_number,
    ACTIONS(47), 1,
      aux_sym_probability_token2,
    STATE(72), 1,
      sym_expression,
    ACTIONS(37), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(45), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(30), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [1210] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(33), 1,
      anon_sym_LPAREN,
    ACTIONS(39), 1,
      anon_sym_if,
    ACTIONS(41), 1,
      sym_identifier,
    ACTIONS(43), 1,
      sym_number,
    ACTIONS(47), 1,
      aux_sym_probability_token2,
    STATE(73), 1,
      sym_expression,
    ACTIONS(37), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(45), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(30), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [1248] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(51), 1,
      anon_sym_STAR,
    ACTIONS(53), 1,
      anon_sym_SLASH,
    ACTIONS(81), 1,
      anon_sym_CARET,
    ACTIONS(55), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
    ACTIONS(79), 11,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
      anon_sym_COMMA,
      anon_sym_RPAREN,
      anon_sym_then,
      anon_sym_else,
  [1278] = 9,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(83), 1,
      ts_builtin_sym_end,
    ACTIONS(85), 1,
      anon_sym_question,
    ACTIONS(88), 1,
      anon_sym_driver,
    ACTIONS(91), 1,
      anon_sym_evidence,
    ACTIONS(94), 1,
      anon_sym_agent,
    ACTIONS(97), 1,
      anon_sym_model,
    ACTIONS(100), 1,
      anon_sym_simulate,
    STATE(40), 8,
      sym__statement,
      sym_question_statement,
      sym_driver_statement,
      sym_evidence_statement,
      sym_agent_statement,
      sym_model_statement,
      sym_simulate_statement,
      aux_sym_source_file_repeat1,
  [1313] = 9,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(7), 1,
      anon_sym_question,
    ACTIONS(9), 1,
      anon_sym_driver,
    ACTIONS(11), 1,
      anon_sym_evidence,
    ACTIONS(13), 1,
      anon_sym_agent,
    ACTIONS(15), 1,
      anon_sym_model,
    ACTIONS(17), 1,
      anon_sym_simulate,
    ACTIONS(103), 1,
      ts_builtin_sym_end,
    STATE(40), 8,
      sym__statement,
      sym_question_statement,
      sym_driver_statement,
      sym_evidence_statement,
      sym_agent_statement,
      sym_model_statement,
      sym_simulate_statement,
      aux_sym_source_file_repeat1,
  [1348] = 9,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(105), 1,
      anon_sym_RBRACE,
    ACTIONS(107), 1,
      anon_sym_distribution,
    ACTIONS(109), 1,
      anon_sym_probability,
    ACTIONS(111), 1,
      anon_sym_unit,
    ACTIONS(113), 1,
      anon_sym_rationale,
    ACTIONS(115), 1,
      anon_sym_impact_multiplier,
    STATE(43), 2,
      sym_driver_property,
      aux_sym_driver_block_repeat1,
    STATE(65), 5,
      sym_distribution_property,
      sym_probability_property,
      sym_unit_property,
      sym_rationale_property,
      sym_impact_multiplier_property,
  [1381] = 9,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(107), 1,
      anon_sym_distribution,
    ACTIONS(109), 1,
      anon_sym_probability,
    ACTIONS(111), 1,
      anon_sym_unit,
    ACTIONS(113), 1,
      anon_sym_rationale,
    ACTIONS(115), 1,
      anon_sym_impact_multiplier,
    ACTIONS(117), 1,
      anon_sym_RBRACE,
    STATE(44), 2,
      sym_driver_property,
      aux_sym_driver_block_repeat1,
    STATE(65), 5,
      sym_distribution_property,
      sym_probability_property,
      sym_unit_property,
      sym_rationale_property,
      sym_impact_multiplier_property,
  [1414] = 9,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(119), 1,
      anon_sym_RBRACE,
    ACTIONS(121), 1,
      anon_sym_distribution,
    ACTIONS(124), 1,
      anon_sym_probability,
    ACTIONS(127), 1,
      anon_sym_unit,
    ACTIONS(130), 1,
      anon_sym_rationale,
    ACTIONS(133), 1,
      anon_sym_impact_multiplier,
    STATE(44), 2,
      sym_driver_property,
      aux_sym_driver_block_repeat1,
    STATE(65), 5,
      sym_distribution_property,
      sym_probability_property,
      sym_unit_property,
      sym_rationale_property,
      sym_impact_multiplier_property,
  [1447] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(51), 1,
      anon_sym_STAR,
    ACTIONS(53), 1,
      anon_sym_SLASH,
    ACTIONS(81), 1,
      anon_sym_CARET,
    ACTIONS(55), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
    ACTIONS(136), 7,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
  [1473] = 8,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(138), 1,
      anon_sym_triangular,
    ACTIONS(140), 1,
      anon_sym_normal,
    ACTIONS(142), 1,
      anon_sym_lognormal,
    ACTIONS(144), 1,
      anon_sym_uniform,
    ACTIONS(146), 1,
      anon_sym_beta,
    STATE(85), 1,
      sym_distribution,
    STATE(86), 5,
      sym_triangular_distribution,
      sym_normal_distribution,
      sym_lognormal_distribution,
      sym_uniform_distribution,
      sym_beta_distribution,
  [1502] = 8,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(51), 1,
      anon_sym_STAR,
    ACTIONS(53), 1,
      anon_sym_SLASH,
    ACTIONS(81), 1,
      anon_sym_CARET,
    ACTIONS(148), 1,
      anon_sym_COMMA,
    ACTIONS(150), 1,
      anon_sym_RPAREN,
    STATE(106), 1,
      aux_sym_function_call_repeat1,
    ACTIONS(55), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [1528] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(152), 7,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
  [1541] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(51), 1,
      anon_sym_STAR,
    ACTIONS(53), 1,
      anon_sym_SLASH,
    ACTIONS(81), 1,
      anon_sym_CARET,
    ACTIONS(55), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
    ACTIONS(154), 2,
      anon_sym_COMMA,
      anon_sym_RPAREN,
  [1562] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(156), 7,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
  [1575] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(158), 7,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
  [1588] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(51), 1,
      anon_sym_STAR,
    ACTIONS(53), 1,
      anon_sym_SLASH,
    ACTIONS(81), 1,
      anon_sym_CARET,
    ACTIONS(160), 1,
      anon_sym_COMMA,
    ACTIONS(162), 1,
      anon_sym_RPAREN,
    ACTIONS(55), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [1611] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(164), 1,
      anon_sym_RBRACE,
    ACTIONS(168), 1,
      anon_sym_relevance,
    ACTIONS(170), 1,
      anon_sym_date,
    ACTIONS(166), 2,
      anon_sym_source,
      anon_sym_summary,
    STATE(59), 2,
      sym_evidence_property,
      aux_sym_evidence_block_repeat1,
  [1632] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(172), 7,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
  [1645] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(174), 7,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
  [1658] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(176), 7,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
  [1671] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(178), 1,
      anon_sym_RBRACE,
    ACTIONS(183), 1,
      anon_sym_relevance,
    ACTIONS(186), 1,
      anon_sym_date,
    ACTIONS(180), 2,
      anon_sym_source,
      anon_sym_summary,
    STATE(57), 2,
      sym_evidence_property,
      aux_sym_evidence_block_repeat1,
  [1692] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(189), 7,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
  [1705] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(168), 1,
      anon_sym_relevance,
    ACTIONS(170), 1,
      anon_sym_date,
    ACTIONS(191), 1,
      anon_sym_RBRACE,
    ACTIONS(166), 2,
      anon_sym_source,
      anon_sym_summary,
    STATE(57), 2,
      sym_evidence_property,
      aux_sym_evidence_block_repeat1,
  [1726] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(193), 7,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
  [1739] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(195), 7,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
  [1752] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(197), 7,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
  [1765] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(199), 7,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
  [1778] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(51), 1,
      anon_sym_STAR,
    ACTIONS(53), 1,
      anon_sym_SLASH,
    ACTIONS(81), 1,
      anon_sym_CARET,
    ACTIONS(201), 1,
      anon_sym_COMMA,
    ACTIONS(55), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [1798] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(203), 6,
      anon_sym_RBRACE,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
  [1810] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(51), 1,
      anon_sym_STAR,
    ACTIONS(53), 1,
      anon_sym_SLASH,
    ACTIONS(81), 1,
      anon_sym_CARET,
    ACTIONS(205), 1,
      anon_sym_COMMA,
    ACTIONS(55), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [1830] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(51), 1,
      anon_sym_STAR,
    ACTIONS(53), 1,
      anon_sym_SLASH,
    ACTIONS(81), 1,
      anon_sym_CARET,
    ACTIONS(207), 1,
      anon_sym_COMMA,
    ACTIONS(55), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [1850] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(51), 1,
      anon_sym_STAR,
    ACTIONS(53), 1,
      anon_sym_SLASH,
    ACTIONS(81), 1,
      anon_sym_CARET,
    ACTIONS(209), 1,
      anon_sym_RPAREN,
    ACTIONS(55), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [1870] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(51), 1,
      anon_sym_STAR,
    ACTIONS(53), 1,
      anon_sym_SLASH,
    ACTIONS(81), 1,
      anon_sym_CARET,
    ACTIONS(211), 1,
      anon_sym_COMMA,
    ACTIONS(55), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [1890] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(51), 1,
      anon_sym_STAR,
    ACTIONS(53), 1,
      anon_sym_SLASH,
    ACTIONS(81), 1,
      anon_sym_CARET,
    ACTIONS(213), 1,
      anon_sym_COMMA,
    ACTIONS(55), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [1910] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(51), 1,
      anon_sym_STAR,
    ACTIONS(53), 1,
      anon_sym_SLASH,
    ACTIONS(81), 1,
      anon_sym_CARET,
    ACTIONS(215), 1,
      anon_sym_COMMA,
    ACTIONS(55), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [1930] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(51), 1,
      anon_sym_STAR,
    ACTIONS(53), 1,
      anon_sym_SLASH,
    ACTIONS(81), 1,
      anon_sym_CARET,
    ACTIONS(217), 1,
      anon_sym_RPAREN,
    ACTIONS(55), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [1950] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(51), 1,
      anon_sym_STAR,
    ACTIONS(53), 1,
      anon_sym_SLASH,
    ACTIONS(81), 1,
      anon_sym_CARET,
    ACTIONS(219), 1,
      anon_sym_RPAREN,
    ACTIONS(55), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [1970] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(221), 6,
      anon_sym_RBRACE,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
  [1982] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(223), 6,
      anon_sym_RBRACE,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
  [1994] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(225), 6,
      anon_sym_RBRACE,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
  [2006] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(227), 6,
      anon_sym_RBRACE,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
  [2018] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(51), 1,
      anon_sym_STAR,
    ACTIONS(53), 1,
      anon_sym_SLASH,
    ACTIONS(81), 1,
      anon_sym_CARET,
    ACTIONS(229), 1,
      anon_sym_RPAREN,
    ACTIONS(55), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [2038] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(231), 6,
      anon_sym_RBRACE,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
  [2050] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(51), 1,
      anon_sym_STAR,
    ACTIONS(53), 1,
      anon_sym_SLASH,
    ACTIONS(81), 1,
      anon_sym_CARET,
    ACTIONS(233), 1,
      anon_sym_then,
    ACTIONS(55), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [2070] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(51), 1,
      anon_sym_STAR,
    ACTIONS(53), 1,
      anon_sym_SLASH,
    ACTIONS(81), 1,
      anon_sym_CARET,
    ACTIONS(235), 1,
      anon_sym_RPAREN,
    ACTIONS(55), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [2090] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(51), 1,
      anon_sym_STAR,
    ACTIONS(53), 1,
      anon_sym_SLASH,
    ACTIONS(81), 1,
      anon_sym_CARET,
    ACTIONS(237), 1,
      anon_sym_else,
    ACTIONS(55), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [2110] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(51), 1,
      anon_sym_STAR,
    ACTIONS(53), 1,
      anon_sym_SLASH,
    ACTIONS(81), 1,
      anon_sym_CARET,
    ACTIONS(239), 1,
      anon_sym_COMMA,
    ACTIONS(55), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [2130] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(241), 6,
      anon_sym_RBRACE,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
  [2142] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(243), 6,
      anon_sym_RBRACE,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
  [2154] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(245), 6,
      anon_sym_RBRACE,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
  [2166] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(247), 6,
      anon_sym_RBRACE,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
  [2178] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(249), 6,
      anon_sym_RBRACE,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
  [2190] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(251), 6,
      anon_sym_RBRACE,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
  [2202] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(253), 6,
      anon_sym_RBRACE,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
  [2214] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(51), 1,
      anon_sym_STAR,
    ACTIONS(53), 1,
      anon_sym_SLASH,
    ACTIONS(81), 1,
      anon_sym_CARET,
    ACTIONS(255), 1,
      anon_sym_RPAREN,
    ACTIONS(55), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [2234] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(257), 1,
      anon_sym_RBRACE,
    ACTIONS(259), 1,
      anon_sym_query,
    ACTIONS(262), 1,
      anon_sym_schedule,
    STATE(92), 2,
      sym_agent_property,
      aux_sym_agent_block_repeat1,
  [2251] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(265), 1,
      sym_number,
    STATE(87), 1,
      sym_probability,
    ACTIONS(45), 3,
      aux_sym_probability_token1,
      aux_sym_probability_token2,
      aux_sym_probability_token3,
  [2266] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(267), 1,
      anon_sym_RBRACE,
    ACTIONS(269), 1,
      anon_sym_query,
    ACTIONS(271), 1,
      anon_sym_schedule,
    STATE(95), 2,
      sym_agent_property,
      aux_sym_agent_block_repeat1,
  [2283] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(269), 1,
      anon_sym_query,
    ACTIONS(271), 1,
      anon_sym_schedule,
    ACTIONS(273), 1,
      anon_sym_RBRACE,
    STATE(92), 2,
      sym_agent_property,
      aux_sym_agent_block_repeat1,
  [2300] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(275), 1,
      sym_number,
    STATE(97), 1,
      sym_probability,
    ACTIONS(45), 3,
      aux_sym_probability_token1,
      aux_sym_probability_token2,
      aux_sym_probability_token3,
  [2315] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(277), 5,
      anon_sym_RBRACE,
      anon_sym_source,
      anon_sym_summary,
      anon_sym_relevance,
      anon_sym_date,
  [2326] = 5,
    ACTIONS(279), 1,
      anon_sym_DQUOTE,
    ACTIONS(281), 1,
      aux_sym_string_token1,
    ACTIONS(283), 1,
      anon_sym_BSLASH,
    ACTIONS(285), 1,
      sym_comment,
    STATE(100), 1,
      aux_sym_string_repeat1,
  [2342] = 5,
    ACTIONS(283), 1,
      anon_sym_BSLASH,
    ACTIONS(285), 1,
      sym_comment,
    ACTIONS(287), 1,
      anon_sym_DQUOTE,
    ACTIONS(289), 1,
      aux_sym_string_token1,
    STATE(98), 1,
      aux_sym_string_repeat1,
  [2358] = 5,
    ACTIONS(285), 1,
      sym_comment,
    ACTIONS(291), 1,
      anon_sym_DQUOTE,
    ACTIONS(293), 1,
      aux_sym_string_token1,
    ACTIONS(296), 1,
      anon_sym_BSLASH,
    STATE(100), 1,
      aux_sym_string_repeat1,
  [2374] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(299), 2,
      anon_sym_day,
      anon_sym_week,
    ACTIONS(301), 2,
      anon_sym_days,
      anon_sym_weeks,
  [2386] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(303), 1,
      anon_sym_COMMA,
    ACTIONS(306), 1,
      anon_sym_RPAREN,
    STATE(102), 1,
      aux_sym_function_call_repeat1,
  [2399] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(308), 3,
      anon_sym_RBRACE,
      anon_sym_query,
      anon_sym_schedule,
  [2408] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(310), 3,
      anon_sym_continuous,
      anon_sym_binary,
      anon_sym_discrete,
  [2417] = 2,
    ACTIONS(285), 1,
      sym_comment,
    ACTIONS(291), 3,
      anon_sym_DQUOTE,
      aux_sym_string_token1,
      anon_sym_BSLASH,
  [2426] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(148), 1,
      anon_sym_COMMA,
    ACTIONS(312), 1,
      anon_sym_RPAREN,
    STATE(102), 1,
      aux_sym_function_call_repeat1,
  [2439] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(314), 3,
      anon_sym_RBRACE,
      anon_sym_query,
      anon_sym_schedule,
  [2448] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(316), 1,
      anon_sym_DQUOTE,
    STATE(97), 1,
      sym_string,
  [2458] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(316), 1,
      anon_sym_DQUOTE,
    STATE(51), 1,
      sym_string,
  [2468] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(316), 1,
      anon_sym_DQUOTE,
    STATE(89), 1,
      sym_string,
  [2478] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(316), 1,
      anon_sym_DQUOTE,
    STATE(103), 1,
      sym_string,
  [2488] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(318), 1,
      anon_sym_LBRACE,
    STATE(48), 1,
      sym_evidence_block,
  [2498] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(320), 1,
      anon_sym_LBRACE,
    STATE(61), 1,
      sym_agent_block,
  [2508] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(322), 1,
      anon_sym_LBRACE,
    STATE(62), 1,
      sym_driver_block,
  [2518] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(316), 1,
      anon_sym_DQUOTE,
    STATE(88), 1,
      sym_string,
  [2528] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(324), 1,
      anon_sym_COLON,
  [2535] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(326), 1,
      anon_sym_COLON,
  [2542] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(328), 1,
      anon_sym_COLON,
  [2549] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(330), 1,
      anon_sym_COLON,
  [2556] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(332), 1,
      anon_sym_LPAREN,
  [2563] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(334), 1,
      anon_sym_LPAREN,
  [2570] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(336), 1,
      sym_date,
  [2577] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(338), 1,
      anon_sym_COLON,
  [2584] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(340), 1,
      anon_sym_COLON,
  [2591] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(342), 1,
      anon_sym_COLON,
  [2598] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(344), 1,
      anon_sym_every,
  [2605] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(346), 1,
      sym_number,
  [2612] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(348), 1,
      anon_sym_LPAREN,
  [2619] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(350), 1,
      anon_sym_COLON,
  [2626] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(352), 1,
      anon_sym_LPAREN,
  [2633] = 2,
    ACTIONS(285), 1,
      sym_comment,
    ACTIONS(354), 1,
      aux_sym_string_token2,
  [2640] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(356), 1,
      anon_sym_iterations,
  [2647] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(358), 1,
      anon_sym_COLON,
  [2654] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(360), 1,
      anon_sym_LPAREN,
  [2661] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(362), 1,
      anon_sym_COLON,
  [2668] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(364), 1,
      sym_number,
  [2675] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(366), 1,
      ts_builtin_sym_end,
  [2682] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(368), 1,
      sym_number,
  [2689] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(370), 1,
      anon_sym_COLON,
  [2696] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(372), 1,
      sym_identifier,
  [2703] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(374), 1,
      sym_identifier,
  [2710] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(376), 1,
      sym_identifier,
};

static const uint32_t ts_small_parse_table_map[] = {
  [SMALL_STATE(2)] = 0,
  [SMALL_STATE(3)] = 34,
  [SMALL_STATE(4)] = 59,
  [SMALL_STATE(5)] = 84,
  [SMALL_STATE(6)] = 111,
  [SMALL_STATE(7)] = 152,
  [SMALL_STATE(8)] = 190,
  [SMALL_STATE(9)] = 228,
  [SMALL_STATE(10)] = 256,
  [SMALL_STATE(11)] = 280,
  [SMALL_STATE(12)] = 318,
  [SMALL_STATE(13)] = 344,
  [SMALL_STATE(14)] = 368,
  [SMALL_STATE(15)] = 406,
  [SMALL_STATE(16)] = 430,
  [SMALL_STATE(17)] = 468,
  [SMALL_STATE(18)] = 506,
  [SMALL_STATE(19)] = 544,
  [SMALL_STATE(20)] = 582,
  [SMALL_STATE(21)] = 606,
  [SMALL_STATE(22)] = 644,
  [SMALL_STATE(23)] = 668,
  [SMALL_STATE(24)] = 706,
  [SMALL_STATE(25)] = 744,
  [SMALL_STATE(26)] = 782,
  [SMALL_STATE(27)] = 820,
  [SMALL_STATE(28)] = 858,
  [SMALL_STATE(29)] = 896,
  [SMALL_STATE(30)] = 934,
  [SMALL_STATE(31)] = 958,
  [SMALL_STATE(32)] = 996,
  [SMALL_STATE(33)] = 1034,
  [SMALL_STATE(34)] = 1072,
  [SMALL_STATE(35)] = 1110,
  [SMALL_STATE(36)] = 1134,
  [SMALL_STATE(37)] = 1172,
  [SMALL_STATE(38)] = 1210,
  [SMALL_STATE(39)] = 1248,
  [SMALL_STATE(40)] = 1278,
  [SMALL_STATE(41)] = 1313,
  [SMALL_STATE(42)] = 1348,
  [SMALL_STATE(43)] = 1381,
  [SMALL_STATE(44)] = 1414,
  [SMALL_STATE(45)] = 1447,
  [SMALL_STATE(46)] = 1473,
  [SMALL_STATE(47)] = 1502,
  [SMALL_STATE(48)] = 1528,
  [SMALL_STATE(49)] = 1541,
  [SMALL_STATE(50)] = 1562,
  [SMALL_STATE(51)] = 1575,
  [SMALL_STATE(52)] = 1588,
  [SMALL_STATE(53)] = 1611,
  [SMALL_STATE(54)] = 1632,
  [SMALL_STATE(55)] = 1645,
  [SMALL_STATE(56)] = 1658,
  [SMALL_STATE(57)] = 1671,
  [SMALL_STATE(58)] = 1692,
  [SMALL_STATE(59)] = 1705,
  [SMALL_STATE(60)] = 1726,
  [SMALL_STATE(61)] = 1739,
  [SMALL_STATE(62)] = 1752,
  [SMALL_STATE(63)] = 1765,
  [SMALL_STATE(64)] = 1778,
  [SMALL_STATE(65)] = 1798,
  [SMALL_STATE(66)] = 1810,
  [SMALL_STATE(67)] = 1830,
  [SMALL_STATE(68)] = 1850,
  [SMALL_STATE(69)] = 1870,
  [SMALL_STATE(70)] = 1890,
  [SMALL_STATE(71)] = 1910,
  [SMALL_STATE(72)] = 1930,
  [SMALL_STATE(73)] = 1950,
  [SMALL_STATE(74)] = 1970,
  [SMALL_STATE(75)] = 1982,
  [SMALL_STATE(76)] = 1994,
  [SMALL_STATE(77)] = 2006,
  [SMALL_STATE(78)] = 2018,
  [SMALL_STATE(79)] = 2038,
  [SMALL_STATE(80)] = 2050,
  [SMALL_STATE(81)] = 2070,
  [SMALL_STATE(82)] = 2090,
  [SMALL_STATE(83)] = 2110,
  [SMALL_STATE(84)] = 2130,
  [SMALL_STATE(85)] = 2142,
  [SMALL_STATE(86)] = 2154,
  [SMALL_STATE(87)] = 2166,
  [SMALL_STATE(88)] = 2178,
  [SMALL_STATE(89)] = 2190,
  [SMALL_STATE(90)] = 2202,
  [SMALL_STATE(91)] = 2214,
  [SMALL_STATE(92)] = 2234,
  [SMALL_STATE(93)] = 2251,
  [SMALL_STATE(94)] = 2266,
  [SMALL_STATE(95)] = 2283,
  [SMALL_STATE(96)] = 2300,
  [SMALL_STATE(97)] = 2315,
  [SMALL_STATE(98)] = 2326,
  [SMALL_STATE(99)] = 2342,
  [SMALL_STATE(100)] = 2358,
  [SMALL_STATE(101)] = 2374,
  [SMALL_STATE(102)] = 2386,
  [SMALL_STATE(103)] = 2399,
  [SMALL_STATE(104)] = 2408,
  [SMALL_STATE(105)] = 2417,
  [SMALL_STATE(106)] = 2426,
  [SMALL_STATE(107)] = 2439,
  [SMALL_STATE(108)] = 2448,
  [SMALL_STATE(109)] = 2458,
  [SMALL_STATE(110)] = 2468,
  [SMALL_STATE(111)] = 2478,
  [SMALL_STATE(112)] = 2488,
  [SMALL_STATE(113)] = 2498,
  [SMALL_STATE(114)] = 2508,
  [SMALL_STATE(115)] = 2518,
  [SMALL_STATE(116)] = 2528,
  [SMALL_STATE(117)] = 2535,
  [SMALL_STATE(118)] = 2542,
  [SMALL_STATE(119)] = 2549,
  [SMALL_STATE(120)] = 2556,
  [SMALL_STATE(121)] = 2563,
  [SMALL_STATE(122)] = 2570,
  [SMALL_STATE(123)] = 2577,
  [SMALL_STATE(124)] = 2584,
  [SMALL_STATE(125)] = 2591,
  [SMALL_STATE(126)] = 2598,
  [SMALL_STATE(127)] = 2605,
  [SMALL_STATE(128)] = 2612,
  [SMALL_STATE(129)] = 2619,
  [SMALL_STATE(130)] = 2626,
  [SMALL_STATE(131)] = 2633,
  [SMALL_STATE(132)] = 2640,
  [SMALL_STATE(133)] = 2647,
  [SMALL_STATE(134)] = 2654,
  [SMALL_STATE(135)] = 2661,
  [SMALL_STATE(136)] = 2668,
  [SMALL_STATE(137)] = 2675,
  [SMALL_STATE(138)] = 2682,
  [SMALL_STATE(139)] = 2689,
  [SMALL_STATE(140)] = 2696,
  [SMALL_STATE(141)] = 2703,
  [SMALL_STATE(142)] = 2710,
};

static const TSParseActionEntry ts_parse_actions[] = {
  [0] = {.entry = {.count = 0, .reusable = false}},
  [1] = {.entry = {.count = 1, .reusable = false}}, RECOVER(),
  [3] = {.entry = {.count = 1, .reusable = true}}, SHIFT_EXTRA(),
  [5] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_source_file, 0, 0, 0),
  [7] = {.entry = {.count = 1, .reusable = true}}, SHIFT(109),
  [9] = {.entry = {.count = 1, .reusable = true}}, SHIFT(142),
  [11] = {.entry = {.count = 1, .reusable = true}}, SHIFT(141),
  [13] = {.entry = {.count = 1, .reusable = true}}, SHIFT(140),
  [15] = {.entry = {.count = 1, .reusable = true}}, SHIFT(139),
  [17] = {.entry = {.count = 1, .reusable = true}}, SHIFT(138),
  [19] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_probability, 1, 0, 0),
  [21] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_probability, 1, 0, 0),
  [23] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_string, 2, 0, 0),
  [25] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_string, 3, 0, 0),
  [27] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_expression, 1, 0, 0),
  [29] = {.entry = {.count = 1, .reusable = true}}, SHIFT(6),
  [31] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_expression, 1, 0, 0),
  [33] = {.entry = {.count = 1, .reusable = true}}, SHIFT(26),
  [35] = {.entry = {.count = 1, .reusable = true}}, SHIFT(15),
  [37] = {.entry = {.count = 1, .reusable = true}}, SHIFT(27),
  [39] = {.entry = {.count = 1, .reusable = false}}, SHIFT(28),
  [41] = {.entry = {.count = 1, .reusable = false}}, SHIFT(5),
  [43] = {.entry = {.count = 1, .reusable = false}}, SHIFT(30),
  [45] = {.entry = {.count = 1, .reusable = true}}, SHIFT(2),
  [47] = {.entry = {.count = 1, .reusable = false}}, SHIFT(2),
  [49] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_binary_expression, 3, 0, 8),
  [51] = {.entry = {.count = 1, .reusable = true}}, SHIFT(14),
  [53] = {.entry = {.count = 1, .reusable = false}}, SHIFT(14),
  [55] = {.entry = {.count = 1, .reusable = true}}, SHIFT(24),
  [57] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_unary_expression, 2, 0, 6),
  [59] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_unary_expression, 2, 0, 6),
  [61] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_binary_expression, 3, 0, 8),
  [63] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_function_call, 3, 0, 7),
  [65] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_function_call, 3, 0, 7),
  [67] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_parenthesized_expression, 3, 0, 0),
  [69] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_parenthesized_expression, 3, 0, 0),
  [71] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_function_call, 5, 0, 13),
  [73] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_function_call, 5, 0, 13),
  [75] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_function_call, 4, 0, 10),
  [77] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_function_call, 4, 0, 10),
  [79] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_conditional_expression, 6, 0, 16),
  [81] = {.entry = {.count = 1, .reusable = true}}, SHIFT(32),
  [83] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2, 0, 0),
  [85] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2, 0, 0), SHIFT_REPEAT(109),
  [88] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2, 0, 0), SHIFT_REPEAT(142),
  [91] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2, 0, 0), SHIFT_REPEAT(141),
  [94] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2, 0, 0), SHIFT_REPEAT(140),
  [97] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2, 0, 0), SHIFT_REPEAT(139),
  [100] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2, 0, 0), SHIFT_REPEAT(138),
  [103] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_source_file, 1, 0, 0),
  [105] = {.entry = {.count = 1, .reusable = true}}, SHIFT(55),
  [107] = {.entry = {.count = 1, .reusable = true}}, SHIFT(119),
  [109] = {.entry = {.count = 1, .reusable = true}}, SHIFT(118),
  [111] = {.entry = {.count = 1, .reusable = true}}, SHIFT(116),
  [113] = {.entry = {.count = 1, .reusable = true}}, SHIFT(129),
  [115] = {.entry = {.count = 1, .reusable = true}}, SHIFT(117),
  [117] = {.entry = {.count = 1, .reusable = true}}, SHIFT(50),
  [119] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_driver_block_repeat1, 2, 0, 0),
  [121] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_driver_block_repeat1, 2, 0, 0), SHIFT_REPEAT(119),
  [124] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_driver_block_repeat1, 2, 0, 0), SHIFT_REPEAT(118),
  [127] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_driver_block_repeat1, 2, 0, 0), SHIFT_REPEAT(116),
  [130] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_driver_block_repeat1, 2, 0, 0), SHIFT_REPEAT(129),
  [133] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_driver_block_repeat1, 2, 0, 0), SHIFT_REPEAT(117),
  [136] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_model_statement, 3, 0, 3),
  [138] = {.entry = {.count = 1, .reusable = true}}, SHIFT(130),
  [140] = {.entry = {.count = 1, .reusable = true}}, SHIFT(134),
  [142] = {.entry = {.count = 1, .reusable = true}}, SHIFT(128),
  [144] = {.entry = {.count = 1, .reusable = true}}, SHIFT(121),
  [146] = {.entry = {.count = 1, .reusable = true}}, SHIFT(120),
  [148] = {.entry = {.count = 1, .reusable = true}}, SHIFT(29),
  [150] = {.entry = {.count = 1, .reusable = true}}, SHIFT(35),
  [152] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_evidence_statement, 3, 0, 2),
  [154] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_function_call_repeat1, 2, 0, 12),
  [156] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_driver_block, 3, 0, 0),
  [158] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_question_statement, 2, 0, 1),
  [160] = {.entry = {.count = 1, .reusable = true}}, SHIFT(11),
  [162] = {.entry = {.count = 1, .reusable = true}}, SHIFT(79),
  [164] = {.entry = {.count = 1, .reusable = true}}, SHIFT(60),
  [166] = {.entry = {.count = 1, .reusable = true}}, SHIFT(125),
  [168] = {.entry = {.count = 1, .reusable = true}}, SHIFT(124),
  [170] = {.entry = {.count = 1, .reusable = true}}, SHIFT(123),
  [172] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_agent_block, 2, 0, 0),
  [174] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_driver_block, 2, 0, 0),
  [176] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_agent_block, 3, 0, 0),
  [178] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_evidence_block_repeat1, 2, 0, 0),
  [180] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_evidence_block_repeat1, 2, 0, 0), SHIFT_REPEAT(125),
  [183] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_evidence_block_repeat1, 2, 0, 0), SHIFT_REPEAT(124),
  [186] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_evidence_block_repeat1, 2, 0, 0), SHIFT_REPEAT(123),
  [189] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_evidence_block, 3, 0, 0),
  [191] = {.entry = {.count = 1, .reusable = true}}, SHIFT(58),
  [193] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_evidence_block, 2, 0, 0),
  [195] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_agent_statement, 3, 0, 2),
  [197] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_driver_statement, 4, 0, 5),
  [199] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_simulate_statement, 3, 0, 4),
  [201] = {.entry = {.count = 1, .reusable = true}}, SHIFT(17),
  [203] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_driver_property, 1, 0, 0),
  [205] = {.entry = {.count = 1, .reusable = true}}, SHIFT(18),
  [207] = {.entry = {.count = 1, .reusable = true}}, SHIFT(34),
  [209] = {.entry = {.count = 1, .reusable = true}}, SHIFT(75),
  [211] = {.entry = {.count = 1, .reusable = true}}, SHIFT(38),
  [213] = {.entry = {.count = 1, .reusable = true}}, SHIFT(37),
  [215] = {.entry = {.count = 1, .reusable = true}}, SHIFT(36),
  [217] = {.entry = {.count = 1, .reusable = true}}, SHIFT(76),
  [219] = {.entry = {.count = 1, .reusable = true}}, SHIFT(77),
  [221] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_beta_distribution, 10, 0, 22),
  [223] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_normal_distribution, 6, 0, 17),
  [225] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_lognormal_distribution, 6, 0, 18),
  [227] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_uniform_distribution, 6, 0, 19),
  [229] = {.entry = {.count = 1, .reusable = true}}, SHIFT(20),
  [231] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_beta_distribution, 6, 0, 20),
  [233] = {.entry = {.count = 1, .reusable = true}}, SHIFT(19),
  [235] = {.entry = {.count = 1, .reusable = true}}, SHIFT(84),
  [237] = {.entry = {.count = 1, .reusable = true}}, SHIFT(21),
  [239] = {.entry = {.count = 1, .reusable = true}}, SHIFT(31),
  [241] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_triangular_distribution, 8, 0, 21),
  [243] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_distribution_property, 3, 0, 11),
  [245] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_distribution, 1, 0, 0),
  [247] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_probability_property, 3, 0, 9),
  [249] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_unit_property, 3, 0, 9),
  [251] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_rationale_property, 3, 0, 9),
  [253] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_impact_multiplier_property, 3, 0, 9),
  [255] = {.entry = {.count = 1, .reusable = true}}, SHIFT(74),
  [257] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_agent_block_repeat1, 2, 0, 0),
  [259] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_agent_block_repeat1, 2, 0, 0), SHIFT_REPEAT(135),
  [262] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_agent_block_repeat1, 2, 0, 0), SHIFT_REPEAT(133),
  [265] = {.entry = {.count = 1, .reusable = false}}, SHIFT(87),
  [267] = {.entry = {.count = 1, .reusable = true}}, SHIFT(54),
  [269] = {.entry = {.count = 1, .reusable = true}}, SHIFT(135),
  [271] = {.entry = {.count = 1, .reusable = true}}, SHIFT(133),
  [273] = {.entry = {.count = 1, .reusable = true}}, SHIFT(56),
  [275] = {.entry = {.count = 1, .reusable = false}}, SHIFT(97),
  [277] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_evidence_property, 3, 0, 9),
  [279] = {.entry = {.count = 1, .reusable = false}}, SHIFT(4),
  [281] = {.entry = {.count = 1, .reusable = false}}, SHIFT(100),
  [283] = {.entry = {.count = 1, .reusable = false}}, SHIFT(131),
  [285] = {.entry = {.count = 1, .reusable = false}}, SHIFT_EXTRA(),
  [287] = {.entry = {.count = 1, .reusable = false}}, SHIFT(3),
  [289] = {.entry = {.count = 1, .reusable = false}}, SHIFT(98),
  [291] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_string_repeat1, 2, 0, 0),
  [293] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_string_repeat1, 2, 0, 0), SHIFT_REPEAT(100),
  [296] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_string_repeat1, 2, 0, 0), SHIFT_REPEAT(131),
  [299] = {.entry = {.count = 1, .reusable = false}}, SHIFT(107),
  [301] = {.entry = {.count = 1, .reusable = true}}, SHIFT(107),
  [303] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_function_call_repeat1, 2, 0, 14), SHIFT_REPEAT(29),
  [306] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_function_call_repeat1, 2, 0, 14),
  [308] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_agent_property, 3, 0, 9),
  [310] = {.entry = {.count = 1, .reusable = true}}, SHIFT(114),
  [312] = {.entry = {.count = 1, .reusable = true}}, SHIFT(22),
  [314] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_agent_property, 5, 0, 15),
  [316] = {.entry = {.count = 1, .reusable = true}}, SHIFT(99),
  [318] = {.entry = {.count = 1, .reusable = true}}, SHIFT(53),
  [320] = {.entry = {.count = 1, .reusable = true}}, SHIFT(94),
  [322] = {.entry = {.count = 1, .reusable = true}}, SHIFT(42),
  [324] = {.entry = {.count = 1, .reusable = true}}, SHIFT(115),
  [326] = {.entry = {.count = 1, .reusable = true}}, SHIFT(127),
  [328] = {.entry = {.count = 1, .reusable = true}}, SHIFT(93),
  [330] = {.entry = {.count = 1, .reusable = true}}, SHIFT(46),
  [332] = {.entry = {.count = 1, .reusable = true}}, SHIFT(8),
  [334] = {.entry = {.count = 1, .reusable = true}}, SHIFT(33),
  [336] = {.entry = {.count = 1, .reusable = true}}, SHIFT(97),
  [338] = {.entry = {.count = 1, .reusable = true}}, SHIFT(122),
  [340] = {.entry = {.count = 1, .reusable = true}}, SHIFT(96),
  [342] = {.entry = {.count = 1, .reusable = true}}, SHIFT(108),
  [344] = {.entry = {.count = 1, .reusable = true}}, SHIFT(136),
  [346] = {.entry = {.count = 1, .reusable = true}}, SHIFT(90),
  [348] = {.entry = {.count = 1, .reusable = true}}, SHIFT(25),
  [350] = {.entry = {.count = 1, .reusable = true}}, SHIFT(110),
  [352] = {.entry = {.count = 1, .reusable = true}}, SHIFT(23),
  [354] = {.entry = {.count = 1, .reusable = false}}, SHIFT(105),
  [356] = {.entry = {.count = 1, .reusable = true}}, SHIFT(63),
  [358] = {.entry = {.count = 1, .reusable = true}}, SHIFT(126),
  [360] = {.entry = {.count = 1, .reusable = true}}, SHIFT(7),
  [362] = {.entry = {.count = 1, .reusable = true}}, SHIFT(111),
  [364] = {.entry = {.count = 1, .reusable = true}}, SHIFT(101),
  [366] = {.entry = {.count = 1, .reusable = true}},  ACCEPT_INPUT(),
  [368] = {.entry = {.count = 1, .reusable = true}}, SHIFT(132),
  [370] = {.entry = {.count = 1, .reusable = true}}, SHIFT(16),
  [372] = {.entry = {.count = 1, .reusable = true}}, SHIFT(113),
  [374] = {.entry = {.count = 1, .reusable = true}}, SHIFT(112),
  [376] = {.entry = {.count = 1, .reusable = true}}, SHIFT(104),
};

#ifdef __cplusplus
extern "C" {
#endif
#ifdef TREE_SITTER_HIDE_SYMBOLS
#define TS_PUBLIC
#elif defined(_WIN32)
#define TS_PUBLIC __declspec(dllexport)
#else
#define TS_PUBLIC __attribute__((visibility("default")))
#endif

TS_PUBLIC const TSLanguage *tree_sitter_fpl(void) {
  static const TSLanguage language = {
    .version = LANGUAGE_VERSION,
    .symbol_count = SYMBOL_COUNT,
    .alias_count = ALIAS_COUNT,
    .token_count = TOKEN_COUNT,
    .external_token_count = EXTERNAL_TOKEN_COUNT,
    .state_count = STATE_COUNT,
    .large_state_count = LARGE_STATE_COUNT,
    .production_id_count = PRODUCTION_ID_COUNT,
    .field_count = FIELD_COUNT,
    .max_alias_sequence_length = MAX_ALIAS_SEQUENCE_LENGTH,
    .parse_table = &ts_parse_table[0][0],
    .small_parse_table = ts_small_parse_table,
    .small_parse_table_map = ts_small_parse_table_map,
    .parse_actions = ts_parse_actions,
    .symbol_names = ts_symbol_names,
    .field_names = ts_field_names,
    .field_map_slices = ts_field_map_slices,
    .field_map_entries = ts_field_map_entries,
    .symbol_metadata = ts_symbol_metadata,
    .public_symbol_map = ts_symbol_map,
    .alias_map = ts_non_terminal_alias_map,
    .alias_sequences = &ts_alias_sequences[0][0],
    .lex_modes = ts_lex_modes,
    .lex_fn = ts_lex,
    .primary_state_ids = ts_primary_state_ids,
  };
  return &language;
}
#ifdef __cplusplus
}
#endif
