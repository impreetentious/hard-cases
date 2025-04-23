#![forbid(unsafe_code)]

//! The trust-boundary receipt.
//!
//! A verifier asks to be believed. The least it can do is publish what it is:
//! how much code there is, what that code depends on, and the hash of the exact
//! source that did the checking. Those three numbers are what turn "the proof
//! was verified" into a statement somebody could go and check.
//!
//! The receipt is computed from the source tree rather than declared, so it
//! cannot drift from what is actually there. Nothing in it is aspirational and
//! nothing in it is a target: the line count is what the files hold today, and
//! the budget it is checked against is a separate number in the specification.
//!
//! Counting rules, stated because a line count nobody can reproduce is a
//! decoration. A line counts when it is not blank and is not a comment; test
//! modules are excluded, because a reader auditing the trusted base is auditing
//! what runs during a verification and the tests do not.
//!
//! Its own crate because it has two callers that must not be joined by it. The
//! command line offers `hc receipt`, and the scoreboard puts the same figures
//! on the front page; when this lived inside the benchmark harness, the first
//! of those made the shipped binary depend on the harness, and so on the
//! reference solver that exists to disagree with it. The decision log records that
//! edge should not exist. It reads the filesystem, so it sits outside the
//! deterministic perimeter with the other host crates.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use hc_core::Sha256;

/// What the trusted base measures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    /// Every source file, with its counted lines.
    pub files: BTreeMap<String, usize>,
    /// Nonblank, non-comment, non-test lines across all of them.
    pub lines: usize,
    /// Lines inside test modules, reported separately rather than hidden.
    pub test_lines: usize,
    /// The digest of every source file, in path order.
    pub source: [u8; 32],
    /// Dependency lines the manifest declares, which must be none.
    pub dependencies: Vec<String>,
    /// Uses of the unsafe keyword as a qualifier, which must be none.
    ///
    /// Counted as the constructs that actually introduce unsafety rather than
    /// as occurrences of the word: the crate opens by forbidding unsafe code,
    /// and a scan that counted that line would report the defence as the
    /// defect.
    pub unsafe_uses: usize,
    /// Whether the crate root forbids unsafe code outright.
    pub forbids_unsafe: bool,
}

impl Receipt {
    /// Renders the receipt as the release publishes it.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str("trust boundary\n");
        out.push_str(&format!("  files          {}\n", self.files.len()));
        out.push_str(&format!("  lines          {}\n", self.lines));
        out.push_str(&format!("  test lines     {}\n", self.test_lines));
        out.push_str(&format!(
            "  source         {}\n",
            hc_core::hex(&self.source)
        ));
        out.push_str(&format!("  dependencies   {}\n", self.dependencies.len()));
        out.push_str(&format!("  unsafe         {}\n", self.unsafe_uses));
        out.push_str(&format!(
            "  forbids unsafe {}\n",
            if self.forbids_unsafe { "yes" } else { "no" }
        ));
        out.push_str("  per file\n");
        for (path, lines) in &self.files {
            out.push_str(&format!("    {lines:>6}  {path}\n"));
        }
        out
    }
}

/// Measures a verifier source tree.
///
/// # Panics
///
/// Panics when the directory cannot be read. A receipt that silently reported
/// a smaller trusted base than exists would be worse than no receipt, so an
/// unreadable tree is a failure rather than a smaller number.
#[must_use]
pub fn of(crate_root: &Path) -> Receipt {
    let mut paths = Vec::new();
    collect(&crate_root.join("src"), &mut paths);
    paths.sort();

    let mut files = BTreeMap::new();
    let mut lines = 0;
    let mut test_lines = 0;
    let mut unsafe_uses = 0;
    let mut forbids_unsafe = false;
    let mut hash = Sha256::new();

    for path in &paths {
        let text = fs::read_to_string(path).expect("a source file the walk just found");
        hash.update(text.as_bytes());
        let (counted, tests) = count(&text);
        unsafe_uses += unsafe_qualifiers(&text);
        if text.contains("#![forbid(unsafe_code)]") {
            forbids_unsafe = true;
        }
        let relative = path
            .strip_prefix(crate_root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        files.insert(relative, counted);
        lines += counted;
        test_lines += tests;
    }

    Receipt {
        files,
        lines,
        test_lines,
        source: hash.finish(),
        dependencies: declared_dependencies(&crate_root.join("Cargo.toml")),
        unsafe_uses,
        forbids_unsafe,
    }
}

/// Uses of `unsafe` as a qualifier on a block, function, implementation, or
/// trait.
///
/// Deliberately not a substring count. The crate root carries
/// `#![forbid(unsafe_code)]`, and a scan that counted the word would report
/// that line as a use — turning the strongest evidence that there is no unsafe
/// code into evidence that there is some.
fn unsafe_qualifiers(text: &str) -> usize {
    [
        "unsafe {",
        "unsafe fn",
        "unsafe impl",
        "unsafe trait",
        "unsafe extern",
    ]
    .iter()
    .map(|pattern| text.matches(pattern).count())
    .sum()
}

/// Every Rust file beneath a directory, in walk order.
fn collect(directory: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, into);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            into.push(path);
        }
    }
}

/// Counted lines and test lines in one file.
///
/// The test boundary is the `#[cfg(test)]` attribute followed by a module,
/// which is the one shape this project uses. Everything from that attribute to
/// the end of the file is test code, because the test module is always last.
fn count(text: &str) -> (usize, usize) {
    let mut counted = 0;
    let mut tests = 0;
    let mut in_tests = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("#[cfg(test)]") {
            in_tests = true;
        }
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("/*") {
            continue;
        }
        if in_tests {
            tests += 1;
        } else {
            counted += 1;
        }
    }
    (counted, tests)
}

/// The dependency lines a manifest declares, which for the verifier must be
/// none at all.
fn declared_dependencies(manifest: &Path) -> Vec<String> {
    let Ok(text) = fs::read_to_string(manifest) else {
        return Vec::new();
    };
    let mut inside = false;
    let mut found = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            inside = matches!(
                trimmed,
                "[dependencies]" | "[dev-dependencies]" | "[build-dependencies]"
            );
            continue;
        }
        if inside && !trimmed.is_empty() && !trimmed.starts_with('#') {
            found.push(trimmed.to_owned());
        }
    }
    found
}
