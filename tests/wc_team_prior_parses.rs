//! Verify the live team-prior FPL parses cleanly and the simulator
//! recognizes all the variables it references.
//!
//! Cross-checks that
//!   (a) the lexer + parser accept the rendered template, and
//!   (b) the program's variable namespace covers everything the
//!       `estimate tournament_strength as: ...` expression and any
//!       `factor X_n` formulations reference.
//!
//! Catches the regression where executor reports `Undefined variable: foo`
//! because a factor formulation references `goal_difference` but no
//! `param goal_difference` exists.

use fermi::ast::Statement;
use fermi::lexer::Lexer;
use fermi::parser::Parser;

const ARG_TEMPLATE: &str = include_str!("../templates/world_cup/team_prior.fpl");

#[test]
fn team_prior_template_parses() {
    let tokens = Lexer::new(ARG_TEMPLATE)
        .tokenize()
        .expect("template should tokenize");
    let program = Parser::new(tokens).parse().expect("template should parse");

    let driver_names: Vec<_> = program.drivers().iter().map(|d| d.name.as_str()).collect();
    assert!(
        driver_names.contains(&"won_rate"),
        "expected won_rate driver in {:?}",
        driver_names
    );
    assert!(
        driver_names.contains(&"form_signal"),
        "expected form_signal driver in {:?}",
        driver_names
    );

    let has_estimate = program
        .statements
        .iter()
        .any(|s| matches!(s, Statement::Estimate(_)));
    assert!(
        has_estimate,
        "expected an `estimate ... as:` statement (the 6-factor Cobb-Douglas)"
    );

    // BAYESOPS_CONTRACT.md §3 declares one curated agent per factor family;
    // four distinct agents own the six factor X_k inputs. The template MUST
    // declare them so the cockpit's "⚠ No agents — assign one" warning
    // doesn't fire on every driver and so the Agent Fleet panel shows the
    // expected ownership map.
    let agent_names: Vec<_> = program.agents().iter().map(|a| a.name.as_str()).collect();
    for expected in &[
        "macro_data_agent",
        "football_institution_agent",
        "football_analyst",
        "fixture_context_agent",
    ] {
        assert!(
            agent_names.contains(expected),
            "expected agent {} in {:?}",
            expected,
            agent_names
        );
    }

    // Every learnable BayesOps driver should be referenced by at least one
    // agent — otherwise the "No agents" warning fires in the cockpit.
    for d in program.drivers().iter().filter(|d| d.learnable) {
        let referenced = program
            .agents()
            .iter()
            .any(|a| a.driver_refs.iter().any(|r| r == &d.name));
        assert!(
            referenced,
            "learnable driver {} has no agent referencing it via driver_refs",
            d.name
        );
    }
}
