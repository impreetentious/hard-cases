//! The command line's grammar.
//!
//! Separate from the dispatch so that it can be tested, and unforgiving for
//! the same reason the verifier's is: an option read as a filename or a
//! missing operand quietly defaulted is how a tool ends up appearing to have
//! done more than it did.

/// Which format's rules an instance file is read under.
///
/// Named on the command line, never guessed from the content or the file name.
/// The format tag is inside the instance digest, so a file read under the
/// wrong rules is a different instance, and a heuristic here would make the
/// identity of a model depend on a guess.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shape {
    /// MPS read from its column windows.
    #[default]
    MpsFixed,
    /// MPS with whitespace-delimited fields.
    MpsFree,
    /// The accepted LP-format subset.
    Lp,
    /// The JSON instance schema.
    Json,
}

impl Shape {
    /// The spelling the command line uses.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MpsFixed => "mps-fixed",
            Self::MpsFree => "mps-free",
            Self::Lp => "lp",
            Self::Json => "json",
        }
    }

    /// The format a word names.
    #[must_use]
    pub fn named(word: &str) -> Option<Self> {
        match word {
            "mps-fixed" => Some(Self::MpsFixed),
            "mps-free" => Some(Self::MpsFree),
            "lp" => Some(Self::Lp),
            "json" => Some(Self::Json),
            _ => None,
        }
    }
}

/// What the command line was asked to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Solve an instance and, unless refused, certify the result.
    Solve {
        /// The instance file.
        instance: String,
        /// Where to write the certificate.
        certificate: Option<String>,
        /// Whether to skip assembly entirely.
        no_certificate: bool,
        /// Whether to solve by the dual method where it can start.
        dual: bool,
        /// Whether to reduce the model before searching.
        presolve: bool,
        /// A file holding a candidate solution to start from.
        solution: Option<String>,
        /// Whether to branch by the textbook rule instead of pseudocosts.
        simple_branching: bool,
        /// Whether to separate cuts.
        cuts: bool,
        /// Which format's rules the instance is read under.
        shape: Shape,
    },
    /// Recheck a certificate against an instance.
    Verify {
        /// The certificate file.
        certificate: String,
        /// The instance file.
        instance: String,
    },
    /// Read an instance and print its canonical form, checking nothing else.
    Check {
        /// The instance file.
        instance: String,
        /// Which format's rules the instance is read under.
        shape: Shape,
    },
    /// Solve an instance and print the proof its certificate contains.
    Explain {
        /// The instance file.
        instance: String,
        /// Which format's rules the instance is read under.
        shape: Shape,
    },
    /// Print the trust-boundary receipt for a verifier source tree.
    Receipt {
        /// The crate directory to measure.
        root: String,
    },
    /// Print which code is present.
    Version,
}

/// The usage text.
pub const USAGE: &str = "\
usage:
  hc solve <instance> [--format <name>] [--cert <file>] [--no-cert] [--dual]
                      [--no-presolve] [--solution <file>] [--branching simple]
                      [--no-cuts]
  hc verify <certificate> --instance <file>
  hc check <instance> [--format <name>]
  hc explain <instance> [--format <name>]
  hc receipt [<verifier crate directory>]
  hc --version

formats mps-fixed (the default), mps-free, lp, json. The format is named, never
        guessed: it is part of the instance digest, so the same bytes read two
        ways are two instances. verify takes none, because the certificate
        states which format its claim is about.

solve   reads an instance, solves it, rebuilds the result exactly, and has the
        independent verifier recheck the certificate before reporting a status.
verify  runs that verifier alone, which is what a release script does.
check   prints the canonical form of an instance and nothing else.
explain solves, certifies, verifies, and prints the proof tree the certificate
        contains. Every line of it comes from the certificate, so a line is
        something the verifier checked.
receipt measures the trusted base: files, counted lines, source hash,
        declared dependencies, and uses of unsafe code.
