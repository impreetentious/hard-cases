//! `hc`, the command line.
//!
//! A shell around three library calls. The reading of files and the exit code
//! live here; everything that can be wrong lives in `args` and `run`, where
//! the tests can reach it.

mod args;
mod run;
mod version;

use std::process::ExitCode;

use args::{Command, USAGE, parse};
use run::Ending;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = parse(&arguments) else {
        eprint!("{USAGE}");
        return code(Ending::Misused);
    };

    match command {
        Command::Version => {
            print!("{}", version::line());
            code(Ending::Fine)
        }
        Command::Receipt { root } => {
            let directory = std::path::PathBuf::from(&root);
            if !directory.join("src").is_dir() {
                eprintln!("hc: {root} does not look like a crate directory");
                return code(Ending::Misused);
            }
            print!("{}", hc_bench::receipt::of(&directory).render());
            code(Ending::Fine)
        }
        Command::Explain { instance } => match slurp(&instance) {
            Err(ending) => code(ending),
            Ok(bytes) => {
                let (out, ending) = run::explain(&instance, &bytes);
                print!("{out}");
                code(ending)
            }
        },
        Command::Check { instance } => match slurp(&instance) {
            Err(ending) => code(ending),
            Ok(bytes) => {
                let (out, ending) = run::check(&bytes);
                print!("{out}");
                code(ending)
            }
        },
        Command::Verify {
            certificate,
            instance,
        } => {
            let (Ok(proof), Ok(model)) = (slurp(&certificate), slurp(&instance)) else {
                return code(Ending::Misused);
            };
            let (out, ending) = run::recheck(&certificate, &proof, &instance, &model);
            print!("{out}");
            code(ending)
        }
        Command::Solve {
            instance,
            certificate,
            no_certificate,
            dual,
            presolve,
            solution,
            simple_branching,
        } => {
            let Ok(bytes) = slurp(&instance) else {
                return code(Ending::Misused);
            };
            let start = match solution.as_deref().map(slurp).transpose() {
                Err(ending) => return code(ending),
                Ok(None) => None,
                Ok(Some(raw)) => match String::from_utf8(raw)
                    .map_err(|_| String::from("the solution file is not text"))
                    .and_then(|text| run::read_solution(&text))
                {
                    Ok(values) => Some(values),
                    Err(message) => {
                        eprintln!("hc: {message}");
                        return code(Ending::Misused);
                    }
                },
            };
            let settings = run::Settings {
                dual,
                presolve,
                solution: start,
                simple_branching,
            };
            let solved = match run::search(&bytes, &settings) {
                Ok(solved) => solved,
                Err(message) => {
                    println!("REFUSED: {message}");
                    return code(Ending::Refused);
                }
            };
            if no_certificate {
                print!("{}", run::candidate(&solved));
                return code(Ending::Fine);
            }

            let destination = certificate.unwrap_or_else(|| String::from("(not written)"));
            let (out, proof, ending) = run::certify(&instance, &bytes, &destination, &solved);
            print!("{out}");
            if let Some(proof) = proof {
                if destination != "(not written)" {
                    if let Err(error) = std::fs::write(&destination, &proof) {
                        eprintln!("hc: cannot write {destination}: {error}");
                        return code(Ending::Refused);
                    }
                }
            }
            code(ending)
        }
    }
}

/// Reads a file, reporting the failure the way the rest of the tool does.
fn slurp(path: &str) -> Result<Vec<u8>, Ending> {
    std::fs::read(path).map_err(|error| {
        eprintln!("hc: cannot read {path}: {error}");
        Ending::Misused
    })
}

/// The exit code an ending travels as.
fn code(ending: Ending) -> ExitCode {
    match ending {
        Ending::Fine => ExitCode::SUCCESS,
        Ending::Refused => ExitCode::FAILURE,
        Ending::Misused => ExitCode::from(2),
    }
}
