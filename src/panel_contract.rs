//! One stamp, three densities: what a panel says on a desk, a phone and a
//! waveguide.
//!
//! # The split
//!
//! `glasses/hud_field_scout/README.md` states the architecture this module
//! generalises:
//!
//! > **It decides nothing.** Every marker, provenance tag and confidence band
//! > is computed by `src/hud_contract.rs` server-side and arrives already
//! > stamped. The shell copies them to the screen. That split is the point.
//! > […] The glasses are I/O.
//!
//! A renderer that decides anything is a renderer that can disagree with the
//! platform, and the one that disagrees is whichever is nearest the screen. So
//! the server produces a [`Stamp`] and every surface copies it.
//!
//! # Why this is not inside `hud_contract`
//!
//! [`crate::hud_contract`] answers *"can the wearer see which answer is
//! which"* — provenance made visible as typography. That is one axis. Panel
//! **kind** and **density** are another, and welding them together would give
//! the platform two vocabularies for the provenance question, of which the
//! unchecked one is where a fabrication moves. So this module owns the density
//! ladder and **delegates every treatment decision** to `hud_contract`.
//!
//! It also gives that module its first production caller. `hud_contract` is
//! ~1,000 lines of tested display gate that the audit found reachable only
//! from an example binary; `DESIGN_gates_as_a_service.md` §1 notes that a gate
//! whose first user is a paying stranger has not been operated. Operating it
//! here, where we can watch it, is the right order.
//!
//! # Degradation must be one-directional
//!
//! `hud_contract::Treatment::marker` documents the load-bearing rule:
//!
//! > `Verified` gets no marker: the unmarked case must be the trustworthy one,
//! > so that a marker always means "read this more carefully" and a renderer
//! > that drops markers degrades to *less* confident rather than more.
//!
//! Densities are the same rule across a second dimension. [`Density::Glance`]
//! throws away almost everything, and what survives must never imply more
//! confidence than [`Density::Study`] would. `glance_never_reads_safer_than_study`
//! is that as an assertion.
//!
//! # The distinguishability requirement
//!
//! `panel_absence` separates *nothing happened* from *something is broken* from
//! *nobody can say*. Two words on a waveguide is where that distinction is
//! most likely to collapse, and a glance tier that collapses it has undone the
//! work. `the_three_readings_stay_distinct_at_every_density` pins it.

use crate::hud_contract::{Treatment, LINE_MAX, MAX_LINES};
use crate::panel_absence::{Absence, Kind, Panel, Reading, Scope};

/// How much room the surface has.
///
/// Not breakpoints. A density is a *budget*, and the server stamps for all
/// three so a shell never has to decide what to drop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Density {
    /// One glyph, one phrase, one line. Waveguide, AR world-label, watch.
    ///
    /// The budget is `hud_contract`'s, measured against real optics: 60
    /// characters at 15px on a 480px panel.
    Glance,
    /// A row: marker, subject, reason. Phone, register rows.
    Scan,
    /// Everything, with provenance. Desktop.
    Study,
}

impl Density {
    /// Lines this density may emit.
    pub fn max_lines(self) -> usize {
        match self {
            Density::Glance => 1,
            Density::Scan => 2,
            Density::Study => MAX_LINES,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Density::Glance => "glance",
            Density::Scan => "scan",
            Density::Study => "study",
        }
    }
}

/// Every density, for callers that stamp all three.
pub const DENSITIES: &[Density] = &[Density::Glance, Density::Scan, Density::Study];

/// What a renderer copies to the screen.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Stamp {
    pub panel: &'static str,
    pub kind: Kind,
    pub scope: Scope,
    pub density: Density,
    /// The leading marker, from [`crate::hud_contract`]. ASCII.
    pub marker: &'static str,
    /// The marker as a word, for TTS and for renderers that cannot show glyphs.
    pub marker_word: &'static str,
    pub reading: Reading,
    /// The answering contract's own token, carried at every density.
    pub token: &'static str,
    /// Ready to draw. Never empty; each line is within [`LINE_MAX`].
    pub lines: Vec<String>,
}

/// The [`Treatment`] an absence is shown with.
///
/// Reusing `hud_contract`'s vocabulary rather than minting a second set of
/// glyphs, because a screen showing two glyph languages teaches neither. The
/// mapping is close to exact:
///
/// | reading | treatment | why |
/// |---|---|---|
/// | [`Reading::Idle`] | [`Treatment::NoMatch`] | *"consulted, and had nothing for this subject"* — the same claim |
/// | [`Reading::Fault`] | [`Treatment::Rejected`] | *"checked, and found wrong"* — something should have been here |
/// | [`Reading::Unknown`] | [`Treatment::Unavailable`] | *"nothing could supply it"* — no contract can say |
///
/// [`Treatment::Verified`] is deliberately unreachable from an absence. It is
/// the unmarked, trustworthy case, and an empty panel is never that.
pub fn treatment_for(reading: Reading) -> Treatment {
    match reading {
        Reading::Idle => Treatment::NoMatch,
        Reading::Fault => Treatment::Rejected,
        Reading::Unknown => Treatment::Unavailable,
    }
}