";

/// Reads an argument list.
#[must_use]
pub fn parse(arguments: &[String]) -> Option<Command> {
    let (head, rest) = arguments.split_first()?;
    match head.as_str() {
        "--version" if rest.is_empty() => Some(Command::Version),
        "solve" => solve(rest),
        "verify" => verify(rest),
        "receipt" => match rest {
            [] => Some(Command::Receipt {
                root: "crates/hc-verify".to_owned(),
            }),
            [root] if !root.starts_with('-') => Some(Command::Receipt { root: root.clone() }),
            _ => None,
        },
        "explain" => {
            let (instance, shape) = one_file(rest)?;
            Some(Command::Explain { instance, shape })
        }
        "check" => {
            let (instance, shape) = one_file(rest)?;
            Some(Command::Check { instance, shape })
        }
        _ => None,
    }
}

/// Reads a subcommand that takes one instance file and an optional format.
fn one_file(arguments: &[String]) -> Option<(String, Shape)> {
    match arguments {
        [instance] if !instance.starts_with('-') => Some((instance.clone(), Shape::default())),
        [instance, flag, word] if !instance.starts_with('-') && flag == "--format" => {
            Some((instance.clone(), Shape::named(word)?))
        }
        [flag, word, instance] if !instance.starts_with('-') && flag == "--format" => {
            Some((instance.clone(), Shape::named(word)?))
        }
        _ => None,
    }
}

fn solve(arguments: &[String]) -> Option<Command> {
    let mut instance = None;
    let mut certificate = None;
    let mut no_certificate = false;
    let mut dual = false;
    let mut presolve = true;
    let mut solution = None;
    let mut simple_branching = false;
    let mut cuts = true;
    let mut shape = None;
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--cert" => {
                if certificate.is_some() {
                    return None;
                }
                certificate = Some(arguments.get(index + 1)?.clone());
                index += 2;
            }
            "--no-cert" => {
                if no_certificate {
                    return None;
                }
                no_certificate = true;
                index += 1;
            }
            "--dual" => {
                if dual {
                    return None;
                }
                dual = true;
                index += 1;
            }
            "--no-presolve" => {
                if !presolve {
                    return None;
                }
                presolve = false;
                index += 1;
            }
            "--no-cuts" => {
                if !cuts {
                    return None;
                }
                cuts = false;
                index += 1;
            }
            "--format" => {
                if shape.is_some() {
                    return None;
                }
                shape = Some(Shape::named(arguments.get(index + 1)?)?);
                index += 2;
            }
            "--solution" => {
                if solution.is_some() {
                    return None;
                }
                solution = Some(arguments.get(index + 1)?.clone());
                index += 2;
            }
            "--branching" => {
                // One rule is named and it is the only one that can be. The
                // other is the default, and asking for it by name would be a
                // second spelling of doing nothing.
                if simple_branching || arguments.get(index + 1)? != "simple" {
                    return None;
                }
                simple_branching = true;
                index += 2;
            }
            other if other.starts_with('-') => return None,
            other if instance.is_none() => {
                instance = Some(other.to_owned());
                index += 1;
            }
            _ => return None,
        }
    }
    // Asking for a certificate and refusing to assemble one at the same time
    // is a contradiction, and guessing which half was meant is the wrong
    // response to it.
    if no_certificate && certificate.is_some() {
        return None;
    }
    Some(Command::Solve {
        instance: instance?,
        certificate,
        no_certificate,
        dual,
        presolve,
        solution,
        simple_branching,
        cuts,
        shape: shape.unwrap_or_default(),
    })
}

