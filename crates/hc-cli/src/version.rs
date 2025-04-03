//! Which code did the work.
//!
//! Both binaries print a version, and both take it from the manifest rather
//! than from a string somebody maintains. A tool that reported a version it
//! had been told to report would be reporting the last time someone
//! remembered, which is the one thing a receipt must not do.

/// The version line for the solver's command line.
#[must_use]
pub fn line() -> String {
    format!(
        "hc {}\nhc-verify {}\n",
        env!("CARGO_PKG_VERSION"),
        hc_verify::vcli::version()
            .trim_start_matches("hc-verify ")
            .trim_end()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_halves_report_and_they_report_the_same_release() {
        let text = line();
        let mut lines = text.lines();
        let solver = lines.next().expect("a solver line");
        let verifier = lines.next().expect("a verifier line");
        assert_eq!(lines.next(), None);

        let solver_version = solver.strip_prefix("hc ").expect("a version");
        let verifier_version = verifier.strip_prefix("hc-verify ").expect("a version");
        // They come from one workspace version, and a divergence would mean a
        // report was checked by code from a different release than the one
        // that produced it.
        assert_eq!(solver_version, verifier_version);
        assert_eq!(solver_version, env!("CARGO_PKG_VERSION"));
    }
}
