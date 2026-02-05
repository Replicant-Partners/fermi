#include "tree_sitter/parser.h"

#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic ignored "-Wmissing-field-initializers"
#endif

#ifdef _MSC_VER
#pragma optimize("", off)
#elif defined(__clang__)
#pragma clang optimize off
#elif defined(__GNUC__)
#pragma GCC optimize ("O0")
#endif

#define LANGUAGE_VERSION 14
#define STATE_COUNT 169
#define LARGE_STATE_COUNT 2
#define SYMBOL_COUNT 113
#define ALIAS_COUNT 0
#define TOKEN_COUNT 67
#define EXTERNAL_TOKEN_COUNT 0
#define FIELD_COUNT 32
#define MAX_ALIAS_SEQUENCE_LENGTH 10
#define PRODUCTION_ID_COUNT 25

enum ts_symbol_identifiers {
  anon_sym_question = 1,
  anon_sym_LBRACE = 2,
  anon_sym_RBRACE = 3,
  anon_sym_target_date = 4,
  anon_sym_COLON = 5,
  anon_sym_resolution_criteria = 6,
  anon_sym_base_rate = 7,
  anon_sym_reference_class = 8,
  anon_sym_historical_frequency = 9,
  anon_sym_sample_size = 10,
  anon_sym_source = 11,
  anon_sym_reasoning = 12,
  anon_sym_generated_by = 13,
  anon_sym_human = 14,
  anon_sym_driver = 15,
  anon_sym_continuous = 16,
  anon_sym_binary = 17,
  anon_sym_discrete = 18,
  anon_sym_distribution = 19,
  anon_sym_probability = 20,
  anon_sym_unit = 21,
  anon_sym_rationale = 22,
  anon_sym_impact_multiplier = 23,
  anon_sym_evidence = 24,
  anon_sym_summary = 25,
  anon_sym_relevance = 26,
  anon_sym_date = 27,
  anon_sym_agent = 28,
  anon_sym_query = 29,
  anon_sym_schedule = 30,
  anon_sym_every = 31,
  anon_sym_day = 32,
  anon_sym_days = 33,
  anon_sym_week = 34,
  anon_sym_weeks = 35,
  anon_sym_model = 36,
  anon_sym_simulate = 37,
  anon_sym_iterations = 38,
  anon_sym_triangular = 39,
  anon_sym_LPAREN = 40,
  anon_sym_COMMA = 41,
  anon_sym_RPAREN = 42,
  anon_sym_normal = 43,
  anon_sym_lognormal = 44,
  anon_sym_uniform = 45,
  anon_sym_beta = 46,
  anon_sym_STAR = 47,
  anon_sym_SLASH = 48,
  anon_sym_PLUS = 49,
  anon_sym_DASH = 50,
  anon_sym_CARET = 51,
  anon_sym_BANG = 52,
  anon_sym_if = 53,
  anon_sym_then = 54,
  anon_sym_else = 55,
  sym_identifier = 56,
  sym_number = 57,
  aux_sym_probability_token1 = 58,
  aux_sym_probability_token2 = 59,
  aux_sym_probability_token3 = 60,
  sym_date = 61,
  anon_sym_DQUOTE = 62,
  aux_sym_string_token1 = 63,
  anon_sym_BSLASH = 64,
  aux_sym_string_token2 = 65,
  sym_comment = 66,
  sym_source_file = 67,
  sym__statement = 68,
  sym_question_statement = 69,
  sym_question_block = 70,
  sym_question_property = 71,
  sym_base_rate_property = 72,
  sym_base_rate_block = 73,
  sym_base_rate_field = 74,
  sym_driver_statement = 75,
  sym_driver_block = 76,
  sym_driver_property = 77,
  sym_distribution_property = 78,
  sym_probability_property = 79,
  sym_unit_property = 80,
  sym_rationale_property = 81,
  sym_impact_multiplier_property = 82,
  sym_evidence_statement = 83,
  sym_evidence_block = 84,
  sym_evidence_property = 85,
  sym_agent_statement = 86,
  sym_agent_block = 87,
  sym_agent_property = 88,
  sym_model_statement = 89,
  sym_simulate_statement = 90,
  sym_distribution = 91,
  sym_triangular_distribution = 92,
  sym_normal_distribution = 93,
  sym_lognormal_distribution = 94,
  sym_uniform_distribution = 95,
  sym_beta_distribution = 96,
  sym_expression = 97,
  sym_binary_expression = 98,
  sym_unary_expression = 99,
  sym_conditional_expression = 100,
  sym_function_call = 101,
  sym_parenthesized_expression = 102,
  sym_probability = 103,
  sym_string = 104,
  aux_sym_source_file_repeat1 = 105,
  aux_sym_question_block_repeat1 = 106,
  aux_sym_base_rate_block_repeat1 = 107,
  aux_sym_driver_block_repeat1 = 108,
  aux_sym_evidence_block_repeat1 = 109,
  aux_sym_agent_block_repeat1 = 110,
  aux_sym_function_call_repeat1 = 111,
  aux_sym_string_repeat1 = 112,
};

static const char * const ts_symbol_names[] = {
  [ts_builtin_sym_end] = "end",
  [anon_sym_question] = "question",
  [anon_sym_LBRACE] = "{",
  [anon_sym_RBRACE] = "}",
  [anon_sym_target_date] = "target_date",
  [anon_sym_COLON] = ":",
  [anon_sym_resolution_criteria] = "resolution_criteria",
  [anon_sym_base_rate] = "base_rate",
  [anon_sym_reference_class] = "reference_class",
  [anon_sym_historical_frequency] = "historical_frequency",
  [anon_sym_sample_size] = "sample_size",
  [anon_sym_source] = "source",
  [anon_sym_reasoning] = "reasoning",
  [anon_sym_generated_by] = "generated_by",
  [anon_sym_human] = "human",
  [anon_sym_driver] = "driver",
  [anon_sym_continuous] = "continuous",
  [anon_sym_binary] = "binary",
  [anon_sym_discrete] = "discrete",
  [anon_sym_distribution] = "distribution",
  [anon_sym_probability] = "probability",
  [anon_sym_unit] = "unit",
  [anon_sym_rationale] = "rationale",
  [anon_sym_impact_multiplier] = "impact_multiplier",
  [anon_sym_evidence] = "evidence",
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
  [sym_question_block] = "question_block",
  [sym_question_property] = "question_property",
  [sym_base_rate_property] = "base_rate_property",
  [sym_base_rate_block] = "base_rate_block",
  [sym_base_rate_field] = "base_rate_field",
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
  [aux_sym_question_block_repeat1] = "question_block_repeat1",
  [aux_sym_base_rate_block_repeat1] = "base_rate_block_repeat1",
  [aux_sym_driver_block_repeat1] = "driver_block_repeat1",
  [aux_sym_evidence_block_repeat1] = "evidence_block_repeat1",
  [aux_sym_agent_block_repeat1] = "agent_block_repeat1",
  [aux_sym_function_call_repeat1] = "function_call_repeat1",
  [aux_sym_string_repeat1] = "string_repeat1",
};