fn verify(arguments: &[String]) -> Option<Command> {
    let mut certificate = None;
    let mut instance = None;
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--instance" => {
                if instance.is_some() {
                    return None;
                }
                instance = Some(arguments.get(index + 1)?.clone());
                index += 2;
            }
            other if other.starts_with('-') => return None,
            other if certificate.is_none() => {
                certificate = Some(other.to_owned());
                index += 1;
            }
            _ => return None,
        }
    }
    Some(Command::Verify {
        certificate: certificate?,
        instance: instance?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|word| (*word).to_owned()).collect()
    }

    #[test]
    fn solve_takes_an_instance_and_an_optional_destination() {
        assert_eq!(
            parse(&words(&["solve", "model.mps"])),
            Some(Command::Solve {
                instance: "model.mps".to_owned(),
                certificate: None,
                no_certificate: false,
                dual: false,
                presolve: true,
                solution: None,
                simple_branching: false,
                cuts: true,
                shape: Shape::MpsFixed,
            })
        );
        assert_eq!(
            parse(&words(&["solve", "model.mps", "--cert", "out.hcc"])),
            Some(Command::Solve {
                instance: "model.mps".to_owned(),
                certificate: Some("out.hcc".to_owned()),
                no_certificate: false,
                dual: false,
                presolve: true,
                solution: None,
                simple_branching: false,
                cuts: true,
                shape: Shape::MpsFixed,
            })
        );
    }

    #[test]
    fn the_dual_method_is_asked_for_by_name_and_only_once() {
        assert_eq!(
            parse(&words(&["solve", "m.mps", "--dual"])),
            Some(Command::Solve {
                instance: "m.mps".to_owned(),
                certificate: None,
                no_certificate: false,
                dual: true,
                presolve: true,
                solution: None,
                simple_branching: false,
                cuts: true,
                shape: Shape::MpsFixed,
            })
        );
        assert_eq!(parse(&words(&["solve", "m.mps", "--dual", "--dual"])), None);
    }

    #[test]
    fn a_solution_file_and_the_teaching_branching_rule_are_named() {
        let parsed = parse(&words(&[
            "solve",
            "m.mps",
            "--solution",
            "start.txt",
            "--branching",
            "simple",
        ]));
        assert_eq!(
            parsed,
            Some(Command::Solve {
                instance: "m.mps".to_owned(),
                certificate: None,
                no_certificate: false,
                dual: false,
                presolve: true,
                solution: Some("start.txt".to_owned()),
                simple_branching: true,
                cuts: true,
                shape: Shape::MpsFixed,
            })
        );
        assert_eq!(parse(&words(&["solve", "m.mps", "--branching"])), None);
        assert_eq!(
            parse(&words(&["solve", "m.mps", "--branching", "clever"])),
            None,
            "the only rule that can be asked for by name is the teaching one"
        );
        assert_eq!(parse(&words(&["solve", "m.mps", "--solution"])), None);
    }

    #[test]
    fn presolve_is_on_unless_it_is_turned_off_by_name() {
        assert_eq!(
            parse(&words(&["solve", "m.mps", "--no-presolve"])),
            Some(Command::Solve {
                instance: "m.mps".to_owned(),
                certificate: None,
                no_certificate: false,
                dual: false,
                presolve: false,
                solution: None,
                simple_branching: false,
                cuts: true,
                shape: Shape::MpsFixed,
            })
        );
        assert_eq!(
            parse(&words(&[
                "solve",
                "m.mps",
                "--no-presolve",
                "--no-presolve"
            ])),
            None
        );
    }

    #[test]
    fn asking_for_a_certificate_and_refusing_one_is_a_contradiction() {
        assert_eq!(
            parse(&words(&["solve", "m.mps", "--cert", "o.hcc", "--no-cert"])),
            None
        );
    }

    #[test]
    fn verify_needs_both_halves_and_the_order_does_not_matter() {
        let expected = Some(Command::Verify {
            certificate: "out.hcc".to_owned(),
            instance: "model.mps".to_owned(),
        });
        assert_eq!(
            parse(&words(&["verify", "out.hcc", "--instance", "model.mps"])),
            expected
        );
        assert_eq!(
            parse(&words(&["verify", "--instance", "model.mps", "out.hcc"])),
            expected
        );
        assert_eq!(parse(&words(&["verify", "out.hcc"])), None);
        assert_eq!(parse(&words(&["verify", "--instance", "model.mps"])), None);
    }

    #[test]
    fn explain_takes_one_instance_and_nothing_else() {
        assert_eq!(
            parse(&words(&["explain", "m.mps"])),
            Some(Command::Explain {
                instance: "m.mps".to_owned(),
                shape: Shape::MpsFixed,
            })
        );
        assert_eq!(parse(&words(&["explain"])), None);
        assert_eq!(parse(&words(&["explain", "m.mps", "--tree"])), None);
    }

    #[test]
    fn a_format_is_named_and_never_guessed() {
        // Every subcommand that reads an instance takes the same names, and
        // the flag may stand on either side of the file.
        for (word, shape) in [
            ("mps-fixed", Shape::MpsFixed),
            ("mps-free", Shape::MpsFree),
            ("lp", Shape::Lp),
            ("json", Shape::Json),
        ] {
            assert_eq!(
                parse(&words(&["check", "m.txt", "--format", word])),
                Some(Command::Check {
                    instance: "m.txt".to_owned(),
                    shape,
                })
            );
            assert_eq!(
                parse(&words(&["explain", "--format", word, "m.txt"])),
                Some(Command::Explain {
                    instance: "m.txt".to_owned(),
                    shape,
                })
            );
            let Some(Command::Solve { shape: found, .. }) =
                parse(&words(&["solve", "m.txt", "--format", word]))
            else {
                panic!("solve did not take --format {word}");
            };
            assert_eq!(found, shape);
            assert_eq!(Shape::named(word), Some(shape));
            assert_eq!(shape.as_str(), word);
        }

        // A format nobody implements, a flag with no name after it, and the
        // same flag twice are all misuse rather than a default.
        assert_eq!(
            parse(&words(&["check", "m.txt", "--format", "cplex"])),
            None
        );
        assert_eq!(parse(&words(&["check", "m.txt", "--format"])), None);
        assert_eq!(
            parse(&words(&[
                "solve", "m.txt", "--format", "lp", "--format", "lp"
            ])),
            None
        );
        assert_eq!(parse(&words(&["verify", "c.hcc", "--format", "lp"])), None);
    }

    #[test]
    fn the_receipt_defaults_to_the_verifier_and_accepts_one_path() {
        assert_eq!(
            parse(&words(&["receipt"])),
            Some(Command::Receipt {
                root: "crates/hc-verify".to_owned()
            })
        );
        assert_eq!(
            parse(&words(&["receipt", "crates/hc-verify"])),
            Some(Command::Receipt {
                root: "crates/hc-verify".to_owned()
            })
        );
        assert_eq!(parse(&words(&["receipt", "a", "b"])), None);
    }

    #[test]
    fn the_version_is_a_subcommand_of_its_own_and_takes_nothing() {
        assert_eq!(parse(&words(&["--version"])), Some(Command::Version));
        assert_eq!(parse(&words(&["--version", "solve"])), None);
    }

    #[test]
    fn an_unknown_subcommand_or_option_stops_rather_than_being_skipped() {
        assert_eq!(parse(&words(&["solve", "model.mps", "--fast"])), None);
        assert_eq!(parse(&words(&["check", "--all"])), None);
        assert_eq!(parse(&[]), None);
    }

    #[test]
    fn a_repeated_option_is_a_mistake_and_not_an_override() {
        assert_eq!(
            parse(&words(&[
                "solve", "m.mps", "--cert", "a.hcc", "--cert", "b.hcc"
            ])),
            None
        );
        assert_eq!(
            parse(&words(&[
                "verify",
                "a.hcc",
                "--instance",
                "m.mps",
                "--instance",
                "n.mps"
            ])),
            None
        );
    }
}
