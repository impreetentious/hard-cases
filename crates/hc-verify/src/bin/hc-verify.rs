//! The standalone verifier.
//!
//! This file is the only one in the crate that touches anything outside the
//! process. It reads two files and the command line, hands the bytes to the
//! library, prints what the library renders, and exits zero or one. It holds
//! no logic of its own, and the isolation check in continuous integration
//! enforces exactly that: this is the single named exemption from the
//! ambient-input ban, and it is paid for by a check that no other file in the
//! crate reaches outside the process.
//!
//! It links nothing but the standard library and the verifier. That is the
//! point of shipping it separately: verifying a certificate must never require
//! having the solver on the machine, let alone linked into the checker.

use std::process::ExitCode;

use hc_verify::vcli::{Ask, USAGE, accepted, refused, request, version};
use hc_verify::vclose::verify;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let asked = match request(&arguments) {
        Ask::Recheck(asked) => asked,
        Ask::Version => {
            print!("{}", version());
            return ExitCode::SUCCESS;
        }
        Ask::Usage => {
            eprint!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    let (certificate_path, instance_path) = (asked.certificate, asked.instance);

    let certificate = match std::fs::read(&certificate_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("hc-verify: cannot read {certificate_path}: {error}");
            return ExitCode::from(2);
        }
    };
    let instance = match std::fs::read(&instance_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("hc-verify: cannot read {instance_path}: {error}");
            return ExitCode::from(2);
        }
    };

    match verify(&certificate, &instance) {
        Ok(verdict) => {
            print!(
                "{}",
                accepted(&instance_path, instance.len(), &certificate_path, &verdict)
            );
            ExitCode::SUCCESS
        }
        Err(refusal) => {
            print!("{}", refused(&refusal));
            ExitCode::FAILURE
        }
    }
}
