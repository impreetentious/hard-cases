//! Writes the scoreboard.
//!
//! Two modes. With no arguments it regenerates `docs/scoreboard.md` from the
//! committed corpora, which is what a person runs after changing something the
//! board reports. With `--fetched` it prints a board for whatever netlib and
//! MIPLIB instances are present on this machine, and names what is not — that
//! one is printed rather than written, because a document about what one
//! machine downloaded has no business being committed.
//!
//! The committed board is diffed by a test rather than by this binary, so a
//! stale board fails the ordinary suite instead of waiting for somebody to
//! remember to regenerate it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use hc_bench::corpus::run_one as run_linear;
use hc_bench::figure;
use hc_bench::miplib::{HEADING, against_published, run_one as run_integer};
use hc_bench::scoreboard::{COUNTERS_BEGIN, COUNTERS_END, NODE_BUDGET, counters, render};
use hc_core::{SpecDocument, SpecValue};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn main() -> ExitCode {
    if std::env::args().any(|argument| argument == "--fetched") {
        return fetched();
    }
    let path = root().join("docs").join("scoreboard.md");
    let text = render();
    if let Err(error) = fs::write(&path, &text) {
        eprintln!("scoreboard: could not write {}: {error}", path.display());
        return ExitCode::FAILURE;
    }
    println!(
        "scoreboard: wrote {} ({} bytes)",
        path.display(),
        text.len()
    );

    let picture = root().join("docs").join("proof-tree.svg");
    let drawn = figure::render();
    if let Err(error) = fs::write(&picture, &drawn) {
        eprintln!("scoreboard: could not write {}: {error}", picture.display());
        return ExitCode::FAILURE;
    }
    println!(
        "scoreboard: wrote {} ({} bytes)",
        picture.display(),
        drawn.len()
    );

    // And the front page's counters, spliced between their markers. The README
    // is written by hand everywhere else; these numbers are the part of it that
    // has to keep being true.
    let readme = root().join("README.md");
    let held = fs::read_to_string(&readme).expect("the front page");
    let verifier = root().join("crates").join("hc-verify");
    match splice(&held, &counters(&verifier)) {
        Err(message) => {
            eprintln!("scoreboard: {message}");
            return ExitCode::FAILURE;
        }
        Ok(replaced) => {
            fs::write(&readme, replaced).expect("the front page is writable");
            println!("scoreboard: refreshed the counters in {}", readme.display());
        }
    }
    ExitCode::SUCCESS
}

/// Replaces whatever sits between the counter markers.
///
/// # Errors
///
/// Returns the message to print when the markers are missing or out of order,
/// which means the front page has been edited into a shape this cannot write.
pub fn splice(document: &str, block: &str) -> Result<String, String> {
    let Some(from) = document.find(COUNTERS_BEGIN) else {
        return Err(format!("README.md has no {COUNTERS_BEGIN} marker"));
    };
    let Some(to) = document.find(COUNTERS_END) else {
        return Err(format!("README.md has no {COUNTERS_END} marker"));
    };
    if to < from {
        return Err("README.md has its counter markers the wrong way round".to_owned());
    }
    let mut out = String::with_capacity(document.len());
    out.push_str(&document[..from]);
    out.push_str(block);
    out.push_str(&document[to + COUNTERS_END.len()..]);
    Ok(out)
}

/// The fetched collections, for whatever is present here.
fn fetched() -> ExitCode {
    let mut absent = 0usize;
    let mut present = 0usize;

    println!("netlib\n");
    let netlib = root().join("instances").join("netlib");
    let manifest = fs::read_to_string(netlib.join("manifest.toml")).expect("the netlib manifest");
    let document = SpecDocument::parse(&manifest).expect("inside the subset");
    println!("instance                rows  cols     nz  solver      reference   iters  objective");
    for name in names(&document, "easy", "names") {
        let file = netlib.join(format!("{name}.mps"));
        match read_file(&file) {
            None => {
                println!("{name:<22} not fetched");
                absent += 1;
            }
            Some(raw) => {
                present += 1;
                let row = run_linear(&name, &raw, None);
                println!(
                    "{:<22} {:>5} {:>5} {:>6}  {:<11} {:<11} {:>5}  {}",
                    row.name,
                    row.shape.0,
                    row.shape.1,
                    row.nonzeros,
                    row.solver,
                    row.reference,
                    row.iterations,
                    row.objective
                        .map_or_else(|| "-".to_owned(), |value| format!("{value:.6}"))
                );
                if let Some(finding) = &row.finding {
                    println!("    finding: {finding}");
                }
            }
        }
    }

    println!("\nmiplib\n{HEADING}");
    let miplib = root().join("instances").join("miplib");
    let manifest = fs::read_to_string(miplib.join("manifest.toml")).expect("the miplib manifest");
    let document = SpecDocument::parse(&manifest).expect("inside the subset");
    let mut wanted = names(&document, "split", "dev");
    wanted.extend(names(&document, "split", "held"));
    for name in wanted {
        let file = miplib.join(format!("{name}.mps"));
        match read_file(&file) {
            None => {
                println!("{name:<16} not fetched");
                absent += 1;
            }
            Some(raw) => {
                present += 1;
                let row = run_integer(&name, &raw, NODE_BUDGET);
                println!("{}", row.render());
                if let Some(finding) = against_published(&row, None) {
                    println!("    finding: {finding}");
                }
            }
        }
    }

    println!("\n{present} instance(s) present, {absent} not fetched.");
    if present == 0 {
        println!(
            "Nothing was fetched into this checkout. ./scripts/fetch-netlib.sh and\n\
             ./scripts/fetch-miplib.sh record a digest on first fetch and verify it after."
        );
    }
    ExitCode::SUCCESS
}

fn names(document: &SpecDocument, table: &str, key: &str) -> Vec<String> {
    document
        .table(table)
        .and_then(|found| found.get(key))
        .and_then(SpecValue::as_list)
        .unwrap_or_else(|| panic!("[{table}] has no {key}"))
        .to_vec()
}

fn read_file(path: &Path) -> Option<Vec<u8>> {
    fs::read(path).ok()
}
