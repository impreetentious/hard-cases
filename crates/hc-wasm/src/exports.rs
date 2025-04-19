//! The C interface, and the only unsafe code in the project.
//!
//! Eight exports and three allocators. Every call takes a pointer and a length
//! the caller received from `hc_reserve` and filled in, and returns a pointer
//! to an *answer*: four bytes of little-endian length followed by that many
//! bytes of UTF-8. The caller reads them out and hands the pointer back to
//! `hc_release_answer`.
//!
//! A length prefix rather than a pointer and a length packed into one word.
//! Packing works on a thirty-two-bit target and silently loses the top of
//! every pointer anywhere else, so the tests below — which run on the host, on
//! purpose, because a wasm interface nobody exercises is a wasm interface
//! nobody has run — would be testing something the browser never does.
//!
//! What the unsafe blocks do is exactly one thing — turn a pointer and a
//! length back into the slice they came from — and they are written out
//! separately rather than hidden behind a helper so that a reader can count
//! them. The safety condition is the same in all of them and is stated once
//! here: **the pointer and the length must be a pair this module handed out
//! and the caller has not released.** A caller that invents a pair is a caller
//! that can read this module's memory, which is what a C interface is.
//!
//! Nothing here decides anything. Each export reads its arguments, calls the
//! function above it, and returns the answer as bytes; a failure comes back as
//! a JSON object with a message in it rather than as a trap, because a page
//! that has to catch a trap to find out its file was malformed cannot say what
//! was wrong with it.

use crate::{outline, pivots, recheck, replay, solve_live, tamper_cases, tampered};

/// Reserves a buffer the caller fills and then hands back to a call.
///
/// The allocation is exactly the length asked for, never more. A buffer with
/// spare capacity would be released against a different layout from the one it
/// was made with, which is undefined behaviour and, in practice, a crash on
/// the second call — the interface hands out lengths, so a length is all it
/// can be given back.
///
/// # Panics
///
/// Panics when the length is so large the allocator refuses it, which in a
/// browser means the tab is out of memory and there is nothing useful to
/// return.
#[unsafe(no_mangle)]
pub extern "C" fn hc_reserve(length: usize) -> *mut u8 {
    let room: Box<[u8]> = vec![0u8; length].into_boxed_slice();
    Box::into_raw(room).cast::<u8>()
}

/// Releases a buffer this module handed out.
///
/// # Safety
///
/// The pointer and the length must be a pair this module returned and the
/// caller has not already released.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hc_release(pointer: *mut u8, length: usize) {
    if pointer.is_null() {
        return;
    }
    drop(unsafe { Vec::from_raw_parts(pointer, length, length) });
}

/// Writes a string into this module's memory behind its own length.
///
/// Boxed rather than leaked from a vector, for the reason `hc_reserve` gives:
/// the allocation has to be exactly as long as what the release call will
/// reconstruct, and the release call has only the prefix to go on.
fn answer(text: String) -> *mut u8 {
    let bytes = text.into_bytes();
    let mut out = Vec::with_capacity(4 + bytes.len());
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&bytes);
    Box::into_raw(out.into_boxed_slice()).cast::<u8>()
}

/// Releases an answer this module returned.
///
/// # Safety
///
/// The pointer must be one a call above returned and the caller has not
/// already released.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hc_release_answer(pointer: *mut u8) {
    if pointer.is_null() {
        return;
    }
    let prefix = unsafe { core::slice::from_raw_parts(pointer, 4) };
    let length = u32::from_le_bytes([prefix[0], prefix[1], prefix[2], prefix[3]]) as usize;
    let whole = 4 + length;
    drop(unsafe { Vec::from_raw_parts(pointer, whole, whole) });
}

/// The two halves of a packed word, as a slice.
///
/// # Safety
///
/// The pointer and the length must describe a buffer the caller reserved and
/// filled.
unsafe fn borrow<'a>(pointer: *const u8, length: usize) -> &'a [u8] {
    if pointer.is_null() || length == 0 {
        return &[];
    }
    unsafe { core::slice::from_raw_parts(pointer, length) }
}

/// Solves an instance and returns the report as JSON.
///
/// # Safety
///
/// The pointer and the length must describe a buffer the caller reserved.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hc_solve_live(
    instance: *const u8,
    length: usize,
    format: u8,
    nodes: u32,
) -> *mut u8 {
    let raw = unsafe { borrow(instance, length) };
    answer(match solve_live(raw, format, u64::from(nodes)) {
        Ok(report) => report,
        Err(message) => refusal(&message),
    })
}