/// The short name of a panel, for a surface with no room for the full id.
fn short(panel_id: &str) -> &str {
    panel_id.rsplit('.').next().unwrap_or(panel_id)
}

/// Cut to a character budget on a char boundary, marking the cut.
///
/// Truncation is visible on purpose: a sentence silently losing its qualifying
/// clause is how a hedged claim becomes a flat one at the smallest density.
fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let keep = max.saturating_sub(1);
    let mut out: String = s.chars().take(keep).collect();
    out.push('…');
    out
}

/// Stamp one resolved absence at one density.
pub fn stamp_absence(p: &Panel, a: &Absence, density: Density) -> Stamp {
    let t = treatment_for(a.reading);
    let marker = t.marker();

    let lines = match density {
        // One line. The subject and the token, because the token is the part a
        // returning reader recognises and it is already short by construction.
        Density::Glance => {
            vec![clip(
                &format!("{marker}{}: {}", short(p.id), a.token),
                LINE_MAX,
            )]
        }
        // Two: what it is, and the first clause of why.
        Density::Scan => vec![
            clip(&format!("{marker}{}", p.shows), LINE_MAX),
            clip(&first_clause(&a.detail), LINE_MAX),
        ],
        // Everything, plus where the answer came from. The provenance line is
        // the `← source` habit from the Observatory, which is the surface this
        // whole design is trying to spread rather than replace.
        Density::Study => {
            let mut v = vec![clip(&format!("{marker}{}", p.shows), LINE_MAX)];
            for chunk in wrap(&a.detail, LINE_MAX, MAX_LINES - 2) {
                v.push(chunk);
            }
            v.push(clip(
                &match a.rung {
                    Some(r) => format!("← {} (ladder rung {r})", a.answered_by),
                    None => format!("← {}", a.answered_by),
                },
                LINE_MAX,
            ));
            if let Some(r) = a.remediation {
                v.push(clip(&format!("→ {r}"), LINE_MAX));
            }
            v.truncate(MAX_LINES);
            v
        }
    };

    Stamp {
        panel: p.id,
        kind: p.kind,
        scope: p.scope,
        density,
        marker,
        marker_word: t.word(),
        reading: a.reading,
        token: a.token,
        lines,
    }
}

/// Stamp all three densities at once.
///
/// The shape a wire response takes: the server decides everything, and each
/// surface picks its tier without ever choosing what to discard.
pub fn stamp_all(p: &Panel, a: &Absence) -> Vec<Stamp> {
    DENSITIES.iter().map(|d| stamp_absence(p, a, *d)).collect()
}

/// The first sentence, or the whole thing if it is one sentence.
fn first_clause(s: &str) -> String {
    match s.find(". ") {
        Some(i) => s[..=i].trim().to_string(),
        None => s.to_string(),
    }
}

