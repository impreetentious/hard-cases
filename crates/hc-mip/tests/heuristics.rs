//! A heuristic changes how fast an answer arrives and not what the answer is.
//!
//! Rounding a relaxation's point is a guess, and so is diving from it, and so
//! is pumping. Each is allowed to be wrong as often as it likes, because
//! nothing any of them produces is believed: the candidate goes through the
//! same exact screening a node's own integral vertex goes through, and a
//! refused candidate leaves no trace on the search beyond the record that it
//! was tried. So the search with heuristics on and the search with them off
//! must agree on the status and on the objective, every time.
//!
//! The misses are checked too. A run that filed only its successes would make
//! the solver look luckier than it is, and the honesty rule that says otherwise
//! is only real if something asserts it.
//!
//! The ladder is checked as well. Rounding runs at every node about to split,
//! and the two expensive heuristics run only while the search has nothing to
//! prune against — so a corpus that never escalated would leave them wired to
//! nothing, which is the state this file exists to keep the search out of.

use hc_core::Xoshiro256PlusPlus;
use hc_io::mps::{Form, read};
use hc_lp::exact::Canonical;
use hc_mip::incumbent::screen;
use hc_mip::search::{Options, run};

fn without_heuristics() -> Options {
    Options {
        heuristics: false,
        ..Options::default()
    }
}

fn generate(generator: &mut Xoshiro256PlusPlus, rows: usize, columns: usize) -> String {
    use core::fmt::Write as _;
    let mut source = String::from("NAME H\nROWS\n N obj\n");
    for row in 0..rows {
        let _ = writeln!(source, " L r{row}");
    }
    source.push_str("COLUMNS\n MARKER 'MARKER' 'INTORG'\n");
    for column in 0..columns {
        let _ = writeln!(source, " c{column} obj -{}", 1 + generator.next_below(9));
        for row in 0..rows {
            let value = 1 + generator.next_below(7);
            let _ = writeln!(source, " c{column} r{row} {value}");
        }
    }
    source.push_str(" MARKER 'MARKER' 'INTEND'\nRHS\n");
    for row in 0..rows {
        let _ = writeln!(source, " rhs r{row} {}", 5 + generator.next_below(20));
    }
    source.push_str("BOUNDS\n");
    for column in 0..columns {
        let _ = writeln!(source, " UI b c{column} {}", 1 + generator.next_below(3));
    }
    source.push_str("ENDATA\n");
    source
}

#[test]
fn heuristics_change_the_search_and_not_the_answer() {
    let mut generator = Xoshiro256PlusPlus::from_seed(0x4e57_0000_0000_0011);
    let mut compared = 0;
    let mut guessed = 0;
    let mut landed = 0;

    for round in 0..400 {
        let source = generate(&mut generator, 1 + (round % 2), 2 + (round % 3));
        let model = read(source.as_bytes(), Form::Free).expect("the source parses");

        let guessing = run(&model, &Options::default());
        let plain = run(&model, &without_heuristics());

        assert_eq!(
            guessing.ending, plain.ending,
            "round {round}: a guess changed the status\n{source}"
        );
        assert_eq!(
            guessing
                .incumbent
                .as_ref()
                .map(|found| found.objective.to_canonical_string()),
            plain
                .incumbent
                .as_ref()
                .map(|found| found.objective.to_canonical_string()),
            "round {round}: a guess changed the answer\n{source}"
        );
        assert!(
            plain.heuristics.is_empty(),
            "round {round}: heuristics ran with the option off"
        );

        compared += 1;
        if !guessing.heuristics.is_empty() {
            guessed += 1;
        }
        if guessing.heuristics.iter().any(|run| run.outcome == "found") {
            landed += 1;
        }
    }

    assert_eq!(compared, 400);
    assert!(
        guessed > 100,
        "only {guessed} of 400 searches ran a heuristic at all, so this proves little"
    );
    assert!(
        landed > 0,
        "no rounded candidate was ever accepted, so the accepting half is untested"
    );
}