/// Replays a recorded trace and certificate against an instance.
///
/// # Safety
///
/// Every pointer and length must describe a buffer the caller reserved.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hc_replay(
    trace: *const u8,
    trace_length: usize,
    proof: *const u8,
    proof_length: usize,
    instance: *const u8,
    instance_length: usize,
    format: u8,
) -> *mut u8 {
    let trace = unsafe { borrow(trace, trace_length) };
    let proof = unsafe { borrow(proof, proof_length) };
    let raw = unsafe { borrow(instance, instance_length) };
    answer(match replay(trace, proof, raw, format) {
        Ok(report) => report,
        Err(message) => refusal(&message),
    })
}

/// Rechecks a certificate against an instance and returns the verdict alone.
///
/// # Safety
///
/// Every pointer and length must describe a buffer the caller reserved.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hc_recheck(
    proof: *const u8,
    proof_length: usize,
    instance: *const u8,
    instance_length: usize,
) -> *mut u8 {
    let proof = unsafe { borrow(proof, proof_length) };
    let raw = unsafe { borrow(instance, instance_length) };
    answer(recheck(proof, raw))
}

/// Runs the root relaxation under the textbook rule and reports every pivot.
///
/// # Safety
///
/// The pointer and the length must describe a buffer the caller reserved.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hc_pivots(
    instance: *const u8,
    length: usize,
    format: u8,
    limit: u32,
) -> *mut u8 {
    let raw = unsafe { borrow(instance, length) };
    answer(match pivots(raw, format, limit as usize) {
        Ok(glass) => glass,
        Err(message) => refusal(&message),
    })
}

/// Reads a certificate the way the verifier reads it and reports the
/// structure alongside the verdict.
///
/// # Safety
///
/// Every pointer and length must describe a buffer the caller reserved.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hc_outline(
    proof: *const u8,
    proof_length: usize,
    instance: *const u8,
    instance_length: usize,
) -> *mut u8 {
    let proof = unsafe { borrow(proof, proof_length) };
    let raw = unsafe { borrow(instance, instance_length) };
    answer(outline(proof, raw))
}

/// The tamper corpus, as names and expected rejections.
#[unsafe(no_mangle)]
pub extern "C" fn hc_tamper_cases() -> *mut u8 {
    answer(tamper_cases())
}

/// Applies a named mutation and reports what the verifier says about it.
///
/// # Safety
///
/// Every pointer and length must describe a buffer the caller reserved.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hc_tamper(
    name: *const u8,
    name_length: usize,
    proof: *const u8,
    proof_length: usize,
    instance: *const u8,
    instance_length: usize,
    other: *const u8,
    other_length: usize,
) -> *mut u8 {
    let name = unsafe { borrow(name, name_length) };
    let proof = unsafe { borrow(proof, proof_length) };
    let raw = unsafe { borrow(instance, instance_length) };
    let other = unsafe { borrow(other, other_length) };
    match core::str::from_utf8(name) {
        Ok(name) => answer(tampered(name, proof, raw, other)),
        Err(_) => answer(refusal("the mutation name is not text")),
    }
}

/// The version this module was built from, so a page can say what it is running.
#[unsafe(no_mangle)]
pub extern "C" fn hc_version() -> *mut u8 {
    answer(format!(
        "{{\"version\": \"{}\", \"trace_schema\": {}}}",
        env!("CARGO_PKG_VERSION"),
        hc_core::trace::VERSION
    ))
}

