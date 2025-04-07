//! The command line's grammar.
//!
//! Separate from the dispatch so that it can be tested, and unforgiving for
//! the same reason the verifier's is: an option read as a filename or a
//! missing operand quietly defaulted is how a tool ends up appearing to have
//! done more than it did.

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
    },
    /// Solve an instance and print the proof its certificate contains.
    Explain {
        /// The instance file.
        instance: String,
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
  hc solve <instance> [--cert <file>] [--no-cert] [--dual] [--no-presolve]
  hc verify <certificate> --instance <file>
  hc check <instance>
  hc explain <instance>
  hc receipt [<verifier crate directory>]
  hc --version

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
        "explain" => match rest {
            [instance] if !instance.starts_with('-') => Some(Command::Explain {
                instance: instance.clone(),
            }),
            _ => None,
        },
        "check" => match rest {
            [instance] if !instance.starts_with('-') => Some(Command::Check {
                instance: instance.clone(),
            }),
            _ => None,
        },
        _ => None,
    }
}

fn solve(arguments: &[String]) -> Option<Command> {
    let mut instance = None;
    let mut certificate = None;
    let mut no_certificate = false;
    let mut dual = false;
    let mut presolve = true;
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
            })
        );
        assert_eq!(parse(&words(&["solve", "m.mps", "--dual", "--dual"])), None);
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
                instance: "m.mps".to_owned()
            })
        );
        assert_eq!(parse(&words(&["explain"])), None);
        assert_eq!(parse(&words(&["explain", "m.mps", "--tree"])), None);
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