#[test]
fn every_run_is_filed_including_the_misses() {
    let mut generator = Xoshiro256PlusPlus::from_seed(0x4e57_0000_0000_0012);
    let mut found = 0;
    let mut missed = 0;

    for round in 0..200 {
        let source = generate(&mut generator, 1 + (round % 2), 2 + (round % 3));
        let model = read(source.as_bytes(), Form::Free).expect("the source parses");
        let search = run(&model, &Options::default());

        for attempt in &search.heuristics {
            assert!(
                matches!(attempt.rule, "round" | "dive" | "pump"),
                "an unnamed heuristic filed a run: {}",
                attempt.rule
            );
            assert!(
                search.tree.get(attempt.node).is_some(),
                "a run names node {} which the tree does not have",
                attempt.node
            );
            match attempt.outcome {
                "found" => found += 1,
                "refused" | "abandoned" => missed += 1,
                other => panic!("a run reported the unknown outcome {other:?}"),
            }
        }
    }

    assert!(found > 0, "no run was ever recorded as a hit");
    assert!(
        missed > 0,
        "no run was ever recorded as a miss, which no honest rounding heuristic achieves"
    );
}

#[test]
fn an_incumbent_a_guess_produced_still_passes_the_screening() {
    let mut generator = Xoshiro256PlusPlus::from_seed(0x4e57_0000_0000_0013);
    let mut checked = 0;

    for round in 0..200 {
        let source = generate(&mut generator, 1 + (round % 2), 2 + (round % 3));
        let model = read(source.as_bytes(), Form::Free).expect("the source parses");
        let search = run(&model, &Options::default());

        let Some(incumbent) = search.incumbent.as_ref() else {
            continue;
        };
        if !search.heuristics.iter().any(|run| run.outcome == "found") {
            continue;
        }

        let canonical = Canonical::of(&model);
        let rescreened = screen(&canonical, &incumbent.point).unwrap_or_else(|reason| {
            panic!("round {round}: the incumbent fails the model: {reason}\n{source}")
        });
        assert_eq!(
            rescreened.objective.to_canonical_string(),
            incumbent.objective.to_canonical_string(),
            "round {round}: the incumbent's objective is not the model's\n{source}"
        );
        checked += 1;
    }

    assert!(
        checked > 0,
        "no search accepted a guessed candidate, so nothing was rescreened"
    );
}

#[test]
fn the_ladder_escalates_past_rounding_and_both_rungs_land() {
    let mut generator = Xoshiro256PlusPlus::from_seed(0x4e57_0000_0000_0014);
    let mut ran = [0_usize; 3];
    let mut landed = [0_usize; 3];

    for round in 0..200 {
        let source = generate(&mut generator, 1 + (round % 2), 2 + (round % 3));
        let model = read(source.as_bytes(), Form::Free).expect("the source parses");
        let search = run(&model, &Options::default());

        for attempt in &search.heuristics {
            let rung = match attempt.rule {
                "round" => 0,
                "dive" => 1,
                "pump" => 2,
                other => panic!("a run reported the unknown rule {other:?}"),
            };
            ran[rung] += 1;
            if attempt.outcome == "found" {
                landed[rung] += 1;
            }
        }
    }

    for (rung, name) in ["round", "dive", "pump"].into_iter().enumerate() {
        assert!(
            ran[rung] > 0,
            "the {name} heuristic never ran, so it is wired to nothing"
        );
        assert!(
            landed[rung] > 0,
            "the {name} heuristic never produced a candidate the model accepted"
        );
    }

    // Rounding is the cheap rung and runs at every splitting node; the other
    // two are gated on there being no incumbent yet. A corpus where the
    // expensive rungs ran as often as the cheap one would mean the gate is not
    // gating.
    assert!(
        ran[1] + ran[2] < ran[0],
        "the expensive rungs ran {} times against rounding's {}, so the ladder is not a ladder",
        ran[1] + ran[2],
        ran[0]
    );
}

#[test]
fn the_pump_is_reproducible_across_searches() {
    let mut generator = Xoshiro256PlusPlus::from_seed(0x4e57_0000_0000_0015);
    let mut compared = 0;

    for round in 0..120 {
        let source = generate(&mut generator, 1 + (round % 2), 2 + (round % 3));
        let model = read(source.as_bytes(), Form::Free).expect("the source parses");

        let first = run(&model, &Options::default());
        let again = run(&model, &Options::default());
        assert_eq!(
            first.heuristics, again.heuristics,
            "round {round}: two searches of one instance filed different runs\n{source}"
        );
        if first.heuristics.iter().any(|run| run.rule == "pump") {
            compared += 1;
        }
    }

    assert!(
        compared > 0,
        "no search reached the pump, so its reproducibility is untested"
    );
}