/// Greedy wrap to `max` characters, at most `lines` lines, last line clipped.
fn wrap(s: &str, max: usize, lines: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        let candidate = if cur.is_empty() {
            word.to_string()
        } else {
            format!("{cur} {word}")
        };
        if candidate.chars().count() > max {
            if out.len() + 1 == lines {
                // Last line: say that it was cut rather than ending mid-clause.
                out.push(clip(&format!("{cur} {word}"), max));
                return out;
            }
            out.push(std::mem::take(&mut cur));
            cur = word.to_string();
        } else {
            cur = candidate;
        }
    }
    if !cur.is_empty() && out.len() < lines {
        out.push(cur);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_evaluators::Observation;
    use crate::panel_absence::{resolve, PANELS};

    /// Resolve every panel against an empty snapshot, so the fixtures are the
    /// real ones rather than hand-written absences.
    fn every_absence() -> Vec<(&'static Panel, Absence)> {
        let o = Observation::default();
        PANELS.iter().map(|p| (p, resolve(p, &o))).collect()
    }

    #[test]
    fn every_stamp_is_within_its_budget_and_never_empty() {
        for (p, a) in every_absence() {
            for d in DENSITIES {
                let s = stamp_absence(p, &a, *d);
                assert!(
                    !s.lines.is_empty(),
                    "{} at {}: a panel that renders nothing has said nothing",
                    p.id,
                    d.as_str()
                );
                assert!(
                    s.lines.len() <= d.max_lines(),
                    "{} at {}: {} lines, budget {}",
                    p.id,
                    d.as_str(),
                    s.lines.len(),
                    d.max_lines()
                );
                for l in &s.lines {
                    assert!(
                        l.chars().count() <= LINE_MAX,
                        "{} at {}: {} chars > {LINE_MAX}: {l:?}",
                        p.id,
                        d.as_str(),
                        l.chars().count()
                    );
                }
            }
        }
    }

    /// The glance tier must fit the optics it was measured against.
    #[test]
    fn glance_is_one_line_and_says_which_panel_it_is() {
        for (p, a) in every_absence() {
            let s = stamp_absence(p, &a, Density::Glance);
            assert_eq!(s.lines.len(), 1, "{}", p.id);
            let line = &s.lines[0];
            assert!(
                line.contains(short(p.id)),
                "{}: a glance line nobody can attribute is noise: {line:?}",
                p.id
            );
        }
    }

    /// Three readings, three markers, at every density.
    ///
    /// The distinction `panel_absence` exists to make is most likely to
    /// collapse where there is least room, which is exactly where collapsing it
    /// does most damage.
    #[test]
    fn the_three_readings_stay_distinct_at_every_density() {
        let markers: Vec<&str> = [Reading::Idle, Reading::Fault, Reading::Unknown]
            .iter()
            .map(|r| treatment_for(*r).marker())
            .collect();
        let unique: std::collections::HashSet<_> = markers.iter().collect();
        assert_eq!(
            unique.len(),
            3,
            "two readings share a marker, so the surface cannot tell them apart: {markers:?}"
        );

        let words: std::collections::HashSet<&str> =
            [Reading::Idle, Reading::Fault, Reading::Unknown]
                .iter()
                .map(|r| treatment_for(*r).word())
                .collect();
        assert_eq!(
            words.len(),
            3,
            "and the TTS fallback must distinguish them too"
        );
    }

    /// An absence may never render as the trustworthy unmarked case.
    ///
    /// `Verified` carries no marker by design. If an absence could reach it, an
    /// empty panel would be indistinguishable from a sourced value — which is
    /// the presentation-layer version of the `genome_profiler` incident that
    /// `hud_contract` was written for.
    #[test]
    fn an_absence_is_never_unmarked() {
        for r in [Reading::Idle, Reading::Fault, Reading::Unknown] {
            let t = treatment_for(r);
            assert_ne!(t, Treatment::Verified, "{r:?} reached the unmarked case");
            assert!(!t.marker().is_empty(), "{r:?} has no marker");
        }
        for (p, a) in every_absence() {
            for d in DENSITIES {
                let s = stamp_absence(p, &a, *d);
                assert!(
                    !s.marker.is_empty(),
                    "{} at {}: unmarked absence",
                    p.id,
                    d.as_str()
                );
            }
        }
    }

    /// Dropping detail must never buy confidence.
    ///
    /// The one-directional rule from `Treatment::marker`, applied to the
    /// density ladder: whatever `Study` says is wrong, `Glance` must still say
    /// is wrong. A shell that shows less must not show better.
    #[test]
    fn glance_never_reads_safer_than_study() {
        for (p, a) in every_absence() {
            let g = stamp_absence(p, &a, Density::Glance);
            let s = stamp_absence(p, &a, Density::Study);

            assert_eq!(
                g.reading, s.reading,
                "{}: the reading changed with the density",
                p.id
            );
            assert_eq!(
                g.marker, s.marker,
                "{}: the marker changed with the density",
                p.id
            );
            assert_eq!(g.token, s.token, "{}: the token changed", p.id);

            if s.reading == Reading::Fault {
                assert_eq!(
                    g.marker,
                    Treatment::Rejected.marker(),
                    "{}: a fault softened at glance",
                    p.id
                );
            }
        }
    }

    /// Truncation must be visible.
    #[test]
    fn a_clipped_line_says_it_was_clipped() {
        let long = "a".repeat(LINE_MAX + 20);
        let c = clip(&long, LINE_MAX);
        assert_eq!(c.chars().count(), LINE_MAX);
        assert!(
            c.ends_with('…'),
            "a sentence that silently loses its qualifying clause reads as a \
             flatter claim than it is"
        );
        // And a line already inside the budget is untouched.
        assert_eq!(clip("short", LINE_MAX), "short");
    }

    /// Wrapping must not lose the beginning of the sentence.
    #[test]
    fn wrap_keeps_the_opening_and_bounds_the_rest() {
        let s = "The quick brown fox jumps over the lazy dog and keeps going well \
                 past the end of any single line budget we might set for it here.";
        let w = wrap(s, 40, 3);
        assert!(w.len() <= 3);
        assert!(w[0].starts_with("The quick"));
        for l in &w {
            assert!(l.chars().count() <= 40, "{l:?}");
        }
    }

    /// `stamp_all` is what a wire response carries.
    #[test]
    fn stamp_all_covers_every_density_once() {
        let (p, a) = every_absence().into_iter().next().expect("panels exist");
        let all = stamp_all(p, &a);
        assert_eq!(all.len(), DENSITIES.len());
        let ds: std::collections::HashSet<_> = all.iter().map(|s| s.density).collect();
        assert_eq!(ds.len(), DENSITIES.len(), "a density was stamped twice");
    }
}