/// A refusal, in the shape every other answer has.
fn refusal(message: &str) -> String {
    format!(
        "{{\n  \"provenance\": null,\n  \"refused\": {}\n}}",
        crate::quoted(message)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const TINY: &[u8] = include_bytes!("../../../spec/v1/vectors/mps/tiny.mps");

    /// Copies bytes in through the reserve the interface asks callers to use.
    fn handed(bytes: &[u8]) -> (*mut u8, usize) {
        let pointer = hc_reserve(bytes.len());
        unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer, bytes.len()) };
        (pointer, bytes.len())
    }

    /// Reads an answer back out and releases it, the way the page does.
    fn taken(pointer: *mut u8) -> String {
        let prefix = unsafe { borrow(pointer, 4) };
        let length = u32::from_le_bytes([prefix[0], prefix[1], prefix[2], prefix[3]]) as usize;
        let body = unsafe { borrow(pointer.wrapping_add(4), length) };
        let text = String::from_utf8(body.to_vec()).expect("the answers are text");
        unsafe { hc_release_answer(pointer) };
        text
    }

    #[test]
    fn the_interface_carries_a_solve_in_and_a_report_out() {
        let (instance, length) = handed(TINY);
        let report = taken(unsafe { hc_solve_live(instance, length, 1, 100) });
        unsafe { hc_release(instance, length) };
        assert!(report.contains("\"provenance\": \"live\""));
        assert!(report.contains("\"accepted\": true"));
    }

    #[test]
    fn a_refusal_comes_back_as_an_answer_rather_than_as_a_trap() {
        // A page that had to catch a trap to find out its file was malformed
        // could not say what was wrong with it.
        let (instance, length) = handed(b"not an instance\n");
        let report = taken(unsafe { hc_solve_live(instance, length, 1, 100) });
        unsafe { hc_release(instance, length) };
        assert!(report.contains("\"provenance\": null"));
        assert!(report.contains("\"refused\""));
    }

    #[test]
    fn an_empty_buffer_is_an_empty_slice_and_not_a_fault() {
        let report = taken(unsafe { hc_solve_live(core::ptr::null(), 0, 1, 10) });
        assert!(report.contains("\"refused\""));
    }

    #[test]
    fn the_glass_carries_a_run_of_pivots_out() {
        let (instance, length) = handed(TINY);
        let glass = taken(unsafe { hc_pivots(instance, length, 1, 64) });
        unsafe { hc_release(instance, length) };
        assert!(glass.contains("\"rule\": \"dantzig\""));
        assert!(glass.contains("\"entering\""));
        // Nothing in the glass is a claim, so no exact value appears in it.
        assert!(!glass.contains("\"stated\""));
    }

    #[test]
    fn the_outline_reads_a_certificate_the_way_the_verifier_does() {
        let (instance, length) = handed(TINY);
        let report = taken(unsafe { hc_solve_live(instance, length, 1, 100) });
        let hex = report
            .split("\"hex\": \"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("the certificate came back");
        let proof: Vec<u8> = (0..hex.len() / 2)
            .map(|at| u8::from_str_radix(&hex[at * 2..at * 2 + 2], 16).expect("hex"))
            .collect();
        let (held, proof_length) = handed(&proof);
        let drawn = taken(unsafe { hc_outline(held, proof_length, instance, length) });
        unsafe { hc_release(held, proof_length) };
        unsafe { hc_release(instance, length) };
        assert!(drawn.contains("\"read\": true"));
        assert!(drawn.contains("\"accepted\": true"));
        assert!(drawn.contains("\"kind\": \"assertion\""));
        assert!(drawn.contains("\"kind\": \"terminator\""));
    }

    #[test]
    fn the_corpus_can_be_driven_through_the_interface() {
        let listed = taken(hc_tamper_cases());
        assert!(listed.contains("\"name\": \"negated_multiplier\""));
        assert!(listed.contains("\"expect\": \"HCC-"));

        let (instance, length) = handed(TINY);
        let report = taken(unsafe { hc_solve_live(instance, length, 1, 100) });
        let hex = report
            .split("\"hex\": \"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("the certificate came back");
        let proof: Vec<u8> = (0..hex.len() / 2)
            .map(|at| u8::from_str_radix(&hex[at * 2..at * 2 + 2], 16).expect("hex"))
            .collect();
        let (held, proof_length) = handed(&proof);
        let (name, name_length) = handed(b"misstated_objective");
        let answered = taken(unsafe {
            hc_tamper(
                name,
                name_length,
                held,
                proof_length,
                instance,
                length,
                core::ptr::null(),
                0,
            )
        });
        unsafe { hc_release(name, name_length) };
        unsafe { hc_release(held, proof_length) };
        unsafe { hc_release(instance, length) };

        // Either the mutation had nothing to edit on this certificate, or it
        // was applied and refused at the rule the corpus names. What must not
        // happen is a forgery that verifies.
        if answered.contains("\"applied\": true") {
            assert!(answered.contains("\"accepted\": false"));
            assert!(answered.contains("\"rejection\": \"HCC-POINT-04\""));
        } else {
            assert!(answered.contains("\"why\""));
        }
    }

    #[test]
    fn a_mutation_nobody_has_heard_of_is_answered_rather_than_applied() {
        let (name, name_length) = handed(b"there_is_no_such_case");
        let answered = taken(unsafe {
            hc_tamper(
                name,
                name_length,
                core::ptr::null(),
                0,
                core::ptr::null(),
                0,
                core::ptr::null(),
                0,
            )
        });
        unsafe { hc_release(name, name_length) };
        assert!(answered.contains("\"applied\": false"));
    }

    #[test]
    fn the_module_says_which_version_it_was_built_from() {
        let stated = taken(hc_version());
        assert!(stated.contains(env!("CARGO_PKG_VERSION")));
        assert!(stated.contains("\"trace_schema\": 1"));
    }
}
