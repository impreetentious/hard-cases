//! The fuzz runner.
//!
//! A shell: it reads a budget, runs the campaigns, prints what they found, and
//! exits nonzero if they found anything. Everything that can be wrong lives in
//! the library, where the tests can reach it.

use hc_fuzz::{Budget, parsers, proofs, solver};

fn main() -> std::process::ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let budget = match arguments.as_slice() {
        [] => Budget::PER_CHANGE,
        [word] if word == "--nightly" => Budget::NIGHTLY,
        [word, cases] if word == "--cases" => match cases.parse() {
            Ok(cases) => Budget { from: 1, cases },
            Err(_) => return usage(),
        },
        [word, from, cases] if word == "--from" => match (from.parse(), cases.parse()) {
            (Ok(from), Ok(cases)) => Budget { from, cases },
            _ => return usage(),
        },
        _ => return usage(),
    };

    let mut report = parsers::campaign(budget);
    report.absorb(proofs::campaign(budget));
    report.absorb(solver::campaign(budget));
    println!("fuzz: {}", report.line());
    for finding in &report.findings {
        print!("{finding}");
    }
    if report.clean() {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}

fn usage() -> std::process::ExitCode {
    eprint!(
        "\
usage:
  hc-fuzz                       the budget a change pays before it lands
  hc-fuzz --nightly             the budget the nightly run pays
  hc-fuzz --cases <n>           a run of n seeds from the first
  hc-fuzz --from <seed> <n>     n seeds from one, to reproduce a finding

Every case is derived from its seed and nothing else, so a finding is
reproducible by running the seed it names.
"
    );
    std::process::ExitCode::from(2)
}