static const TSSymbol ts_symbol_map[] = {
  [ts_builtin_sym_end] = ts_builtin_sym_end,
  [anon_sym_question] = anon_sym_question,
  [anon_sym_LBRACE] = anon_sym_LBRACE,
  [anon_sym_RBRACE] = anon_sym_RBRACE,
  [anon_sym_target_date] = anon_sym_target_date,
  [anon_sym_COLON] = anon_sym_COLON,
  [anon_sym_resolution_criteria] = anon_sym_resolution_criteria,
  [anon_sym_base_rate] = anon_sym_base_rate,
  [anon_sym_reference_class] = anon_sym_reference_class,
  [anon_sym_historical_frequency] = anon_sym_historical_frequency,
  [anon_sym_sample_size] = anon_sym_sample_size,
  [anon_sym_source] = anon_sym_source,
  [anon_sym_reasoning] = anon_sym_reasoning,
  [anon_sym_generated_by] = anon_sym_generated_by,
  [anon_sym_human] = anon_sym_human,
  [anon_sym_driver] = anon_sym_driver,
  [anon_sym_continuous] = anon_sym_continuous,
  [anon_sym_binary] = anon_sym_binary,
  [anon_sym_discrete] = anon_sym_discrete,
  [anon_sym_distribution] = anon_sym_distribution,
  [anon_sym_probability] = anon_sym_probability,
  [anon_sym_unit] = anon_sym_unit,
  [anon_sym_rationale] = anon_sym_rationale,
  [anon_sym_impact_multiplier] = anon_sym_impact_multiplier,
  [anon_sym_evidence] = anon_sym_evidence,
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
  [sym_question_block] = sym_question_block,
  [sym_question_property] = sym_question_property,
  [sym_base_rate_property] = sym_base_rate_property,
  [sym_base_rate_block] = sym_base_rate_block,
  [sym_base_rate_field] = sym_base_rate_field,
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
  [aux_sym_question_block_repeat1] = aux_sym_question_block_repeat1,
  [aux_sym_base_rate_block_repeat1] = aux_sym_base_rate_block_repeat1,
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
  [anon_sym_LBRACE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_RBRACE] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_target_date] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_COLON] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_resolution_criteria] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_base_rate] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_reference_class] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_historical_frequency] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_sample_size] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_source] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_reasoning] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_generated_by] = {
    .visible = true,
    .named = false,
  },
  [anon_sym_human] = {
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
  [anon_sym_distribution] = {
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
  [sym_question_block] = {
    .visible = true,
    .named = true,
  },
  [sym_question_property] = {
    .visible = true,
    .named = true,
  },
  [sym_base_rate_property] = {
    .visible = true,
    .named = true,
  },
  [sym_base_rate_block] = {
    .visible = true,
    .named = true,
  },
  [sym_base_rate_field] = {
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
  [aux_sym_question_block_repeat1] = {
    .visible = false,
    .named = false,
  },
  [aux_sym_base_rate_block_repeat1] = {
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
  field_block = 4,
  field_body = 5,
  field_condition = 6,
  field_distribution = 7,
  field_else_expr = 8,
  field_expression = 9,
  field_function = 10,
  field_high = 11,
  field_interval = 12,
  field_iterations = 13,
  field_left = 14,
  field_low = 15,
  field_max = 16,
  field_mean = 17,
  field_median = 18,
  field_min = 19,
  field_name = 20,
  field_operand = 21,
  field_p5 = 22,
  field_p50 = 23,
  field_p95 = 24,
  field_right = 25,
  field_sigma = 26,
  field_stddev = 27,
  field_text = 28,
  field_then_expr = 29,
  field_type = 30,
  field_unit = 31,
  field_value = 32,
};

static const char * const ts_field_names[] = {
  [0] = NULL,
  [field_alpha] = "alpha",
  [field_argument] = "argument",
  [field_beta_param] = "beta_param",
  [field_block] = "block",
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
  [3] = {.index = 3, .length = 2},
  [4] = {.index = 5, .length = 1},
  [5] = {.index = 6, .length = 1},
  [6] = {.index = 7, .length = 3},
  [7] = {.index = 10, .length = 1},
  [8] = {.index = 11, .length = 1},
  [9] = {.index = 12, .length = 1},
  [10] = {.index = 13, .length = 2},
  [11] = {.index = 15, .length = 1},
  [12] = {.index = 16, .length = 2},
  [13] = {.index = 18, .length = 1},
  [14] = {.index = 19, .length = 1},
  [15] = {.index = 20, .length = 3},
  [16] = {.index = 23, .length = 2},
  [17] = {.index = 25, .length = 2},
  [18] = {.index = 27, .length = 3},
  [19] = {.index = 30, .length = 2},
  [20] = {.index = 32, .length = 2},
  [21] = {.index = 34, .length = 2},
  [22] = {.index = 36, .length = 2},
  [23] = {.index = 38, .length = 3},
  [24] = {.index = 41, .length = 4},
};

static const TSFieldMapEntry ts_field_map_entries[] = {
  [0] =
    {field_text, 1},
  [1] =
    {field_block, 2},
    {field_text, 1},
  [3] =
    {field_body, 2},
    {field_name, 1},
  [5] =
    {field_expression, 2},
  [6] =
    {field_iterations, 1},
  [7] =
    {field_body, 3},
    {field_name, 1},
    {field_type, 2},
  [10] =
    {field_operand, 1},
  [11] =
    {field_block, 1},
  [12] =
    {field_function, 0},
  [13] =
    {field_left, 0},
    {field_right, 2},
  [15] =
    {field_value, 2},
  [16] =
    {field_argument, 2},
    {field_function, 0},
  [18] =
    {field_distribution, 2},
  [19] =
    {field_argument, 1},
  [20] =
    {field_argument, 2},
    {field_argument, 3, .inherited = true},
    {field_function, 0},
  [23] =
    {field_argument, 0, .inherited = true},
    {field_argument, 1, .inherited = true},
  [25] =
    {field_interval, 3},
    {field_unit, 4},
  [27] =
    {field_condition, 1},
    {field_else_expr, 5},
    {field_then_expr, 3},
  [30] =
    {field_mean, 2},
    {field_stddev, 4},
  [32] =
    {field_median, 2},
    {field_sigma, 4},
  [34] =
    {field_high, 4},
    {field_low, 2},
  [36] =
    {field_alpha, 2},
    {field_beta_param, 4},
  [38] =
    {field_p5, 2},
    {field_p50, 4},
    {field_p95, 6},
  [41] =
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
  [143] = 143,
  [144] = 144,
  [145] = 145,
  [146] = 146,
  [147] = 147,
  [148] = 148,
  [149] = 149,
  [150] = 150,
  [151] = 151,
  [152] = 152,
  [153] = 153,
  [154] = 154,
  [155] = 155,
  [156] = 156,
  [157] = 157,
  [158] = 158,
  [159] = 159,
  [160] = 160,
  [161] = 161,
  [162] = 162,
  [163] = 163,
  [164] = 164,
  [165] = 165,
  [166] = 166,
  [167] = 167,
  [168] = 168,
};

static bool ts_lex(TSLexer *lexer, TSStateId state) {
  START_LEXER();
  eof = lexer->eof(lexer);
  switch (state) {
    case 0:
      if (eof) ADVANCE(280);
      ADVANCE_MAP(
        '!', 333,
        '"', 353,
        '#', 364,
        '(', 321,
        ')', 323,
        '*', 328,
        '+', 330,
        ',', 322,
        '-', 331,
        '/', 329,
        ':', 285,
        '\\', 358,
        '^', 332,
        'a', 108,
        'b', 19,
        'c', 182,
        'd', 20,
        'e', 143,
        'g', 85,
        'h', 114,
        'i', 102,
        'l', 180,
        'm', 177,
        'n', 178,
        'p', 205,
        'q', 250,
        'r', 24,
        's', 25,
        't', 26,
        'u', 162,
        'w', 87,
        '{', 282,
        '}', 283,
      );
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(0);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(345);
      END_STATE();
    case 1:
      if (lookahead == '\n') SKIP(1);
      if (lookahead == '#') ADVANCE(362);
      if (lookahead == '/') ADVANCE(361);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') ADVANCE(360);
      if (lookahead != 0) ADVANCE(359);
      END_STATE();
    case 2:
      ADVANCE_MAP(
        '!', 333,
        '#', 364,
        '(', 321,
        ')', 323,
        '-', 331,
        '/', 6,
        'i', 339,
        'p', 343,
      );
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(2);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(345);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(344);
      END_STATE();
    case 3:
      if (lookahead == '"') ADVANCE(353);
      if (lookahead == '#') ADVANCE(357);
      if (lookahead == '/') ADVANCE(356);
      if (lookahead == '\\') ADVANCE(358);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') ADVANCE(355);
      if (lookahead != 0) ADVANCE(354);
      END_STATE();
    case 4:
      if (lookahead == '#') ADVANCE(364);
      if (lookahead == '/') ADVANCE(6);
      if (lookahead == 'h') ADVANCE(342);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(4);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(346);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(344);
      END_STATE();
    case 5:
      if (lookahead == '#') ADVANCE(364);
      if (lookahead == '/') ADVANCE(6);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') SKIP(5);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(277);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(344);
      END_STATE();
    case 6:
      if (lookahead == '*') ADVANCE(8);
      if (lookahead == '/') ADVANCE(364);
      END_STATE();
    case 7:
      if (lookahead == '*') ADVANCE(7);
      if (lookahead == '/') ADVANCE(363);
      if (lookahead != 0) ADVANCE(8);
      END_STATE();
    case 8:
      if (lookahead == '*') ADVANCE(7);
      if (lookahead != 0) ADVANCE(8);
      END_STATE();
    case 9:
      if (lookahead == '-') ADVANCE(276);
      END_STATE();
    case 10:
      if (lookahead == '-') ADVANCE(279);
      END_STATE();
    case 11:
      if (lookahead == '_') ADVANCE(104);
      END_STATE();
    case 12:
      if (lookahead == '_') ADVANCE(63);
      END_STATE();
    case 13:
      if (lookahead == '_') ADVANCE(45);
      END_STATE();
    case 14:
      if (lookahead == '_') ADVANCE(58);
      END_STATE();
    case 15:
      if (lookahead == '_') ADVANCE(228);
      END_STATE();
    case 16:
      if (lookahead == '_') ADVANCE(54);
      END_STATE();
    case 17:
      if (lookahead == '_') ADVANCE(155);
      END_STATE();
    case 18:
      if (lookahead == '_') ADVANCE(219);
      END_STATE();
    case 19:
      if (lookahead == 'a') ADVANCE(225);
      if (lookahead == 'e') ADVANCE(230);
      if (lookahead == 'i') ADVANCE(160);
      END_STATE();
    case 20:
      if (lookahead == 'a') ADVANCE(238);
      if (lookahead == 'i') ADVANCE(220);
      if (lookahead == 'r') ADVANCE(111);
      END_STATE();
    case 21:
      if (lookahead == 'a') ADVANCE(327);
      END_STATE();
    case 22:
      if (lookahead == 'a') ADVANCE(49);
      END_STATE();
    case 23:
      if (lookahead == 'a') ADVANCE(286);
      END_STATE();
    case 24:
      if (lookahead == 'a') ADVANCE(233);
      if (lookahead == 'e') ADVANCE(28);
      END_STATE();
    case 25:
      if (lookahead == 'a') ADVANCE(148);
      if (lookahead == 'c') ADVANCE(110);
      if (lookahead == 'i') ADVANCE(149);
      if (lookahead == 'o') ADVANCE(252);
      if (lookahead == 'u') ADVANCE(154);
      END_STATE();
    case 26:
      if (lookahead == 'a') ADVANCE(214);
      if (lookahead == 'h') ADVANCE(92);
      if (lookahead == 'r') ADVANCE(125);
      END_STATE();
    case 27:
      if (lookahead == 'a') ADVANCE(47);
      END_STATE();
    case 28:
      if (lookahead == 'a') ADVANCE(229);
      if (lookahead == 'f') ADVANCE(94);
      if (lookahead == 'l') ADVANCE(81);
      if (lookahead == 's') ADVANCE(181);
      END_STATE();
    case 29:
      if (lookahead == 'a') ADVANCE(133);
      END_STATE();
    case 30:
      if (lookahead == 'a') ADVANCE(203);
      END_STATE();
    case 31:
      if (lookahead == 'a') ADVANCE(157);
      END_STATE();
    case 32:
      if (lookahead == 'a') ADVANCE(227);
      END_STATE();
    case 33:
      if (lookahead == 'a') ADVANCE(134);
      END_STATE();
    case 34:
      if (lookahead == 'a') ADVANCE(161);
      END_STATE();
    case 35:
      if (lookahead == 'a') ADVANCE(136);
      END_STATE();
    case 36:
      if (lookahead == 'a') ADVANCE(204);
      END_STATE();
    case 37:
      if (lookahead == 'a') ADVANCE(199);
      END_STATE();
    case 38:
      if (lookahead == 'a') ADVANCE(241);
      END_STATE();
    case 39:
      if (lookahead == 'a') ADVANCE(142);
      END_STATE();
    case 40:
      if (lookahead == 'a') ADVANCE(242);
      END_STATE();
    case 41:
      if (lookahead == 'a') ADVANCE(243);
      END_STATE();
    case 42:
      if (lookahead == 'a') ADVANCE(244);
      END_STATE();
    case 43:
      if (lookahead == 'a') ADVANCE(174);
      END_STATE();
    case 44:
      if (lookahead == 'a') ADVANCE(247);
      END_STATE();
    case 45:
      if (lookahead == 'b') ADVANCE(268);
      END_STATE();
    case 46:
      if (lookahead == 'b') ADVANCE(27);
      END_STATE();
    case 47:
      if (lookahead == 'b') ADVANCE(117);
      END_STATE();
    case 48:
      if (lookahead == 'b') ADVANCE(260);
      END_STATE();
    case 49:
      if (lookahead == 'c') ADVANCE(232);
      END_STATE();
    case 50:
      if (lookahead == 'c') ADVANCE(212);
      if (lookahead == 't') ADVANCE(208);
      END_STATE();
    case 51:
      if (lookahead == 'c') ADVANCE(269);
      END_STATE();
    case 52:
      if (lookahead == 'c') ADVANCE(35);
      END_STATE();
    case 53:
      if (lookahead == 'c') ADVANCE(70);
      END_STATE();
    case 54:
      if (lookahead == 'c') ADVANCE(211);
      END_STATE();
    case 55:
      if (lookahead == 'c') ADVANCE(72);
      END_STATE();
    case 56:
      if (lookahead == 'c') ADVANCE(91);
      END_STATE();
    case 57:
      if (lookahead == 'c') ADVANCE(77);
      END_STATE();
    case 58:
      if (lookahead == 'c') ADVANCE(138);
      END_STATE();
    case 59:
      if (lookahead == 'd') ADVANCE(13);
      END_STATE();
    case 60:
      if (lookahead == 'd') ADVANCE(69);
      END_STATE();
    case 61:
      if (lookahead == 'd') ADVANCE(257);
      END_STATE();
    case 62:
      if (lookahead == 'd') ADVANCE(96);
      END_STATE();
    case 63:
      if (lookahead == 'd') ADVANCE(42);
      END_STATE();
    case 64:
      if (lookahead == 'e') ADVANCE(201);
      END_STATE();
    case 65:
      if (lookahead == 'e') ADVANCE(131);
      END_STATE();
    case 66:
      if (lookahead == 'e') ADVANCE(18);
      END_STATE();
    case 67:
      if (lookahead == 'e') ADVANCE(308);
      END_STATE();
    case 68:
      if (lookahead == 'e') ADVANCE(337);
      END_STATE();
    case 69:
      if (lookahead == 'e') ADVANCE(132);
      END_STATE();
    case 70:
      if (lookahead == 'e') ADVANCE(291);
      END_STATE();
    case 71:
      if (lookahead == 'e') ADVANCE(299);
      END_STATE();
    case 72:
      if (lookahead == 'e') ADVANCE(305);
      END_STATE();
    case 73:
      if (lookahead == 'e') ADVANCE(311);
      END_STATE();
    case 74:
      if (lookahead == 'e') ADVANCE(318);
      END_STATE();
    case 75:
      if (lookahead == 'e') ADVANCE(287);
      END_STATE();
    case 76:
      if (lookahead == 'e') ADVANCE(303);
      END_STATE();
    case 77:
      if (lookahead == 'e') ADVANCE(307);
      END_STATE();
    case 78:
      if (lookahead == 'e') ADVANCE(290);
      END_STATE();
    case 79:
      if (lookahead == 'e') ADVANCE(284);
      END_STATE();
    case 80:
      if (lookahead == 'e') ADVANCE(196);
      END_STATE();
    case 81:
      if (lookahead == 'e') ADVANCE(262);
      END_STATE();
    case 82:
      if (lookahead == 'e') ADVANCE(61);
      END_STATE();
    case 83:
      if (lookahead == 'e') ADVANCE(197);
      if (lookahead == 'i') ADVANCE(62);
      END_STATE();
    case 84:
      if (lookahead == 'e') ADVANCE(15);
      END_STATE();
    case 85:
      if (lookahead == 'e') ADVANCE(175);
      END_STATE();
    case 86:
      if (lookahead == 'e') ADVANCE(207);
      END_STATE();
    case 87:
      if (lookahead == 'e') ADVANCE(65);
      END_STATE();
    case 88:
      if (lookahead == 'e') ADVANCE(164);
      END_STATE();
    case 89:
      if (lookahead == 'e') ADVANCE(59);
      END_STATE();
    case 90:
      if (lookahead == 'e') ADVANCE(234);
      END_STATE();
    case 91:
      if (lookahead == 'e') ADVANCE(14);
      END_STATE();
    case 92:
      if (lookahead == 'e') ADVANCE(156);
      END_STATE();
    case 93:
      if (lookahead == 'e') ADVANCE(240);
      END_STATE();
    case 94:
      if (lookahead == 'e') ADVANCE(218);
      END_STATE();
    case 95:
      if (lookahead == 'e') ADVANCE(198);
      END_STATE();
    case 96:
      if (lookahead == 'e') ADVANCE(171);
      END_STATE();
    case 97:
      if (lookahead == 'e') ADVANCE(200);
      END_STATE();
    case 98:
      if (lookahead == 'e') ADVANCE(166);
      END_STATE();
    case 99:
      if (lookahead == 'e') ADVANCE(216);
      END_STATE();
    case 100:
      if (lookahead == 'e') ADVANCE(210);
      END_STATE();
    case 101:
      if (lookahead == 'e') ADVANCE(173);
      END_STATE();
    case 102:
      if (lookahead == 'f') ADVANCE(334);
      if (lookahead == 'm') ADVANCE(194);
      if (lookahead == 't') ADVANCE(86);
      END_STATE();
    case 103:
      if (lookahead == 'f') ADVANCE(186);
      if (lookahead == 't') ADVANCE(302);
      END_STATE();
    case 104:
      if (lookahead == 'f') ADVANCE(213);
      END_STATE();
    case 105:
      if (lookahead == 'g') ADVANCE(292);
      END_STATE();
    case 106:
      if (lookahead == 'g') ADVANCE(176);
      END_STATE();
    case 107:
      if (lookahead == 'g') ADVANCE(90);
      END_STATE();
    case 108:
      if (lookahead == 'g') ADVANCE(88);
      END_STATE();
    case 109:
      if (lookahead == 'g') ADVANCE(254);
      END_STATE();
    case 110:
      if (lookahead == 'h') ADVANCE(82);
      END_STATE();
    case 111:
      if (lookahead == 'i') ADVANCE(261);
      END_STATE();
    case 112:
      if (lookahead == 'i') ADVANCE(103);
      END_STATE();
    case 113:
      if (lookahead == 'i') ADVANCE(270);
      END_STATE();
    case 114:
      if (lookahead == 'i') ADVANCE(224);
      if (lookahead == 'u') ADVANCE(150);
      END_STATE();
    case 115:
      if (lookahead == 'i') ADVANCE(48);
      END_STATE();
    case 116:
      if (lookahead == 'i') ADVANCE(52);
      END_STATE();
    case 117:
      if (lookahead == 'i') ADVANCE(137);
      END_STATE();
    case 118:
      if (lookahead == 'i') ADVANCE(236);
      END_STATE();
    case 119:
      if (lookahead == 'i') ADVANCE(245);
      END_STATE();
    case 120:
      if (lookahead == 'i') ADVANCE(165);
      END_STATE();
    case 121:
      if (lookahead == 'i') ADVANCE(163);
      END_STATE();
    case 122:
      if (lookahead == 'i') ADVANCE(23);
      END_STATE();
    case 123:
      if (lookahead == 'i') ADVANCE(191);
      END_STATE();
    case 124:
      if (lookahead == 'i') ADVANCE(195);
      END_STATE();
    case 125:
      if (lookahead == 'i') ADVANCE(34);
      END_STATE();
    case 126:
      if (lookahead == 'i') ADVANCE(185);
      END_STATE();
    case 127:
      if (lookahead == 'i') ADVANCE(97);
      END_STATE();
    case 128:
      if (lookahead == 'i') ADVANCE(187);
      END_STATE();
    case 129:
      if (lookahead == 'i') ADVANCE(188);
      END_STATE();
    case 130:
      if (lookahead == 'i') ADVANCE(189);
      END_STATE();
    case 131:
      if (lookahead == 'k') ADVANCE(315);
      END_STATE();
    case 132:
      if (lookahead == 'l') ADVANCE(317);
      END_STATE();
    case 133:
      if (lookahead == 'l') ADVANCE(324);
      END_STATE();
    case 134:
      if (lookahead == 'l') ADVANCE(325);
      END_STATE();
    case 135:
      if (lookahead == 'l') ADVANCE(259);
      END_STATE();
    case 136:
      if (lookahead == 'l') ADVANCE(11);
      END_STATE();
    case 137:
      if (lookahead == 'l') ADVANCE(118);
      END_STATE();
    case 138:
      if (lookahead == 'l') ADVANCE(32);
      END_STATE();
    case 139:
      if (lookahead == 'l') ADVANCE(127);
      END_STATE();
    case 140:
      if (lookahead == 'l') ADVANCE(84);
      END_STATE();
    case 141:
      if (lookahead == 'l') ADVANCE(73);
      END_STATE();
    case 142:
      if (lookahead == 'l') ADVANCE(76);
      END_STATE();
    case 143:
      if (lookahead == 'l') ADVANCE(226);
      if (lookahead == 'v') ADVANCE(83);
      END_STATE();
    case 144:
      if (lookahead == 'l') ADVANCE(239);
      END_STATE();
    case 145:
      if (lookahead == 'l') ADVANCE(37);
      END_STATE();
    case 146:
      if (lookahead == 'l') ADVANCE(40);
      END_STATE();
    case 147:
      if (lookahead == 'm') ADVANCE(326);
      END_STATE();
    case 148:
      if (lookahead == 'm') ADVANCE(193);
      END_STATE();
    case 149:
      if (lookahead == 'm') ADVANCE(251);
      END_STATE();
    case 150:
      if (lookahead == 'm') ADVANCE(31);
      END_STATE();
    case 151:
      if (lookahead == 'm') ADVANCE(29);
      END_STATE();
    case 152:
      if (lookahead == 'm') ADVANCE(33);
      END_STATE();
    case 153:
      if (lookahead == 'm') ADVANCE(36);
      END_STATE();
    case 154:
      if (lookahead == 'm') ADVANCE(153);
      END_STATE();
    case 155:
      if (lookahead == 'm') ADVANCE(256);
      END_STATE();
    case 156:
      if (lookahead == 'n') ADVANCE(336);
      END_STATE();
    case 157:
      if (lookahead == 'n') ADVANCE(294);
      END_STATE();
    case 158:
      if (lookahead == 'n') ADVANCE(281);
      END_STATE();
    case 159:
      if (lookahead == 'n') ADVANCE(300);
      END_STATE();
    case 160:
      if (lookahead == 'n') ADVANCE(30);
      END_STATE();
    case 161:
      if (lookahead == 'n') ADVANCE(109);
      END_STATE();
    case 162:
      if (lookahead == 'n') ADVANCE(112);
      END_STATE();
    case 163:
      if (lookahead == 'n') ADVANCE(105);
      END_STATE();
    case 164:
      if (lookahead == 'n') ADVANCE(231);
      END_STATE();
    case 165:
      if (lookahead == 'n') ADVANCE(255);
      END_STATE();
    case 166:
      if (lookahead == 'n') ADVANCE(51);
      END_STATE();
    case 167:
      if (lookahead == 'n') ADVANCE(222);
      END_STATE();
    case 168:
      if (lookahead == 'n') ADVANCE(16);
      END_STATE();
    case 169:
      if (lookahead == 'n') ADVANCE(39);
      END_STATE();
    case 170:
      if (lookahead == 'n') ADVANCE(235);
      END_STATE();
    case 171:
      if (lookahead == 'n') ADVANCE(55);
      END_STATE();
    case 172:
      if (lookahead == 'n') ADVANCE(121);
      END_STATE();
    case 173:
      if (lookahead == 'n') ADVANCE(56);
      END_STATE();
    case 174:
      if (lookahead == 'n') ADVANCE(57);
      END_STATE();
    case 175:
      if (lookahead == 'n') ADVANCE(100);
      END_STATE();
    case 176:
      if (lookahead == 'n') ADVANCE(192);
      END_STATE();
    case 177:
      if (lookahead == 'o') ADVANCE(60);
      END_STATE();
    case 178:
      if (lookahead == 'o') ADVANCE(215);
      END_STATE();
    case 179:
      if (lookahead == 'o') ADVANCE(46);
      END_STATE();
    case 180:
      if (lookahead == 'o') ADVANCE(106);
      END_STATE();
    case 181:
      if (lookahead == 'o') ADVANCE(135);
      END_STATE();
    case 182:
      if (lookahead == 'o') ADVANCE(170);
      END_STATE();
    case 183:
      if (lookahead == 'o') ADVANCE(253);
      END_STATE();
    case 184:
      if (lookahead == 'o') ADVANCE(209);
      END_STATE();
    case 185:
      if (lookahead == 'o') ADVANCE(158);
      END_STATE();
    case 186:
      if (lookahead == 'o') ADVANCE(206);
      END_STATE();
    case 187:
      if (lookahead == 'o') ADVANCE(167);
      END_STATE();
    case 188:
      if (lookahead == 'o') ADVANCE(168);
      END_STATE();
    case 189:
      if (lookahead == 'o') ADVANCE(159);
      END_STATE();
    case 190:
      if (lookahead == 'o') ADVANCE(172);
      END_STATE();
    case 191:
      if (lookahead == 'o') ADVANCE(169);
      END_STATE();
    case 192:
      if (lookahead == 'o') ADVANCE(217);
      END_STATE();
    case 193:
      if (lookahead == 'p') ADVANCE(140);
      END_STATE();
    case 194:
      if (lookahead == 'p') ADVANCE(22);
      END_STATE();
    case 195:
      if (lookahead == 'p') ADVANCE(139);
      END_STATE();
    case 196:
      if (lookahead == 'q') ADVANCE(258);
      END_STATE();
    case 197:
      if (lookahead == 'r') ADVANCE(263);
      END_STATE();
    case 198:
      if (lookahead == 'r') ADVANCE(296);
      END_STATE();
    case 199:
      if (lookahead == 'r') ADVANCE(320);
      END_STATE();
    case 200:
      if (lookahead == 'r') ADVANCE(304);
      END_STATE();
    case 201:
      if (lookahead == 'r') ADVANCE(264);
      if (lookahead == 's') ADVANCE(246);
      END_STATE();
    case 202:
      if (lookahead == 'r') ADVANCE(53);
      END_STATE();
    case 203:
      if (lookahead == 'r') ADVANCE(265);
      END_STATE();
    case 204:
      if (lookahead == 'r') ADVANCE(266);
      END_STATE();
    case 205:
      if (lookahead == 'r') ADVANCE(179);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(350);
      END_STATE();
    case 206:
      if (lookahead == 'r') ADVANCE(147);
      END_STATE();
    case 207:
      if (lookahead == 'r') ADVANCE(44);
      END_STATE();
    case 208:
      if (lookahead == 'r') ADVANCE(115);
      END_STATE();
    case 209:
      if (lookahead == 'r') ADVANCE(116);
      END_STATE();
    case 210:
      if (lookahead == 'r') ADVANCE(38);
      END_STATE();
    case 211:
      if (lookahead == 'r') ADVANCE(119);
      END_STATE();
    case 212:
      if (lookahead == 'r') ADVANCE(93);
      END_STATE();
    case 213:
      if (lookahead == 'r') ADVANCE(80);
      END_STATE();
    case 214:
      if (lookahead == 'r') ADVANCE(107);
      END_STATE();
    case 215:
      if (lookahead == 'r') ADVANCE(151);
      END_STATE();
    case 216:
      if (lookahead == 'r') ADVANCE(122);
      END_STATE();
    case 217:
      if (lookahead == 'r') ADVANCE(152);
      END_STATE();
    case 218:
      if (lookahead == 'r') ADVANCE(101);
      END_STATE();
    case 219:
      if (lookahead == 'r') ADVANCE(41);
      END_STATE();
    case 220:
      if (lookahead == 's') ADVANCE(50);
      END_STATE();
    case 221:
      if (lookahead == 's') ADVANCE(297);
      END_STATE();
    case 222:
      if (lookahead == 's') ADVANCE(319);
      END_STATE();
    case 223:
      if (lookahead == 's') ADVANCE(288);
      END_STATE();
    case 224:
      if (lookahead == 's') ADVANCE(237);
      END_STATE();
    case 225:
      if (lookahead == 's') ADVANCE(66);
      END_STATE();
    case 226:
      if (lookahead == 's') ADVANCE(68);
      END_STATE();
    case 227:
      if (lookahead == 's') ADVANCE(223);
      END_STATE();
    case 228:
      if (lookahead == 's') ADVANCE(113);
      END_STATE();
    case 229:
      if (lookahead == 's') ADVANCE(190);
      END_STATE();
    case 230:
      if (lookahead == 't') ADVANCE(21);
      END_STATE();
    case 231:
      if (lookahead == 't') ADVANCE(309);
      END_STATE();
    case 232:
      if (lookahead == 't') ADVANCE(17);
      END_STATE();
    case 233:
      if (lookahead == 't') ADVANCE(123);
      END_STATE();
    case 234:
      if (lookahead == 't') ADVANCE(12);
      END_STATE();
    case 235:
      if (lookahead == 't') ADVANCE(120);
      END_STATE();
    case 236:
      if (lookahead == 't') ADVANCE(267);
      END_STATE();
    case 237:
      if (lookahead == 't') ADVANCE(184);
      END_STATE();
    case 238:
      if (lookahead == 't') ADVANCE(67);
      if (lookahead == 'y') ADVANCE(313);
      END_STATE();
    case 239:
      if (lookahead == 't') ADVANCE(124);
      END_STATE();
    case 240:
      if (lookahead == 't') ADVANCE(71);
      END_STATE();
    case 241:
      if (lookahead == 't') ADVANCE(89);
      END_STATE();
    case 242:
      if (lookahead == 't') ADVANCE(74);
      END_STATE();
    case 243:
      if (lookahead == 't') ADVANCE(75);
      END_STATE();
    case 244:
      if (lookahead == 't') ADVANCE(79);
      END_STATE();
    case 245:
      if (lookahead == 't') ADVANCE(99);
      END_STATE();
    case 246:
      if (lookahead == 't') ADVANCE(126);
      END_STATE();
    case 247:
      if (lookahead == 't') ADVANCE(128);
      END_STATE();
    case 248:
      if (lookahead == 't') ADVANCE(129);
      END_STATE();
    case 249:
      if (lookahead == 't') ADVANCE(130);
      END_STATE();
    case 250:
      if (lookahead == 'u') ADVANCE(64);
      END_STATE();
    case 251:
      if (lookahead == 'u') ADVANCE(146);
      END_STATE();
    case 252:
      if (lookahead == 'u') ADVANCE(202);
      END_STATE();
    case 253:
      if (lookahead == 'u') ADVANCE(221);
      END_STATE();
    case 254:
      if (lookahead == 'u') ADVANCE(145);
      END_STATE();
    case 255:
      if (lookahead == 'u') ADVANCE(183);
      END_STATE();
    case 256:
      if (lookahead == 'u') ADVANCE(144);
      END_STATE();
    case 257:
      if (lookahead == 'u') ADVANCE(141);
      END_STATE();
    case 258:
      if (lookahead == 'u') ADVANCE(98);
      END_STATE();
    case 259:
      if (lookahead == 'u') ADVANCE(248);
      END_STATE();
    case 260:
      if (lookahead == 'u') ADVANCE(249);
      END_STATE();
    case 261:
      if (lookahead == 'v') ADVANCE(95);
      END_STATE();
    case 262:
      if (lookahead == 'v') ADVANCE(43);
      END_STATE();
    case 263:
      if (lookahead == 'y') ADVANCE(312);
      END_STATE();
    case 264:
      if (lookahead == 'y') ADVANCE(310);
      END_STATE();
    case 265:
      if (lookahead == 'y') ADVANCE(298);
      END_STATE();
    case 266:
      if (lookahead == 'y') ADVANCE(306);
      END_STATE();
    case 267:
      if (lookahead == 'y') ADVANCE(301);
      END_STATE();
    case 268:
      if (lookahead == 'y') ADVANCE(293);
      END_STATE();
    case 269:
      if (lookahead == 'y') ADVANCE(289);
      END_STATE();
    case 270:
      if (lookahead == 'z') ADVANCE(78);
      END_STATE();
    case 271:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(347);
      END_STATE();
    case 272:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(348);
      END_STATE();
    case 273:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(10);
      END_STATE();
    case 274:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(352);
      END_STATE();
    case 275:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(273);
      END_STATE();
    case 276:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(274);
      END_STATE();
    case 277:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(275);
      END_STATE();
    case 278:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(9);
      END_STATE();
    case 279:
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(278);
      END_STATE();
    case 280:
      ACCEPT_TOKEN(ts_builtin_sym_end);
      END_STATE();
    case 281:
      ACCEPT_TOKEN(anon_sym_question);
      END_STATE();
    case 282:
      ACCEPT_TOKEN(anon_sym_LBRACE);
      END_STATE();
    case 283:
      ACCEPT_TOKEN(anon_sym_RBRACE);
      END_STATE();
    case 284:
      ACCEPT_TOKEN(anon_sym_target_date);
      END_STATE();
    case 285:
      ACCEPT_TOKEN(anon_sym_COLON);
      END_STATE();
    case 286:
      ACCEPT_TOKEN(anon_sym_resolution_criteria);
      END_STATE();
    case 287:
      ACCEPT_TOKEN(anon_sym_base_rate);
      END_STATE();
    case 288:
      ACCEPT_TOKEN(anon_sym_reference_class);
      END_STATE();
    case 289:
      ACCEPT_TOKEN(anon_sym_historical_frequency);
      END_STATE();
    case 290:
      ACCEPT_TOKEN(anon_sym_sample_size);
      END_STATE();
    case 291:
      ACCEPT_TOKEN(anon_sym_source);
      END_STATE();
    case 292:
      ACCEPT_TOKEN(anon_sym_reasoning);
      END_STATE();
    case 293:
      ACCEPT_TOKEN(anon_sym_generated_by);
      END_STATE();
    case 294:
      ACCEPT_TOKEN(anon_sym_human);
      END_STATE();
    case 295:
      ACCEPT_TOKEN(anon_sym_human);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(344);
      END_STATE();
    case 296:
      ACCEPT_TOKEN(anon_sym_driver);
      END_STATE();
    case 297:
      ACCEPT_TOKEN(anon_sym_continuous);
      END_STATE();
    case 298:
      ACCEPT_TOKEN(anon_sym_binary);
      END_STATE();
    case 299:
      ACCEPT_TOKEN(anon_sym_discrete);
      END_STATE();
    case 300:
      ACCEPT_TOKEN(anon_sym_distribution);
      END_STATE();
    case 301:
      ACCEPT_TOKEN(anon_sym_probability);
      END_STATE();
    case 302:
      ACCEPT_TOKEN(anon_sym_unit);
      END_STATE();
    case 303:
      ACCEPT_TOKEN(anon_sym_rationale);
      END_STATE();
    case 304:
      ACCEPT_TOKEN(anon_sym_impact_multiplier);
      END_STATE();
    case 305:
      ACCEPT_TOKEN(anon_sym_evidence);
      END_STATE();
    case 306:
      ACCEPT_TOKEN(anon_sym_summary);
      END_STATE();
    case 307:
      ACCEPT_TOKEN(anon_sym_relevance);
      END_STATE();
    case 308:
      ACCEPT_TOKEN(anon_sym_date);
      END_STATE();
    case 309:
      ACCEPT_TOKEN(anon_sym_agent);
      END_STATE();
    case 310:
      ACCEPT_TOKEN(anon_sym_query);
      END_STATE();
    case 311:
      ACCEPT_TOKEN(anon_sym_schedule);
      END_STATE();
    case 312:
      ACCEPT_TOKEN(anon_sym_every);
      END_STATE();
    case 313:
      ACCEPT_TOKEN(anon_sym_day);
      if (lookahead == 's') ADVANCE(314);
      END_STATE();
    case 314:
      ACCEPT_TOKEN(anon_sym_days);
      END_STATE();
    case 315:
      ACCEPT_TOKEN(anon_sym_week);
      if (lookahead == 's') ADVANCE(316);
      END_STATE();
    case 316:
      ACCEPT_TOKEN(anon_sym_weeks);
      END_STATE();
    case 317:
      ACCEPT_TOKEN(anon_sym_model);
      END_STATE();
    case 318:
      ACCEPT_TOKEN(anon_sym_simulate);
      END_STATE();
    case 319:
      ACCEPT_TOKEN(anon_sym_iterations);
      END_STATE();
    case 320:
      ACCEPT_TOKEN(anon_sym_triangular);
      END_STATE();
    case 321:
      ACCEPT_TOKEN(anon_sym_LPAREN);
      END_STATE();
    case 322:
      ACCEPT_TOKEN(anon_sym_COMMA);
      END_STATE();
    case 323:
      ACCEPT_TOKEN(anon_sym_RPAREN);
      END_STATE();
    case 324:
      ACCEPT_TOKEN(anon_sym_normal);
      END_STATE();
    case 325:
      ACCEPT_TOKEN(anon_sym_lognormal);
      END_STATE();
    case 326:
      ACCEPT_TOKEN(anon_sym_uniform);
      END_STATE();
    case 327:
      ACCEPT_TOKEN(anon_sym_beta);
      END_STATE();
    case 328:
      ACCEPT_TOKEN(anon_sym_STAR);
      END_STATE();
    case 329:
      ACCEPT_TOKEN(anon_sym_SLASH);
      if (lookahead == '*') ADVANCE(8);
      if (lookahead == '/') ADVANCE(364);
      END_STATE();
    case 330:
      ACCEPT_TOKEN(anon_sym_PLUS);
      END_STATE();
    case 331:
      ACCEPT_TOKEN(anon_sym_DASH);
      END_STATE();
    case 332:
      ACCEPT_TOKEN(anon_sym_CARET);
      END_STATE();
    case 333:
      ACCEPT_TOKEN(anon_sym_BANG);
      END_STATE();
    case 334:
      ACCEPT_TOKEN(anon_sym_if);
      END_STATE();
    case 335:
      ACCEPT_TOKEN(anon_sym_if);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(344);
      END_STATE();
    case 336:
      ACCEPT_TOKEN(anon_sym_then);
      END_STATE();
    case 337:
      ACCEPT_TOKEN(anon_sym_else);
      END_STATE();
    case 338:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'a') ADVANCE(341);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('b' <= lookahead && lookahead <= 'z')) ADVANCE(344);
      END_STATE();
    case 339:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'f') ADVANCE(335);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(344);
      END_STATE();
    case 340:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'm') ADVANCE(338);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(344);
      END_STATE();
    case 341:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'n') ADVANCE(295);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(344);
      END_STATE();
    case 342:
      ACCEPT_TOKEN(sym_identifier);
      if (lookahead == 'u') ADVANCE(340);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(344);
      END_STATE();
    case 343:
      ACCEPT_TOKEN(sym_identifier);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(343);
      if (('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(344);
      END_STATE();
    case 344:
      ACCEPT_TOKEN(sym_identifier);
      if (('0' <= lookahead && lookahead <= '9') ||
          ('A' <= lookahead && lookahead <= 'Z') ||
          lookahead == '_' ||
          ('a' <= lookahead && lookahead <= 'z')) ADVANCE(344);
      END_STATE();
    case 345:
      ACCEPT_TOKEN(sym_number);
      if (lookahead == '%') ADVANCE(351);
      if (lookahead == '.') ADVANCE(271);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(345);
      END_STATE();
    case 346:
      ACCEPT_TOKEN(sym_number);
      if (lookahead == '.') ADVANCE(272);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(346);
      END_STATE();
    case 347:
      ACCEPT_TOKEN(sym_number);
      if (lookahead == 'p') ADVANCE(349);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(347);
      END_STATE();
    case 348:
      ACCEPT_TOKEN(sym_number);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(348);
      END_STATE();
    case 349:
      ACCEPT_TOKEN(aux_sym_probability_token1);
      END_STATE();
    case 350:
      ACCEPT_TOKEN(aux_sym_probability_token2);
      if (('0' <= lookahead && lookahead <= '9')) ADVANCE(350);
      END_STATE();
    case 351:
      ACCEPT_TOKEN(aux_sym_probability_token3);
      END_STATE();
    case 352:
      ACCEPT_TOKEN(sym_date);
      END_STATE();
    case 353:
      ACCEPT_TOKEN(anon_sym_DQUOTE);
      END_STATE();
    case 354:
      ACCEPT_TOKEN(aux_sym_string_token1);
      END_STATE();
    case 355:
      ACCEPT_TOKEN(aux_sym_string_token1);
      if (lookahead == '#') ADVANCE(357);
      if (lookahead == '/') ADVANCE(356);
      if (('\t' <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') ADVANCE(355);
      if (lookahead != 0 &&
          lookahead != '"' &&
          lookahead != '#' &&
          lookahead != '\\') ADVANCE(354);
      END_STATE();
    case 356:
      ACCEPT_TOKEN(aux_sym_string_token1);
      if (lookahead == '*') ADVANCE(8);
      if (lookahead == '/') ADVANCE(364);
      END_STATE();
    case 357:
      ACCEPT_TOKEN(aux_sym_string_token1);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(364);
      END_STATE();
    case 358:
      ACCEPT_TOKEN(anon_sym_BSLASH);
      END_STATE();
    case 359:
      ACCEPT_TOKEN(aux_sym_string_token2);
      END_STATE();
    case 360:
      ACCEPT_TOKEN(aux_sym_string_token2);
      if (lookahead == '#') ADVANCE(362);
      if (lookahead == '/') ADVANCE(361);
      if (lookahead == '\t' ||
          (0x0b <= lookahead && lookahead <= '\r') ||
          lookahead == ' ') ADVANCE(360);
      if (lookahead != 0 &&
          (lookahead < '\t' || '\r' < lookahead)) ADVANCE(359);
      END_STATE();
    case 361:
      ACCEPT_TOKEN(aux_sym_string_token2);
      if (lookahead == '*') ADVANCE(8);
      if (lookahead == '/') ADVANCE(364);
      END_STATE();
    case 362:
      ACCEPT_TOKEN(aux_sym_string_token2);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(364);
      END_STATE();
    case 363:
      ACCEPT_TOKEN(sym_comment);
      END_STATE();
    case 364:
      ACCEPT_TOKEN(sym_comment);
      if (lookahead != 0 &&
          lookahead != '\n') ADVANCE(364);
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
  [5] = {.lex_state = 2},
  [6] = {.lex_state = 0},
  [7] = {.lex_state = 0},
  [8] = {.lex_state = 2},
  [9] = {.lex_state = 2},
  [10] = {.lex_state = 0},
  [11] = {.lex_state = 2},
  [12] = {.lex_state = 2},
  [13] = {.lex_state = 2},
  [14] = {.lex_state = 0},
  [15] = {.lex_state = 2},
  [16] = {.lex_state = 2},
  [17] = {.lex_state = 2},
  [18] = {.lex_state = 0},
  [19] = {.lex_state = 2},
  [20] = {.lex_state = 2},
  [21] = {.lex_state = 0},
  [22] = {.lex_state = 2},
  [23] = {.lex_state = 2},
  [24] = {.lex_state = 2},
  [25] = {.lex_state = 2},
  [26] = {.lex_state = 2},
  [27] = {.lex_state = 2},
  [28] = {.lex_state = 2},
  [29] = {.lex_state = 2},
  [30] = {.lex_state = 2},
  [31] = {.lex_state = 2},
  [32] = {.lex_state = 0},
  [33] = {.lex_state = 2},
  [34] = {.lex_state = 0},
  [35] = {.lex_state = 0},
  [36] = {.lex_state = 0},
  [37] = {.lex_state = 2},
  [38] = {.lex_state = 0},
  [39] = {.lex_state = 2},
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
  [98] = {.lex_state = 0},
  [99] = {.lex_state = 0},
  [100] = {.lex_state = 0},
  [101] = {.lex_state = 0},
  [102] = {.lex_state = 0},
  [103] = {.lex_state = 0},
  [104] = {.lex_state = 0},
  [105] = {.lex_state = 0},
  [106] = {.lex_state = 0},
  [107] = {.lex_state = 0},
  [108] = {.lex_state = 0},
  [109] = {.lex_state = 3},
  [110] = {.lex_state = 0},
  [111] = {.lex_state = 3},
  [112] = {.lex_state = 3},
  [113] = {.lex_state = 0},
  [114] = {.lex_state = 0},
  [115] = {.lex_state = 0},
  [116] = {.lex_state = 0},
  [117] = {.lex_state = 0},
  [118] = {.lex_state = 0},
  [119] = {.lex_state = 0},
  [120] = {.lex_state = 0},
  [121] = {.lex_state = 0},
  [122] = {.lex_state = 3},
  [123] = {.lex_state = 0},
  [124] = {.lex_state = 0},
  [125] = {.lex_state = 0},
  [126] = {.lex_state = 0},
  [127] = {.lex_state = 0},
  [128] = {.lex_state = 0},
  [129] = {.lex_state = 0},
  [130] = {.lex_state = 0},
  [131] = {.lex_state = 4},
  [132] = {.lex_state = 0},
  [133] = {.lex_state = 0},
  [134] = {.lex_state = 0},
  [135] = {.lex_state = 0},
  [136] = {.lex_state = 0},
  [137] = {.lex_state = 0},
  [138] = {.lex_state = 1},
  [139] = {.lex_state = 0},
  [140] = {.lex_state = 0},
  [141] = {.lex_state = 0},
  [142] = {.lex_state = 0},
  [143] = {.lex_state = 0},
  [144] = {.lex_state = 0},
  [145] = {.lex_state = 0},
  [146] = {.lex_state = 0},
  [147] = {.lex_state = 0},
  [148] = {.lex_state = 0},
  [149] = {.lex_state = 0},
  [150] = {.lex_state = 0},
  [151] = {.lex_state = 4},
  [152] = {.lex_state = 4},
  [153] = {.lex_state = 0},
  [154] = {.lex_state = 0},
  [155] = {.lex_state = 0},
  [156] = {.lex_state = 0},
  [157] = {.lex_state = 5},
  [158] = {.lex_state = 0},
  [159] = {.lex_state = 0},
  [160] = {.lex_state = 4},
  [161] = {.lex_state = 5},
  [162] = {.lex_state = 0},
  [163] = {.lex_state = 0},
  [164] = {.lex_state = 0},
  [165] = {.lex_state = 5},
  [166] = {.lex_state = 5},
  [167] = {.lex_state = 4},
  [168] = {.lex_state = 0},
};

static const uint16_t ts_parse_table[LARGE_STATE_COUNT][SYMBOL_COUNT] = {
  [0] = {
    [ts_builtin_sym_end] = ACTIONS(1),
    [anon_sym_question] = ACTIONS(1),
    [anon_sym_LBRACE] = ACTIONS(1),
    [anon_sym_RBRACE] = ACTIONS(1),
    [anon_sym_target_date] = ACTIONS(1),
    [anon_sym_COLON] = ACTIONS(1),
    [anon_sym_resolution_criteria] = ACTIONS(1),
    [anon_sym_base_rate] = ACTIONS(1),
    [anon_sym_reference_class] = ACTIONS(1),
    [anon_sym_historical_frequency] = ACTIONS(1),
    [anon_sym_sample_size] = ACTIONS(1),
    [anon_sym_source] = ACTIONS(1),
    [anon_sym_reasoning] = ACTIONS(1),
    [anon_sym_generated_by] = ACTIONS(1),
    [anon_sym_human] = ACTIONS(1),
    [anon_sym_driver] = ACTIONS(1),
    [anon_sym_continuous] = ACTIONS(1),
    [anon_sym_binary] = ACTIONS(1),
    [anon_sym_discrete] = ACTIONS(1),
    [anon_sym_distribution] = ACTIONS(1),
    [anon_sym_probability] = ACTIONS(1),
    [anon_sym_unit] = ACTIONS(1),
    [anon_sym_rationale] = ACTIONS(1),
    [anon_sym_impact_multiplier] = ACTIONS(1),
    [anon_sym_evidence] = ACTIONS(1),
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
    [sym_source_file] = STATE(150),
    [sym__statement] = STATE(40),
    [sym_question_statement] = STATE(40),
    [sym_driver_statement] = STATE(40),
    [sym_evidence_statement] = STATE(40),
    [sym_agent_statement] = STATE(40),
    [sym_model_statement] = STATE(40),
    [sym_simulate_statement] = STATE(40),
    [aux_sym_source_file_repeat1] = STATE(40),
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
    ACTIONS(19), 30,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_RBRACE,
      anon_sym_reference_class,
      anon_sym_historical_frequency,
      anon_sym_sample_size,
      anon_sym_source,
      anon_sym_reasoning,
      anon_sym_generated_by,
      anon_sym_driver,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
      anon_sym_evidence,
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
  [39] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(23), 28,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_LBRACE,
      anon_sym_RBRACE,
      anon_sym_target_date,
      anon_sym_resolution_criteria,
      anon_sym_base_rate,
      anon_sym_reference_class,
      anon_sym_historical_frequency,
      anon_sym_sample_size,
      anon_sym_source,
      anon_sym_reasoning,
      anon_sym_generated_by,
      anon_sym_driver,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
      anon_sym_evidence,
      anon_sym_summary,
      anon_sym_relevance,
      anon_sym_date,
      anon_sym_agent,
      anon_sym_query,
      anon_sym_schedule,
      anon_sym_model,
      anon_sym_simulate,
  [73] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(25), 28,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_LBRACE,
      anon_sym_RBRACE,
      anon_sym_target_date,
      anon_sym_resolution_criteria,
      anon_sym_base_rate,
      anon_sym_reference_class,
      anon_sym_historical_frequency,
      anon_sym_sample_size,
      anon_sym_source,
      anon_sym_reasoning,
      anon_sym_generated_by,
      anon_sym_driver,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
      anon_sym_evidence,
      anon_sym_summary,
      anon_sym_relevance,
      anon_sym_date,
      anon_sym_agent,
      anon_sym_query,
      anon_sym_schedule,
      anon_sym_model,
      anon_sym_simulate,
  [107] = 11,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LPAREN,
    ACTIONS(29), 1,
      anon_sym_RPAREN,
    ACTIONS(33), 1,
      anon_sym_if,
    ACTIONS(35), 1,
      sym_identifier,
    ACTIONS(37), 1,
      sym_number,
    ACTIONS(41), 1,
      aux_sym_probability_token2,
    STATE(51), 1,
      sym_expression,
    ACTIONS(31), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(39), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(32), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [148] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(45), 1,
      anon_sym_LPAREN,
    ACTIONS(47), 1,
      anon_sym_SLASH,
    ACTIONS(43), 15,
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
  [175] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(51), 1,
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
  [199] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LPAREN,
    ACTIONS(33), 1,
      anon_sym_if,
    ACTIONS(35), 1,
      sym_identifier,
    ACTIONS(37), 1,
      sym_number,
    ACTIONS(41), 1,
      aux_sym_probability_token2,
    STATE(81), 1,
      sym_expression,
    ACTIONS(31), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(39), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(32), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [237] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LPAREN,
    ACTIONS(33), 1,
      anon_sym_if,
    ACTIONS(35), 1,
      sym_identifier,
    ACTIONS(37), 1,
      sym_number,
    ACTIONS(41), 1,
      aux_sym_probability_token2,
    STATE(87), 1,
      sym_expression,
    ACTIONS(31), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(39), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(32), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [275] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(55), 1,
      anon_sym_SLASH,
    ACTIONS(53), 15,
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
  [299] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LPAREN,
    ACTIONS(33), 1,
      anon_sym_if,
    ACTIONS(35), 1,
      sym_identifier,
    ACTIONS(37), 1,
      sym_number,
    ACTIONS(41), 1,
      aux_sym_probability_token2,
    STATE(88), 1,
      sym_expression,
    ACTIONS(31), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(39), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(32), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [337] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LPAREN,
    ACTIONS(33), 1,
      anon_sym_if,
    ACTIONS(35), 1,
      sym_identifier,
    ACTIONS(37), 1,
      sym_number,
    ACTIONS(41), 1,
      aux_sym_probability_token2,
    STATE(75), 1,
      sym_expression,
    ACTIONS(31), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(39), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(32), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [375] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LPAREN,
    ACTIONS(33), 1,
      anon_sym_if,
    ACTIONS(35), 1,
      sym_identifier,
    ACTIONS(37), 1,
      sym_number,
    ACTIONS(41), 1,
      aux_sym_probability_token2,
    STATE(89), 1,
      sym_expression,
    ACTIONS(31), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(39), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(32), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [413] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(59), 1,
      anon_sym_STAR,
    ACTIONS(61), 1,
      anon_sym_SLASH,
    ACTIONS(65), 1,
      anon_sym_CARET,
    ACTIONS(63), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
    ACTIONS(57), 11,
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
  [443] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LPAREN,
    ACTIONS(33), 1,
      anon_sym_if,
    ACTIONS(35), 1,
      sym_identifier,
    ACTIONS(37), 1,
      sym_number,
    ACTIONS(41), 1,
      aux_sym_probability_token2,
    STATE(91), 1,
      sym_expression,
    ACTIONS(31), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(39), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(32), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [481] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LPAREN,
    ACTIONS(33), 1,
      anon_sym_if,
    ACTIONS(35), 1,
      sym_identifier,
    ACTIONS(37), 1,
      sym_number,
    ACTIONS(41), 1,
      aux_sym_probability_token2,
    STATE(45), 1,
      sym_expression,
    ACTIONS(31), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(39), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(32), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [519] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LPAREN,
    ACTIONS(33), 1,
      anon_sym_if,
    ACTIONS(35), 1,
      sym_identifier,
    ACTIONS(37), 1,
      sym_number,
    ACTIONS(41), 1,
      aux_sym_probability_token2,
    STATE(92), 1,
      sym_expression,
    ACTIONS(31), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(39), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(32), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [557] = 3,
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
  [581] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LPAREN,
    ACTIONS(33), 1,
      anon_sym_if,
    ACTIONS(35), 1,
      sym_identifier,
    ACTIONS(37), 1,
      sym_number,
    ACTIONS(41), 1,
      aux_sym_probability_token2,
    STATE(93), 1,
      sym_expression,
    ACTIONS(31), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(39), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(32), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [619] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LPAREN,
    ACTIONS(33), 1,
      anon_sym_if,
    ACTIONS(35), 1,
      sym_identifier,
    ACTIONS(37), 1,
      sym_number,
    ACTIONS(41), 1,
      aux_sym_probability_token2,
    STATE(94), 1,
      sym_expression,
    ACTIONS(31), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(39), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(32), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [657] = 3,
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
  [681] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LPAREN,
    ACTIONS(33), 1,
      anon_sym_if,
    ACTIONS(35), 1,
      sym_identifier,
    ACTIONS(37), 1,
      sym_number,
    ACTIONS(41), 1,
      aux_sym_probability_token2,
    STATE(36), 1,
      sym_expression,
    ACTIONS(31), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(39), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(32), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [719] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LPAREN,
    ACTIONS(33), 1,
      anon_sym_if,
    ACTIONS(35), 1,
      sym_identifier,
    ACTIONS(37), 1,
      sym_number,
    ACTIONS(41), 1,
      aux_sym_probability_token2,
    STATE(95), 1,
      sym_expression,
    ACTIONS(31), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(39), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(32), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [757] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LPAREN,
    ACTIONS(33), 1,
      anon_sym_if,
    ACTIONS(35), 1,
      sym_identifier,
    ACTIONS(37), 1,
      sym_number,
    ACTIONS(41), 1,
      aux_sym_probability_token2,
    STATE(14), 1,
      sym_expression,
    ACTIONS(31), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(39), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(32), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [795] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LPAREN,
    ACTIONS(33), 1,
      anon_sym_if,
    ACTIONS(35), 1,
      sym_identifier,
    ACTIONS(37), 1,
      sym_number,
    ACTIONS(41), 1,
      aux_sym_probability_token2,
    STATE(34), 1,
      sym_expression,
    ACTIONS(31), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(39), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(32), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [833] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LPAREN,
    ACTIONS(33), 1,
      anon_sym_if,
    ACTIONS(35), 1,
      sym_identifier,
    ACTIONS(37), 1,
      sym_number,
    ACTIONS(41), 1,
      aux_sym_probability_token2,
    STATE(21), 1,
      sym_expression,
    ACTIONS(31), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(39), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(32), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [871] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LPAREN,
    ACTIONS(33), 1,
      anon_sym_if,
    ACTIONS(35), 1,
      sym_identifier,
    ACTIONS(37), 1,
      sym_number,
    ACTIONS(41), 1,
      aux_sym_probability_token2,
    STATE(80), 1,
      sym_expression,
    ACTIONS(31), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(39), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(32), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [909] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LPAREN,
    ACTIONS(33), 1,
      anon_sym_if,
    ACTIONS(35), 1,
      sym_identifier,
    ACTIONS(37), 1,
      sym_number,
    ACTIONS(41), 1,
      aux_sym_probability_token2,
    STATE(97), 1,
      sym_expression,
    ACTIONS(31), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(39), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(32), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [947] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LPAREN,
    ACTIONS(33), 1,
      anon_sym_if,
    ACTIONS(35), 1,
      sym_identifier,
    ACTIONS(37), 1,
      sym_number,
    ACTIONS(41), 1,
      aux_sym_probability_token2,
    STATE(35), 1,
      sym_expression,
    ACTIONS(31), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(39), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(32), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [985] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LPAREN,
    ACTIONS(33), 1,
      anon_sym_if,
    ACTIONS(35), 1,
      sym_identifier,
    ACTIONS(37), 1,
      sym_number,
    ACTIONS(41), 1,
      aux_sym_probability_token2,
    STATE(90), 1,
      sym_expression,
    ACTIONS(31), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(39), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(32), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [1023] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LPAREN,
    ACTIONS(33), 1,
      anon_sym_if,
    ACTIONS(35), 1,
      sym_identifier,
    ACTIONS(37), 1,
      sym_number,
    ACTIONS(41), 1,
      aux_sym_probability_token2,
    STATE(78), 1,
      sym_expression,
    ACTIONS(31), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(39), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(32), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [1061] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(47), 1,
      anon_sym_SLASH,
    ACTIONS(43), 15,
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
  [1085] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LPAREN,
    ACTIONS(33), 1,
      anon_sym_if,
    ACTIONS(35), 1,
      sym_identifier,
    ACTIONS(37), 1,
      sym_number,
    ACTIONS(41), 1,
      aux_sym_probability_token2,
    STATE(52), 1,
      sym_expression,
    ACTIONS(31), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(39), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(32), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [1123] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(59), 1,
      anon_sym_STAR,
    ACTIONS(61), 1,
      anon_sym_SLASH,
    ACTIONS(71), 14,
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
  [1149] = 3,
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
  [1173] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(59), 1,
      anon_sym_STAR,
    ACTIONS(61), 1,
      anon_sym_SLASH,
    ACTIONS(63), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
    ACTIONS(71), 12,
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
  [1201] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LPAREN,
    ACTIONS(33), 1,
      anon_sym_if,
    ACTIONS(35), 1,
      sym_identifier,
    ACTIONS(37), 1,
      sym_number,
    ACTIONS(41), 1,
      aux_sym_probability_token2,
    STATE(64), 1,
      sym_expression,
    ACTIONS(31), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(39), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(32), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [1239] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(81), 1,
      anon_sym_SLASH,
    ACTIONS(79), 15,
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
  [1263] = 10,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(27), 1,
      anon_sym_LPAREN,
    ACTIONS(33), 1,
      anon_sym_if,
    ACTIONS(35), 1,
      sym_identifier,
    ACTIONS(37), 1,
      sym_number,
    ACTIONS(41), 1,
      aux_sym_probability_token2,
    STATE(86), 1,
      sym_expression,
    ACTIONS(31), 2,
      anon_sym_DASH,
      anon_sym_BANG,
    ACTIONS(39), 2,
      aux_sym_probability_token1,
      aux_sym_probability_token3,
    STATE(32), 6,
      sym_binary_expression,
      sym_unary_expression,
      sym_conditional_expression,
      sym_function_call,
      sym_parenthesized_expression,
      sym_probability,
  [1301] = 9,
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
    ACTIONS(83), 1,
      ts_builtin_sym_end,
    STATE(41), 8,
      sym__statement,
      sym_question_statement,
      sym_driver_statement,
      sym_evidence_statement,
      sym_agent_statement,
      sym_model_statement,
      sym_simulate_statement,
      aux_sym_source_file_repeat1,
  [1336] = 9,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(85), 1,
      ts_builtin_sym_end,
    ACTIONS(87), 1,
      anon_sym_question,
    ACTIONS(90), 1,
      anon_sym_driver,
    ACTIONS(93), 1,
      anon_sym_evidence,
    ACTIONS(96), 1,
      anon_sym_agent,
    ACTIONS(99), 1,
      anon_sym_model,
    ACTIONS(102), 1,
      anon_sym_simulate,
    STATE(41), 8,
      sym__statement,
      sym_question_statement,
      sym_driver_statement,
      sym_evidence_statement,
      sym_agent_statement,
      sym_model_statement,
      sym_simulate_statement,
      aux_sym_source_file_repeat1,
  [1371] = 9,
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
    STATE(77), 5,
      sym_distribution_property,
      sym_probability_property,
      sym_unit_property,
      sym_rationale_property,
      sym_impact_multiplier_property,
  [1404] = 9,
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
    STATE(77), 5,
      sym_distribution_property,
      sym_probability_property,
      sym_unit_property,
      sym_rationale_property,
      sym_impact_multiplier_property,
  [1437] = 9,
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
    STATE(77), 5,
      sym_distribution_property,
      sym_probability_property,
      sym_unit_property,
      sym_rationale_property,
      sym_impact_multiplier_property,
  [1470] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(59), 1,
      anon_sym_STAR,
    ACTIONS(61), 1,
      anon_sym_SLASH,
    ACTIONS(65), 1,
      anon_sym_CARET,
    ACTIONS(63), 2,
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
  [1496] = 8,
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
    STATE(101), 1,
      sym_distribution,
    STATE(100), 5,
      sym_triangular_distribution,
      sym_normal_distribution,
      sym_lognormal_distribution,
      sym_uniform_distribution,
      sym_beta_distribution,
  [1525] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(148), 1,
      anon_sym_RBRACE,
    ACTIONS(152), 1,
      anon_sym_historical_frequency,
    ACTIONS(154), 1,
      anon_sym_sample_size,
    ACTIONS(156), 1,
      anon_sym_generated_by,
    STATE(48), 2,
      sym_base_rate_field,
      aux_sym_base_rate_block_repeat1,
    ACTIONS(150), 3,
      anon_sym_reference_class,
      anon_sym_source,
      anon_sym_reasoning,
  [1550] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(158), 1,
      anon_sym_RBRACE,
    ACTIONS(163), 1,
      anon_sym_historical_frequency,
    ACTIONS(166), 1,
      anon_sym_sample_size,
    ACTIONS(169), 1,
      anon_sym_generated_by,
    STATE(48), 2,
      sym_base_rate_field,
      aux_sym_base_rate_block_repeat1,
    ACTIONS(160), 3,
      anon_sym_reference_class,
      anon_sym_source,
      anon_sym_reasoning,
  [1575] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(174), 1,
      anon_sym_LBRACE,
    STATE(61), 1,
      sym_question_block,
    ACTIONS(172), 7,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
  [1594] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(152), 1,
      anon_sym_historical_frequency,
    ACTIONS(154), 1,
      anon_sym_sample_size,
    ACTIONS(156), 1,
      anon_sym_generated_by,
    ACTIONS(176), 1,
      anon_sym_RBRACE,
    STATE(47), 2,
      sym_base_rate_field,
      aux_sym_base_rate_block_repeat1,
    ACTIONS(150), 3,
      anon_sym_reference_class,
      anon_sym_source,
      anon_sym_reasoning,
  [1619] = 8,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(59), 1,
      anon_sym_STAR,
    ACTIONS(61), 1,
      anon_sym_SLASH,
    ACTIONS(65), 1,
      anon_sym_CARET,
    ACTIONS(178), 1,
      anon_sym_COMMA,
    ACTIONS(180), 1,
      anon_sym_RPAREN,
    STATE(120), 1,
      aux_sym_function_call_repeat1,
    ACTIONS(63), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [1645] = 7,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(59), 1,
      anon_sym_STAR,
    ACTIONS(61), 1,
      anon_sym_SLASH,
    ACTIONS(65), 1,
      anon_sym_CARET,
    ACTIONS(182), 1,
      anon_sym_COMMA,
    ACTIONS(184), 1,
      anon_sym_RPAREN,
    ACTIONS(63), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [1668] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(186), 1,
      anon_sym_RBRACE,
    ACTIONS(190), 1,
      anon_sym_relevance,
    ACTIONS(192), 1,
      anon_sym_date,
    ACTIONS(188), 2,
      anon_sym_source,
      anon_sym_summary,
    STATE(72), 2,
      sym_evidence_property,
      aux_sym_evidence_block_repeat1,
  [1689] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(194), 7,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
  [1702] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(196), 7,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
  [1715] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(198), 7,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
  [1728] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(190), 1,
      anon_sym_relevance,
    ACTIONS(192), 1,
      anon_sym_date,
    ACTIONS(200), 1,
      anon_sym_RBRACE,
    ACTIONS(188), 2,
      anon_sym_source,
      anon_sym_summary,
    STATE(53), 2,
      sym_evidence_property,
      aux_sym_evidence_block_repeat1,
  [1749] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(202), 7,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
  [1762] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(204), 7,
      anon_sym_RBRACE,
      anon_sym_reference_class,
      anon_sym_historical_frequency,
      anon_sym_sample_size,
      anon_sym_source,
      anon_sym_reasoning,
      anon_sym_generated_by,
  [1775] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(206), 7,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
  [1788] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(208), 7,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
  [1801] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(210), 1,
      anon_sym_RBRACE,
    ACTIONS(214), 1,
      anon_sym_base_rate,
    STATE(115), 1,
      sym_base_rate_property,
    ACTIONS(212), 2,
      anon_sym_target_date,
      anon_sym_resolution_criteria,
    STATE(70), 2,
      sym_question_property,
      aux_sym_question_block_repeat1,
  [1822] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(216), 7,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
  [1835] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(59), 1,
      anon_sym_STAR,
    ACTIONS(61), 1,
      anon_sym_SLASH,
    ACTIONS(65), 1,
      anon_sym_CARET,
    ACTIONS(63), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
    ACTIONS(218), 2,
      anon_sym_COMMA,
      anon_sym_RPAREN,
  [1856] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(220), 7,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
  [1869] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(222), 1,
      anon_sym_RBRACE,
    ACTIONS(227), 1,
      anon_sym_base_rate,
    STATE(115), 1,
      sym_base_rate_property,
    ACTIONS(224), 2,
      anon_sym_target_date,
      anon_sym_resolution_criteria,
    STATE(66), 2,
      sym_question_property,
      aux_sym_question_block_repeat1,
  [1890] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(230), 7,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
  [1903] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(232), 7,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
  [1916] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(234), 7,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
  [1929] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(214), 1,
      anon_sym_base_rate,
    ACTIONS(236), 1,
      anon_sym_RBRACE,
    STATE(115), 1,
      sym_base_rate_property,
    ACTIONS(212), 2,
      anon_sym_target_date,
      anon_sym_resolution_criteria,
    STATE(66), 2,
      sym_question_property,
      aux_sym_question_block_repeat1,
  [1950] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(238), 7,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
  [1963] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(240), 1,
      anon_sym_RBRACE,
    ACTIONS(245), 1,
      anon_sym_relevance,
    ACTIONS(248), 1,
      anon_sym_date,
    ACTIONS(242), 2,
      anon_sym_source,
      anon_sym_summary,
    STATE(72), 2,
      sym_evidence_property,
      aux_sym_evidence_block_repeat1,
  [1984] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(251), 7,
      ts_builtin_sym_end,
      anon_sym_question,
      anon_sym_driver,
      anon_sym_evidence,
      anon_sym_agent,
      anon_sym_model,
      anon_sym_simulate,
  [1997] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(253), 6,
      anon_sym_RBRACE,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
  [2009] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(59), 1,
      anon_sym_STAR,
    ACTIONS(61), 1,
      anon_sym_SLASH,
    ACTIONS(65), 1,
      anon_sym_CARET,
    ACTIONS(255), 1,
      anon_sym_else,
    ACTIONS(63), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [2029] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(257), 6,
      anon_sym_RBRACE,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
  [2041] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(259), 6,
      anon_sym_RBRACE,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
  [2053] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(59), 1,
      anon_sym_STAR,
    ACTIONS(61), 1,
      anon_sym_SLASH,
    ACTIONS(65), 1,
      anon_sym_CARET,
    ACTIONS(261), 1,
      anon_sym_RPAREN,
    ACTIONS(63), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [2073] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(263), 6,
      anon_sym_RBRACE,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
  [2085] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(59), 1,
      anon_sym_STAR,
    ACTIONS(61), 1,
      anon_sym_SLASH,
    ACTIONS(65), 1,
      anon_sym_CARET,
    ACTIONS(265), 1,
      anon_sym_COMMA,
    ACTIONS(63), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [2105] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(59), 1,
      anon_sym_STAR,
    ACTIONS(61), 1,
      anon_sym_SLASH,
    ACTIONS(65), 1,
      anon_sym_CARET,
    ACTIONS(267), 1,
      anon_sym_RPAREN,
    ACTIONS(63), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [2125] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(269), 6,
      anon_sym_RBRACE,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
  [2137] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(271), 6,
      anon_sym_RBRACE,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
  [2149] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(273), 6,
      anon_sym_RBRACE,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
  [2161] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(275), 6,
      anon_sym_RBRACE,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
  [2173] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(59), 1,
      anon_sym_STAR,
    ACTIONS(61), 1,
      anon_sym_SLASH,
    ACTIONS(65), 1,
      anon_sym_CARET,
    ACTIONS(277), 1,
      anon_sym_RPAREN,
    ACTIONS(63), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [2193] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(59), 1,
      anon_sym_STAR,
    ACTIONS(61), 1,
      anon_sym_SLASH,
    ACTIONS(65), 1,
      anon_sym_CARET,
    ACTIONS(279), 1,
      anon_sym_RPAREN,
    ACTIONS(63), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [2213] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(59), 1,
      anon_sym_STAR,
    ACTIONS(61), 1,
      anon_sym_SLASH,
    ACTIONS(65), 1,
      anon_sym_CARET,
    ACTIONS(281), 1,
      anon_sym_RPAREN,
    ACTIONS(63), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [2233] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(59), 1,
      anon_sym_STAR,
    ACTIONS(61), 1,
      anon_sym_SLASH,
    ACTIONS(65), 1,
      anon_sym_CARET,
    ACTIONS(283), 1,
      anon_sym_COMMA,
    ACTIONS(63), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [2253] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(59), 1,
      anon_sym_STAR,
    ACTIONS(61), 1,
      anon_sym_SLASH,
    ACTIONS(65), 1,
      anon_sym_CARET,
    ACTIONS(285), 1,
      anon_sym_then,
    ACTIONS(63), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [2273] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(59), 1,
      anon_sym_STAR,
    ACTIONS(61), 1,
      anon_sym_SLASH,
    ACTIONS(65), 1,
      anon_sym_CARET,
    ACTIONS(287), 1,
      anon_sym_COMMA,
    ACTIONS(63), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [2293] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(59), 1,
      anon_sym_STAR,
    ACTIONS(61), 1,
      anon_sym_SLASH,
    ACTIONS(65), 1,
      anon_sym_CARET,
    ACTIONS(289), 1,
      anon_sym_COMMA,
    ACTIONS(63), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [2313] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(59), 1,
      anon_sym_STAR,
    ACTIONS(61), 1,
      anon_sym_SLASH,
    ACTIONS(65), 1,
      anon_sym_CARET,
    ACTIONS(291), 1,
      anon_sym_COMMA,
    ACTIONS(63), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [2333] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(59), 1,
      anon_sym_STAR,
    ACTIONS(61), 1,
      anon_sym_SLASH,
    ACTIONS(65), 1,
      anon_sym_CARET,
    ACTIONS(293), 1,
      anon_sym_COMMA,
    ACTIONS(63), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [2353] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(59), 1,
      anon_sym_STAR,
    ACTIONS(61), 1,
      anon_sym_SLASH,
    ACTIONS(65), 1,
      anon_sym_CARET,
    ACTIONS(295), 1,
      anon_sym_COMMA,
    ACTIONS(63), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [2373] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(297), 6,
      anon_sym_RBRACE,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
  [2385] = 6,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(59), 1,
      anon_sym_STAR,
    ACTIONS(61), 1,
      anon_sym_SLASH,
    ACTIONS(65), 1,
      anon_sym_CARET,
    ACTIONS(299), 1,
      anon_sym_RPAREN,
    ACTIONS(63), 2,
      anon_sym_PLUS,
      anon_sym_DASH,
  [2405] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(301), 6,
      anon_sym_RBRACE,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
  [2417] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(303), 6,
      anon_sym_RBRACE,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
  [2429] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(305), 6,
      anon_sym_RBRACE,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
  [2441] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(307), 6,
      anon_sym_RBRACE,
      anon_sym_distribution,
      anon_sym_probability,
      anon_sym_unit,
      anon_sym_rationale,
      anon_sym_impact_multiplier,
  [2453] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(309), 1,
      anon_sym_RBRACE,
    ACTIONS(311), 1,
      anon_sym_query,
    ACTIONS(313), 1,
      anon_sym_schedule,
    STATE(103), 2,
      sym_agent_property,
      aux_sym_agent_block_repeat1,
  [2470] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(315), 1,
      anon_sym_RBRACE,
    ACTIONS(317), 1,
      anon_sym_query,
    ACTIONS(320), 1,
      anon_sym_schedule,
    STATE(103), 2,
      sym_agent_property,
      aux_sym_agent_block_repeat1,
  [2487] = 5,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(311), 1,
      anon_sym_query,
    ACTIONS(313), 1,
      anon_sym_schedule,
    ACTIONS(323), 1,
      anon_sym_RBRACE,
    STATE(102), 2,
      sym_agent_property,
      aux_sym_agent_block_repeat1,
  [2504] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(325), 1,
      sym_number,
    STATE(106), 1,
      sym_probability,
    ACTIONS(39), 3,
      aux_sym_probability_token1,
      aux_sym_probability_token2,
      aux_sym_probability_token3,
  [2519] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(327), 5,
      anon_sym_RBRACE,
      anon_sym_source,
      anon_sym_summary,
      anon_sym_relevance,
      anon_sym_date,
  [2530] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(329), 1,
      sym_number,
    STATE(99), 1,
      sym_probability,
    ACTIONS(39), 3,
      aux_sym_probability_token1,
      aux_sym_probability_token2,
      aux_sym_probability_token3,
  [2545] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(331), 1,
      sym_number,
    STATE(59), 1,
      sym_probability,
    ACTIONS(39), 3,
      aux_sym_probability_token1,
      aux_sym_probability_token2,
      aux_sym_probability_token3,
  [2560] = 5,
    ACTIONS(333), 1,
      anon_sym_DQUOTE,
    ACTIONS(335), 1,
      aux_sym_string_token1,
    ACTIONS(337), 1,
      anon_sym_BSLASH,
    ACTIONS(339), 1,
      sym_comment,
    STATE(112), 1,
      aux_sym_string_repeat1,
  [2576] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(341), 4,
      anon_sym_RBRACE,
      anon_sym_target_date,
      anon_sym_resolution_criteria,
      anon_sym_base_rate,
  [2586] = 5,
    ACTIONS(339), 1,
      sym_comment,
    ACTIONS(343), 1,
      anon_sym_DQUOTE,
    ACTIONS(345), 1,
      aux_sym_string_token1,
    ACTIONS(348), 1,
      anon_sym_BSLASH,
    STATE(111), 1,
      aux_sym_string_repeat1,
  [2602] = 5,
    ACTIONS(337), 1,
      anon_sym_BSLASH,
    ACTIONS(339), 1,
      sym_comment,
    ACTIONS(351), 1,
      anon_sym_DQUOTE,
    ACTIONS(353), 1,
      aux_sym_string_token1,
    STATE(111), 1,
      aux_sym_string_repeat1,
  [2618] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(355), 2,
      anon_sym_day,
      anon_sym_week,
    ACTIONS(357), 2,
      anon_sym_days,
      anon_sym_weeks,
  [2630] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(359), 4,
      anon_sym_RBRACE,
      anon_sym_target_date,
      anon_sym_resolution_criteria,
      anon_sym_base_rate,
  [2640] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(361), 4,
      anon_sym_RBRACE,
      anon_sym_target_date,
      anon_sym_resolution_criteria,
      anon_sym_base_rate,
  [2650] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(363), 4,
      anon_sym_RBRACE,
      anon_sym_target_date,
      anon_sym_resolution_criteria,
      anon_sym_base_rate,
  [2660] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(365), 4,
      anon_sym_RBRACE,
      anon_sym_target_date,
      anon_sym_resolution_criteria,
      anon_sym_base_rate,
  [2670] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(367), 3,
      anon_sym_continuous,
      anon_sym_binary,
      anon_sym_discrete,
  [2679] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(369), 3,
      anon_sym_RBRACE,
      anon_sym_query,
      anon_sym_schedule,
  [2688] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(178), 1,
      anon_sym_COMMA,
    ACTIONS(371), 1,
      anon_sym_RPAREN,
    STATE(121), 1,
      aux_sym_function_call_repeat1,
  [2701] = 4,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(373), 1,
      anon_sym_COMMA,
    ACTIONS(376), 1,
      anon_sym_RPAREN,
    STATE(121), 1,
      aux_sym_function_call_repeat1,
  [2714] = 2,
    ACTIONS(339), 1,
      sym_comment,
    ACTIONS(343), 3,
      anon_sym_DQUOTE,
      aux_sym_string_token1,
      anon_sym_BSLASH,
  [2723] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(378), 3,
      anon_sym_RBRACE,
      anon_sym_query,
      anon_sym_schedule,
  [2732] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(380), 1,
      anon_sym_DQUOTE,
    STATE(117), 1,
      sym_string,
  [2742] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(382), 1,
      anon_sym_LBRACE,
    STATE(69), 1,
      sym_driver_block,
  [2752] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(384), 1,
      anon_sym_LBRACE,
    STATE(60), 1,
      sym_evidence_block,
  [2762] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(380), 1,
      anon_sym_DQUOTE,
    STATE(123), 1,
      sym_string,
  [2772] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(386), 1,
      anon_sym_LBRACE,
    STATE(58), 1,
      sym_agent_block,
  [2782] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(380), 1,
      anon_sym_DQUOTE,
    STATE(98), 1,
      sym_string,
  [2792] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(380), 1,
      anon_sym_DQUOTE,
    STATE(74), 1,
      sym_string,
  [2802] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(331), 2,
      anon_sym_human,
      sym_identifier,
  [2810] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(380), 1,
      anon_sym_DQUOTE,
    STATE(106), 1,
      sym_string,
  [2820] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(380), 1,
      anon_sym_DQUOTE,
    STATE(59), 1,
      sym_string,
  [2830] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(388), 1,
      anon_sym_LBRACE,
    STATE(114), 1,
      sym_base_rate_block,
  [2840] = 3,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(380), 1,
      anon_sym_DQUOTE,
    STATE(49), 1,
      sym_string,
  [2850] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(390), 1,
      anon_sym_LPAREN,
  [2857] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(392), 1,
      anon_sym_COLON,
  [2864] = 2,
    ACTIONS(339), 1,
      sym_comment,
    ACTIONS(394), 1,
      aux_sym_string_token2,
  [2871] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(396), 1,
      anon_sym_iterations,
  [2878] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(398), 1,
      anon_sym_COLON,
  [2885] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(400), 1,
      anon_sym_COLON,
  [2892] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(402), 1,
      anon_sym_COLON,
  [2899] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(404), 1,
      anon_sym_COLON,
  [2906] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(406), 1,
      anon_sym_COLON,
  [2913] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(408), 1,
      anon_sym_COLON,
  [2920] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(410), 1,
      anon_sym_COLON,
  [2927] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(412), 1,
      anon_sym_LPAREN,
  [2934] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(414), 1,
      anon_sym_LPAREN,
  [2941] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(416), 1,
      anon_sym_LPAREN,
  [2948] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(418), 1,
      ts_builtin_sym_end,
  [2955] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(420), 1,
      sym_number,
  [2962] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(422), 1,
      sym_number,
  [2969] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(424), 1,
      anon_sym_LPAREN,
  [2976] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(426), 1,
      anon_sym_COLON,
  [2983] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(428), 1,
      anon_sym_COLON,
  [2990] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(430), 1,
      anon_sym_COLON,
  [2997] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(432), 1,
      sym_identifier,
  [3004] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(434), 1,
      anon_sym_COLON,
  [3011] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(436), 1,
      anon_sym_COLON,
  [3018] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(438), 1,
      sym_number,
  [3025] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(440), 1,
      sym_identifier,
  [3032] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(442), 1,
      anon_sym_COLON,
  [3039] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(444), 1,
      anon_sym_every,
  [3046] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(446), 1,
      anon_sym_COLON,
  [3053] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(448), 1,
      sym_date,
  [3060] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(450), 1,
      sym_identifier,
  [3067] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(452), 1,
      sym_number,
  [3074] = 2,
    ACTIONS(3), 1,
      sym_comment,
    ACTIONS(454), 1,
      anon_sym_COLON,
};

static const uint32_t ts_small_parse_table_map[] = {
  [SMALL_STATE(2)] = 0,
  [SMALL_STATE(3)] = 39,
  [SMALL_STATE(4)] = 73,
  [SMALL_STATE(5)] = 107,
  [SMALL_STATE(6)] = 148,
  [SMALL_STATE(7)] = 175,
  [SMALL_STATE(8)] = 199,
  [SMALL_STATE(9)] = 237,
  [SMALL_STATE(10)] = 275,
  [SMALL_STATE(11)] = 299,
  [SMALL_STATE(12)] = 337,
  [SMALL_STATE(13)] = 375,
  [SMALL_STATE(14)] = 413,
  [SMALL_STATE(15)] = 443,
  [SMALL_STATE(16)] = 481,
  [SMALL_STATE(17)] = 519,
  [SMALL_STATE(18)] = 557,
  [SMALL_STATE(19)] = 581,
  [SMALL_STATE(20)] = 619,
  [SMALL_STATE(21)] = 657,
  [SMALL_STATE(22)] = 681,
  [SMALL_STATE(23)] = 719,
  [SMALL_STATE(24)] = 757,
  [SMALL_STATE(25)] = 795,
  [SMALL_STATE(26)] = 833,
  [SMALL_STATE(27)] = 871,
  [SMALL_STATE(28)] = 909,
  [SMALL_STATE(29)] = 947,
  [SMALL_STATE(30)] = 985,
  [SMALL_STATE(31)] = 1023,
  [SMALL_STATE(32)] = 1061,
  [SMALL_STATE(33)] = 1085,
  [SMALL_STATE(34)] = 1123,
  [SMALL_STATE(35)] = 1149,
  [SMALL_STATE(36)] = 1173,
  [SMALL_STATE(37)] = 1201,
  [SMALL_STATE(38)] = 1239,
  [SMALL_STATE(39)] = 1263,
  [SMALL_STATE(40)] = 1301,
  [SMALL_STATE(41)] = 1336,
  [SMALL_STATE(42)] = 1371,
  [SMALL_STATE(43)] = 1404,
  [SMALL_STATE(44)] = 1437,
  [SMALL_STATE(45)] = 1470,
  [SMALL_STATE(46)] = 1496,
  [SMALL_STATE(47)] = 1525,
  [SMALL_STATE(48)] = 1550,
  [SMALL_STATE(49)] = 1575,
  [SMALL_STATE(50)] = 1594,
  [SMALL_STATE(51)] = 1619,
  [SMALL_STATE(52)] = 1645,
  [SMALL_STATE(53)] = 1668,
  [SMALL_STATE(54)] = 1689,
  [SMALL_STATE(55)] = 1702,
  [SMALL_STATE(56)] = 1715,
  [SMALL_STATE(57)] = 1728,
  [SMALL_STATE(58)] = 1749,
  [SMALL_STATE(59)] = 1762,
  [SMALL_STATE(60)] = 1775,
  [SMALL_STATE(61)] = 1788,
  [SMALL_STATE(62)] = 1801,
  [SMALL_STATE(63)] = 1822,
  [SMALL_STATE(64)] = 1835,
  [SMALL_STATE(65)] = 1856,
  [SMALL_STATE(66)] = 1869,
  [SMALL_STATE(67)] = 1890,
  [SMALL_STATE(68)] = 1903,
  [SMALL_STATE(69)] = 1916,
  [SMALL_STATE(70)] = 1929,
  [SMALL_STATE(71)] = 1950,
  [SMALL_STATE(72)] = 1963,
  [SMALL_STATE(73)] = 1984,
  [SMALL_STATE(74)] = 1997,
  [SMALL_STATE(75)] = 2009,
  [SMALL_STATE(76)] = 2029,
  [SMALL_STATE(77)] = 2041,
  [SMALL_STATE(78)] = 2053,
  [SMALL_STATE(79)] = 2073,
  [SMALL_STATE(80)] = 2085,
  [SMALL_STATE(81)] = 2105,
  [SMALL_STATE(82)] = 2125,
  [SMALL_STATE(83)] = 2137,
  [SMALL_STATE(84)] = 2149,
  [SMALL_STATE(85)] = 2161,
  [SMALL_STATE(86)] = 2173,
  [SMALL_STATE(87)] = 2193,
  [SMALL_STATE(88)] = 2213,
  [SMALL_STATE(89)] = 2233,
  [SMALL_STATE(90)] = 2253,
  [SMALL_STATE(91)] = 2273,
  [SMALL_STATE(92)] = 2293,
  [SMALL_STATE(93)] = 2313,
  [SMALL_STATE(94)] = 2333,
  [SMALL_STATE(95)] = 2353,
  [SMALL_STATE(96)] = 2373,
  [SMALL_STATE(97)] = 2385,
  [SMALL_STATE(98)] = 2405,
  [SMALL_STATE(99)] = 2417,
  [SMALL_STATE(100)] = 2429,
  [SMALL_STATE(101)] = 2441,
  [SMALL_STATE(102)] = 2453,
  [SMALL_STATE(103)] = 2470,
  [SMALL_STATE(104)] = 2487,
  [SMALL_STATE(105)] = 2504,
  [SMALL_STATE(106)] = 2519,
  [SMALL_STATE(107)] = 2530,
  [SMALL_STATE(108)] = 2545,
  [SMALL_STATE(109)] = 2560,
  [SMALL_STATE(110)] = 2576,
  [SMALL_STATE(111)] = 2586,
  [SMALL_STATE(112)] = 2602,
  [SMALL_STATE(113)] = 2618,
  [SMALL_STATE(114)] = 2630,
  [SMALL_STATE(115)] = 2640,
  [SMALL_STATE(116)] = 2650,
  [SMALL_STATE(117)] = 2660,
  [SMALL_STATE(118)] = 2670,
  [SMALL_STATE(119)] = 2679,
  [SMALL_STATE(120)] = 2688,
  [SMALL_STATE(121)] = 2701,
  [SMALL_STATE(122)] = 2714,
  [SMALL_STATE(123)] = 2723,
  [SMALL_STATE(124)] = 2732,
  [SMALL_STATE(125)] = 2742,
  [SMALL_STATE(126)] = 2752,
  [SMALL_STATE(127)] = 2762,
  [SMALL_STATE(128)] = 2772,
  [SMALL_STATE(129)] = 2782,
  [SMALL_STATE(130)] = 2792,
  [SMALL_STATE(131)] = 2802,
  [SMALL_STATE(132)] = 2810,
  [SMALL_STATE(133)] = 2820,
  [SMALL_STATE(134)] = 2830,
  [SMALL_STATE(135)] = 2840,
  [SMALL_STATE(136)] = 2850,
  [SMALL_STATE(137)] = 2857,
  [SMALL_STATE(138)] = 2864,
  [SMALL_STATE(139)] = 2871,
  [SMALL_STATE(140)] = 2878,
  [SMALL_STATE(141)] = 2885,
  [SMALL_STATE(142)] = 2892,
  [SMALL_STATE(143)] = 2899,
  [SMALL_STATE(144)] = 2906,
  [SMALL_STATE(145)] = 2913,
  [SMALL_STATE(146)] = 2920,
  [SMALL_STATE(147)] = 2927,
  [SMALL_STATE(148)] = 2934,
  [SMALL_STATE(149)] = 2941,
  [SMALL_STATE(150)] = 2948,
  [SMALL_STATE(151)] = 2955,
  [SMALL_STATE(152)] = 2962,
  [SMALL_STATE(153)] = 2969,
  [SMALL_STATE(154)] = 2976,
  [SMALL_STATE(155)] = 2983,
  [SMALL_STATE(156)] = 2990,
  [SMALL_STATE(157)] = 2997,
  [SMALL_STATE(158)] = 3004,
  [SMALL_STATE(159)] = 3011,
  [SMALL_STATE(160)] = 3018,
  [SMALL_STATE(161)] = 3025,
  [SMALL_STATE(162)] = 3032,
  [SMALL_STATE(163)] = 3039,
  [SMALL_STATE(164)] = 3046,
  [SMALL_STATE(165)] = 3053,
  [SMALL_STATE(166)] = 3060,
  [SMALL_STATE(167)] = 3067,
  [SMALL_STATE(168)] = 3074,
};

static const TSParseActionEntry ts_parse_actions[] = {
  [0] = {.entry = {.count = 0, .reusable = false}},
  [1] = {.entry = {.count = 1, .reusable = false}}, RECOVER(),
  [3] = {.entry = {.count = 1, .reusable = true}}, SHIFT_EXTRA(),
  [5] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_source_file, 0, 0, 0),
  [7] = {.entry = {.count = 1, .reusable = true}}, SHIFT(135),
  [9] = {.entry = {.count = 1, .reusable = true}}, SHIFT(166),
  [11] = {.entry = {.count = 1, .reusable = true}}, SHIFT(161),
  [13] = {.entry = {.count = 1, .reusable = true}}, SHIFT(157),
  [15] = {.entry = {.count = 1, .reusable = true}}, SHIFT(156),
  [17] = {.entry = {.count = 1, .reusable = true}}, SHIFT(151),
  [19] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_probability, 1, 0, 0),
  [21] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_probability, 1, 0, 0),
  [23] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_string, 2, 0, 0),
  [25] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_string, 3, 0, 0),
  [27] = {.entry = {.count = 1, .reusable = true}}, SHIFT(28),
  [29] = {.entry = {.count = 1, .reusable = true}}, SHIFT(18),
  [31] = {.entry = {.count = 1, .reusable = true}}, SHIFT(29),
  [33] = {.entry = {.count = 1, .reusable = false}}, SHIFT(30),
  [35] = {.entry = {.count = 1, .reusable = false}}, SHIFT(6),
  [37] = {.entry = {.count = 1, .reusable = false}}, SHIFT(32),
  [39] = {.entry = {.count = 1, .reusable = true}}, SHIFT(2),
  [41] = {.entry = {.count = 1, .reusable = false}}, SHIFT(2),
  [43] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_expression, 1, 0, 0),
  [45] = {.entry = {.count = 1, .reusable = true}}, SHIFT(5),
  [47] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_expression, 1, 0, 0),
  [49] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_function_call, 5, 0, 15),
  [51] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_function_call, 5, 0, 15),
  [53] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_parenthesized_expression, 3, 0, 0),
  [55] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_parenthesized_expression, 3, 0, 0),
  [57] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_conditional_expression, 6, 0, 18),
  [59] = {.entry = {.count = 1, .reusable = true}}, SHIFT(26),
  [61] = {.entry = {.count = 1, .reusable = false}}, SHIFT(26),
  [63] = {.entry = {.count = 1, .reusable = true}}, SHIFT(25),
  [65] = {.entry = {.count = 1, .reusable = true}}, SHIFT(22),
  [67] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_function_call, 3, 0, 9),
  [69] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_function_call, 3, 0, 9),
  [71] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_binary_expression, 3, 0, 10),
  [73] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_binary_expression, 3, 0, 10),
  [75] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_unary_expression, 2, 0, 7),
  [77] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_unary_expression, 2, 0, 7),
  [79] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_function_call, 4, 0, 12),
  [81] = {.entry = {.count = 1, .reusable = false}}, REDUCE(sym_function_call, 4, 0, 12),
  [83] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_source_file, 1, 0, 0),
  [85] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2, 0, 0),
  [87] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2, 0, 0), SHIFT_REPEAT(135),
  [90] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2, 0, 0), SHIFT_REPEAT(166),
  [93] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2, 0, 0), SHIFT_REPEAT(161),
  [96] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2, 0, 0), SHIFT_REPEAT(157),
  [99] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2, 0, 0), SHIFT_REPEAT(156),
  [102] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_source_file_repeat1, 2, 0, 0), SHIFT_REPEAT(151),
  [105] = {.entry = {.count = 1, .reusable = true}}, SHIFT(67),
  [107] = {.entry = {.count = 1, .reusable = true}}, SHIFT(155),
  [109] = {.entry = {.count = 1, .reusable = true}}, SHIFT(158),
  [111] = {.entry = {.count = 1, .reusable = true}}, SHIFT(159),
  [113] = {.entry = {.count = 1, .reusable = true}}, SHIFT(141),
  [115] = {.entry = {.count = 1, .reusable = true}}, SHIFT(146),
  [117] = {.entry = {.count = 1, .reusable = true}}, SHIFT(54),
  [119] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_driver_block_repeat1, 2, 0, 0),
  [121] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_driver_block_repeat1, 2, 0, 0), SHIFT_REPEAT(155),
  [124] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_driver_block_repeat1, 2, 0, 0), SHIFT_REPEAT(158),
  [127] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_driver_block_repeat1, 2, 0, 0), SHIFT_REPEAT(159),
  [130] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_driver_block_repeat1, 2, 0, 0), SHIFT_REPEAT(141),
  [133] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_driver_block_repeat1, 2, 0, 0), SHIFT_REPEAT(146),
  [136] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_model_statement, 3, 0, 4),
  [138] = {.entry = {.count = 1, .reusable = true}}, SHIFT(153),
  [140] = {.entry = {.count = 1, .reusable = true}}, SHIFT(136),
  [142] = {.entry = {.count = 1, .reusable = true}}, SHIFT(149),
  [144] = {.entry = {.count = 1, .reusable = true}}, SHIFT(148),
  [146] = {.entry = {.count = 1, .reusable = true}}, SHIFT(147),
  [148] = {.entry = {.count = 1, .reusable = true}}, SHIFT(116),
  [150] = {.entry = {.count = 1, .reusable = true}}, SHIFT(145),
  [152] = {.entry = {.count = 1, .reusable = true}}, SHIFT(144),
  [154] = {.entry = {.count = 1, .reusable = true}}, SHIFT(143),
  [156] = {.entry = {.count = 1, .reusable = true}}, SHIFT(142),
  [158] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_base_rate_block_repeat1, 2, 0, 0),
  [160] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_base_rate_block_repeat1, 2, 0, 0), SHIFT_REPEAT(145),
  [163] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_base_rate_block_repeat1, 2, 0, 0), SHIFT_REPEAT(144),
  [166] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_base_rate_block_repeat1, 2, 0, 0), SHIFT_REPEAT(143),
  [169] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_base_rate_block_repeat1, 2, 0, 0), SHIFT_REPEAT(142),
  [172] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_question_statement, 2, 0, 1),
  [174] = {.entry = {.count = 1, .reusable = true}}, SHIFT(62),
  [176] = {.entry = {.count = 1, .reusable = true}}, SHIFT(110),
  [178] = {.entry = {.count = 1, .reusable = true}}, SHIFT(37),
  [180] = {.entry = {.count = 1, .reusable = true}}, SHIFT(38),
  [182] = {.entry = {.count = 1, .reusable = true}}, SHIFT(27),
  [184] = {.entry = {.count = 1, .reusable = true}}, SHIFT(82),
  [186] = {.entry = {.count = 1, .reusable = true}}, SHIFT(71),
  [188] = {.entry = {.count = 1, .reusable = true}}, SHIFT(154),
  [190] = {.entry = {.count = 1, .reusable = true}}, SHIFT(162),
  [192] = {.entry = {.count = 1, .reusable = true}}, SHIFT(164),
  [194] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_driver_block, 3, 0, 0),
  [196] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_question_block, 2, 0, 0),
  [198] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_simulate_statement, 3, 0, 5),
  [200] = {.entry = {.count = 1, .reusable = true}}, SHIFT(68),
  [202] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_agent_statement, 3, 0, 3),
  [204] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_base_rate_field, 3, 0, 11),
  [206] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_evidence_statement, 3, 0, 3),
  [208] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_question_statement, 3, 0, 2),
  [210] = {.entry = {.count = 1, .reusable = true}}, SHIFT(55),
  [212] = {.entry = {.count = 1, .reusable = true}}, SHIFT(140),
  [214] = {.entry = {.count = 1, .reusable = true}}, SHIFT(134),
  [216] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_agent_block, 2, 0, 0),
  [218] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_function_call_repeat1, 2, 0, 14),
  [220] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_question_block, 3, 0, 0),
  [222] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_question_block_repeat1, 2, 0, 0),
  [224] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_question_block_repeat1, 2, 0, 0), SHIFT_REPEAT(140),
  [227] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_question_block_repeat1, 2, 0, 0), SHIFT_REPEAT(134),
  [230] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_driver_block, 2, 0, 0),
  [232] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_evidence_block, 2, 0, 0),
  [234] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_driver_statement, 4, 0, 6),
  [236] = {.entry = {.count = 1, .reusable = true}}, SHIFT(65),
  [238] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_evidence_block, 3, 0, 0),
  [240] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_evidence_block_repeat1, 2, 0, 0),
  [242] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_evidence_block_repeat1, 2, 0, 0), SHIFT_REPEAT(154),
  [245] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_evidence_block_repeat1, 2, 0, 0), SHIFT_REPEAT(162),
  [248] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_evidence_block_repeat1, 2, 0, 0), SHIFT_REPEAT(164),
  [251] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_agent_block, 3, 0, 0),
  [253] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_unit_property, 3, 0, 11),
  [255] = {.entry = {.count = 1, .reusable = true}}, SHIFT(24),
  [257] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_beta_distribution, 10, 0, 24),
  [259] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_driver_property, 1, 0, 0),
  [261] = {.entry = {.count = 1, .reusable = true}}, SHIFT(76),
  [263] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_triangular_distribution, 8, 0, 23),
  [265] = {.entry = {.count = 1, .reusable = true}}, SHIFT(31),
  [267] = {.entry = {.count = 1, .reusable = true}}, SHIFT(79),
  [269] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_beta_distribution, 6, 0, 22),
  [271] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_uniform_distribution, 6, 0, 21),
  [273] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_lognormal_distribution, 6, 0, 20),
  [275] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_normal_distribution, 6, 0, 19),
  [277] = {.entry = {.count = 1, .reusable = true}}, SHIFT(83),
  [279] = {.entry = {.count = 1, .reusable = true}}, SHIFT(84),
  [281] = {.entry = {.count = 1, .reusable = true}}, SHIFT(85),
  [283] = {.entry = {.count = 1, .reusable = true}}, SHIFT(8),
  [285] = {.entry = {.count = 1, .reusable = true}}, SHIFT(12),
  [287] = {.entry = {.count = 1, .reusable = true}}, SHIFT(33),
  [289] = {.entry = {.count = 1, .reusable = true}}, SHIFT(39),
  [291] = {.entry = {.count = 1, .reusable = true}}, SHIFT(9),
  [293] = {.entry = {.count = 1, .reusable = true}}, SHIFT(11),
  [295] = {.entry = {.count = 1, .reusable = true}}, SHIFT(13),
  [297] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_impact_multiplier_property, 3, 0, 11),
  [299] = {.entry = {.count = 1, .reusable = true}}, SHIFT(10),
  [301] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_rationale_property, 3, 0, 11),
  [303] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_probability_property, 3, 0, 11),
  [305] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_distribution, 1, 0, 0),
  [307] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_distribution_property, 3, 0, 13),
  [309] = {.entry = {.count = 1, .reusable = true}}, SHIFT(73),
  [311] = {.entry = {.count = 1, .reusable = true}}, SHIFT(168),
  [313] = {.entry = {.count = 1, .reusable = true}}, SHIFT(137),
  [315] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_agent_block_repeat1, 2, 0, 0),
  [317] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_agent_block_repeat1, 2, 0, 0), SHIFT_REPEAT(168),
  [320] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_agent_block_repeat1, 2, 0, 0), SHIFT_REPEAT(137),
  [323] = {.entry = {.count = 1, .reusable = true}}, SHIFT(63),
  [325] = {.entry = {.count = 1, .reusable = false}}, SHIFT(106),
  [327] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_evidence_property, 3, 0, 11),
  [329] = {.entry = {.count = 1, .reusable = false}}, SHIFT(99),
  [331] = {.entry = {.count = 1, .reusable = false}}, SHIFT(59),
  [333] = {.entry = {.count = 1, .reusable = false}}, SHIFT(3),
  [335] = {.entry = {.count = 1, .reusable = false}}, SHIFT(112),
  [337] = {.entry = {.count = 1, .reusable = false}}, SHIFT(138),
  [339] = {.entry = {.count = 1, .reusable = false}}, SHIFT_EXTRA(),
  [341] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_base_rate_block, 2, 0, 0),
  [343] = {.entry = {.count = 1, .reusable = false}}, REDUCE(aux_sym_string_repeat1, 2, 0, 0),
  [345] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_string_repeat1, 2, 0, 0), SHIFT_REPEAT(111),
  [348] = {.entry = {.count = 2, .reusable = false}}, REDUCE(aux_sym_string_repeat1, 2, 0, 0), SHIFT_REPEAT(138),
  [351] = {.entry = {.count = 1, .reusable = false}}, SHIFT(4),
  [353] = {.entry = {.count = 1, .reusable = false}}, SHIFT(111),
  [355] = {.entry = {.count = 1, .reusable = false}}, SHIFT(119),
  [357] = {.entry = {.count = 1, .reusable = true}}, SHIFT(119),
  [359] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_base_rate_property, 2, 0, 8),
  [361] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_question_property, 1, 0, 0),
  [363] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_base_rate_block, 3, 0, 0),
  [365] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_question_property, 3, 0, 11),
  [367] = {.entry = {.count = 1, .reusable = true}}, SHIFT(125),
  [369] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_agent_property, 5, 0, 17),
  [371] = {.entry = {.count = 1, .reusable = true}}, SHIFT(7),
  [373] = {.entry = {.count = 2, .reusable = true}}, REDUCE(aux_sym_function_call_repeat1, 2, 0, 16), SHIFT_REPEAT(37),
  [376] = {.entry = {.count = 1, .reusable = true}}, REDUCE(aux_sym_function_call_repeat1, 2, 0, 16),
  [378] = {.entry = {.count = 1, .reusable = true}}, REDUCE(sym_agent_property, 3, 0, 11),
  [380] = {.entry = {.count = 1, .reusable = true}}, SHIFT(109),
  [382] = {.entry = {.count = 1, .reusable = true}}, SHIFT(42),
  [384] = {.entry = {.count = 1, .reusable = true}}, SHIFT(57),
  [386] = {.entry = {.count = 1, .reusable = true}}, SHIFT(104),
  [388] = {.entry = {.count = 1, .reusable = true}}, SHIFT(50),
  [390] = {.entry = {.count = 1, .reusable = true}}, SHIFT(20),
  [392] = {.entry = {.count = 1, .reusable = true}}, SHIFT(163),
  [394] = {.entry = {.count = 1, .reusable = false}}, SHIFT(122),
  [396] = {.entry = {.count = 1, .reusable = true}}, SHIFT(56),
  [398] = {.entry = {.count = 1, .reusable = true}}, SHIFT(124),
  [400] = {.entry = {.count = 1, .reusable = true}}, SHIFT(129),
  [402] = {.entry = {.count = 1, .reusable = true}}, SHIFT(131),
  [404] = {.entry = {.count = 1, .reusable = true}}, SHIFT(160),
  [406] = {.entry = {.count = 1, .reusable = true}}, SHIFT(108),
  [408] = {.entry = {.count = 1, .reusable = true}}, SHIFT(133),
  [410] = {.entry = {.count = 1, .reusable = true}}, SHIFT(152),
  [412] = {.entry = {.count = 1, .reusable = true}}, SHIFT(15),
  [414] = {.entry = {.count = 1, .reusable = true}}, SHIFT(17),
  [416] = {.entry = {.count = 1, .reusable = true}}, SHIFT(19),
  [418] = {.entry = {.count = 1, .reusable = true}},  ACCEPT_INPUT(),
  [420] = {.entry = {.count = 1, .reusable = true}}, SHIFT(139),
  [422] = {.entry = {.count = 1, .reusable = true}}, SHIFT(96),
  [424] = {.entry = {.count = 1, .reusable = true}}, SHIFT(23),
  [426] = {.entry = {.count = 1, .reusable = true}}, SHIFT(132),
  [428] = {.entry = {.count = 1, .reusable = true}}, SHIFT(46),
  [430] = {.entry = {.count = 1, .reusable = true}}, SHIFT(16),
  [432] = {.entry = {.count = 1, .reusable = true}}, SHIFT(128),
  [434] = {.entry = {.count = 1, .reusable = true}}, SHIFT(107),
  [436] = {.entry = {.count = 1, .reusable = true}}, SHIFT(130),
  [438] = {.entry = {.count = 1, .reusable = true}}, SHIFT(59),
  [440] = {.entry = {.count = 1, .reusable = true}}, SHIFT(126),
  [442] = {.entry = {.count = 1, .reusable = true}}, SHIFT(105),
  [444] = {.entry = {.count = 1, .reusable = true}}, SHIFT(167),
  [446] = {.entry = {.count = 1, .reusable = true}}, SHIFT(165),
  [448] = {.entry = {.count = 1, .reusable = true}}, SHIFT(106),
  [450] = {.entry = {.count = 1, .reusable = true}}, SHIFT(118),
  [452] = {.entry = {.count = 1, .reusable = true}}, SHIFT(113),
  [454] = {.entry = {.count = 1, .reusable = true}}, SHIFT(127),
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
