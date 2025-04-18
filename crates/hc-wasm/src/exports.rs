//! The C interface, and the only unsafe code in the project.
//!
//! Four exports and three allocators. Every call takes a pointer and a length
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

use crate::{recheck, replay, solve_live};

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
    fn the_module_says_which_version_it_was_built_from() {
        let stated = taken(hc_version());
        assert!(stated.contains(env!("CARGO_PKG_VERSION")));
        assert!(stated.contains("\"trace_schema\": 1"));
    }
}
