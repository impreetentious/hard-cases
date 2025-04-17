//! The trace: what the search did, written down.
//!
//! A trace is not a proof and never becomes one. A reader learns which nodes
//! were opened and in what order, which cuts entered the pool, where the
//! incumbent came from; it learns nothing about whether the answer is right.
//! That is the certificate's job, and keeping the two apart is the reason this
//! module carries no verification and produces nothing a claim may rest on.
//!
//! The rule the schema is built around is visible in the types below. A search
//! bound, a violation, a node objective are `f64`, because the search that
//! produced them is floating point. An incumbent value, a node bound, a branch
//! threshold are exact text, because those are quantities a proof would cite.
//! The two are never the same field and never spelled the same way: in the
//! textual encoding a float is a number and an exact value is a string, so a
//! consumer that renders one as the other has to do it on purpose.
//!
//! Both encodings carry the same events, and the round trip between them is
//! byte-exact. That is what makes them one schema rather than two formats.

use core::fmt::Write as _;

use crate::hash::crc32c;

/// The four bytes a trace begins with.
pub const MAGIC: &[u8; 4] = b"HCT1";
/// The schema version this module reads and writes.
pub const VERSION: u8 = 1;
/// The byte the terminating record carries.
const END: u8 = 255;

/// Where the linear search is on its escalation ladder.
pub const STAGES: [&str; 4] = ["normal", "refactored", "perturbed", "bland"];
/// Which separator derived a cut.
pub const FAMILIES: [&str; 2] = ["gomory", "cover"];
/// Where an incumbent came from.
pub const SOURCES: [&str; 5] = ["round", "dive", "pump", "supplied", "node"];
/// Why an open node could not be given an exact bound.
pub const BLOCKED: [&str; 3] = ["no-bound", "cap", "refused"];
/// How a run ended.
pub const STATUSES: [&str; 7] = [
    "optimal",
    "infeasible",
    "unbounded",
    "feasible(bound,gap)",
    "feasible(no-certified-bound)",
    "no-progress",
    "error",
];

/// One thing that happened.
///
/// The list is closed. A new kind is a version bump on this contract, because
/// a consumer that skipped an event it did not recognise would be reporting a
/// search that did not happen.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// A presolve transform was applied.
    TransformApplied { rule: u64, position: u64 },
    /// The root relaxation was solved.
    RootLp { iterations: u64, objective: f64 },
    /// A node was opened.
    NodeOpen { node: u64, parent: u64, depth: u64 },
    /// A node relaxation was solved.
    NodeSolved {
        node: u64,
        iterations: u64,
        objective: f64,
    },
    /// A node was closed by bound.
    NodePruned { node: u64, bound: String },
    /// A node relaxation had no feasible point.
    NodeInfeasible { node: u64 },
    /// A node relaxation was integral where integrality is declared.
    NodeIntegral { node: u64, objective: String },
    /// A node was split on a variable at an integer threshold.
    Branch {
        node: u64,
        variable: u64,
        split: String,
    },
    /// A cut entered the pool.
    CutAdded {
        identifier: u64,
        scope: u64,
        family: u8,
        violation: f64,
    },
    /// A point survived exact screening and became the incumbent.
    Incumbent { objective: String, source: u8 },
    /// The float bounds the search is working between.
    SearchGap { lower: f64, upper: f64 },
    /// Frontier certification began.
    CertifyStart,
    /// An open node received an exact bound.
    CertifyNode { node: u64, bound: String },
    /// An open node could not receive one.
    CertifyBlocked { node: u64, reason: u8 },
    /// Certification finished with an exact global bound and gap.
    CertifyComplete {
        lower: String,
        upper: String,
        gap: String,
    },
    /// The linear search moved up its escalation ladder.
    Escalation { stage: u8 },
    /// A certificate was assembled.
    ProofAssembled { bytes: u64, checksum: u64 },
    /// The independent verifier answered.
    VerifyResult {
        accepted: bool,
        rejection: Option<String>,
        work: u64,
    },
    /// How the run ended.
    Terminal { status: u8 },
}

impl Event {
    /// The byte this kind is encoded as.
    #[must_use]
    pub const fn code(&self) -> u8 {
        match self {
            Self::TransformApplied { .. } => 0,
            Self::RootLp { .. } => 1,
            Self::NodeOpen { .. } => 2,
            Self::NodeSolved { .. } => 3,
            Self::NodePruned { .. } => 4,
            Self::NodeInfeasible { .. } => 5,
            Self::NodeIntegral { .. } => 6,
            Self::Branch { .. } => 7,
            Self::CutAdded { .. } => 8,
            Self::Incumbent { .. } => 9,
            Self::SearchGap { .. } => 10,
            Self::CertifyStart => 11,
            Self::CertifyNode { .. } => 12,
            Self::CertifyBlocked { .. } => 13,
            Self::CertifyComplete { .. } => 14,
            Self::Escalation { .. } => 15,
            Self::ProofAssembled { .. } => 16,
            Self::VerifyResult { .. } => 17,
            Self::Terminal { .. } => 18,
        }
    }

    /// The name the textual encoding writes.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::TransformApplied { .. } => "transform_applied",
            Self::RootLp { .. } => "root_lp",
            Self::NodeOpen { .. } => "node_open",
            Self::NodeSolved { .. } => "node_solved",
            Self::NodePruned { .. } => "node_pruned",
            Self::NodeInfeasible { .. } => "node_infeasible",
            Self::NodeIntegral { .. } => "node_integral",
            Self::Branch { .. } => "branch",
            Self::CutAdded { .. } => "cut_added",
            Self::Incumbent { .. } => "incumbent",
            Self::SearchGap { .. } => "search_gap",
            Self::CertifyStart => "certify_start",
            Self::CertifyNode { .. } => "certify_node",
            Self::CertifyBlocked { .. } => "certify_blocked",
            Self::CertifyComplete { .. } => "certify_complete",
            Self::Escalation { .. } => "escalation",
            Self::ProofAssembled { .. } => "proof_assembled",
            Self::VerifyResult { .. } => "verify_result",
            Self::Terminal { .. } => "terminal",
        }
    }
}

/// Why a trace could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceError {
    /// What was wrong.
    pub detail: String,
}

impl TraceError {
    fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }
}

impl core::fmt::Display for TraceError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl core::error::Error for TraceError {}

/// A whole trace.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Trace {
    /// The digest of the instance the run was about.
    ///
    /// The same one the certificate of that run carries. It is what makes a
    /// trace a trace *of something*: without it a trace is a list of nodes
    /// with no way to tell which problem they belong to, and anything
    /// rendering one would be free to draw it beside any instance at all.
    pub instance: [u8; 32],
    /// The events, in the order they happened.
    pub events: Vec<Event>,
}

impl Trace {
    /// An empty trace of an instance.
    #[must_use]
    pub fn about(instance: [u8; 32]) -> Self {
        Self {
            instance,
            events: Vec::new(),
        }
    }

    /// An empty trace of nothing, for a caller that will fill in the digest.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends one event.
    pub fn push(&mut self, event: Event) {
        self.events.push(event);
    }

    /// Encodes the trace in its binary form.
    ///
    /// # Errors
    ///
    /// Returns an error when a float field is not finite or a word field names
    /// a member its list does not have. Both are defects in the producer, and
    /// writing them would put a value in a trace that nothing can read back.
    pub fn encode(&self) -> Result<Vec<u8>, TraceError> {
        let mut out = Vec::with_capacity(64 + self.events.len() * 8);
        out.extend_from_slice(MAGIC);
        out.push(VERSION);
        out.extend_from_slice(&self.instance);
        for event in &self.events {
            out.push(event.code());
            write_fields(&mut out, event)?;
        }
        out.push(END);
        leb(&mut out, self.events.len() as u64);
        let checksum = crc32c(&out);
        out.extend_from_slice(&checksum.to_le_bytes());
        Ok(out)
    }

    /// Reads a trace from its binary form.
    ///
    /// # Errors
    ///
    /// Returns an error naming what stopped the read.
    pub fn decode(raw: &[u8]) -> Result<Self, TraceError> {
        let mut at = 0usize;
        if raw.len() < 5 || &raw[..4] != MAGIC {
            return Err(TraceError::new("the trace does not begin with its magic"));
        }
        at += 4;
        if raw[at] != VERSION {
            return Err(TraceError::new(format!(
                "the trace states version {}, and this reader accepts {VERSION}",
                raw[at]
            )));
        }
        at += 1;
        let instance: [u8; 32] = raw
            .get(at..at + 32)
            .ok_or_else(|| TraceError::new("the trace ends before its instance digest"))?
            .try_into()
            .map_err(|_| TraceError::new("a digest of the wrong width"))?;
        at += 32;

        let mut events = Vec::new();
        loop {
            let code = *raw
                .get(at)
                .ok_or_else(|| TraceError::new("the trace ends before its terminator"))?;
            at += 1;
            if code == END {
                break;
            }
            events.push(read_fields(code, raw, &mut at)?);
        }

        let stated = read_leb(raw, &mut at)?;
        if stated != events.len() as u64 {
            return Err(TraceError::new(format!(
                "the trace states {stated} events and carries {}",
                events.len()
            )));
        }
        let checksum = crc32c(&raw[..at]);
        let carried = raw
            .get(at..at + 4)
            .ok_or_else(|| TraceError::new("the trace ends before its checksum"))?;
        if carried != checksum.to_le_bytes() {
            return Err(TraceError::new(
                "the trace checksum does not match its bytes",
            ));
        }
        if at + 4 != raw.len() {
            return Err(TraceError::new("content follows the trace"));
        }
        Ok(Self { instance, events })
    }

    /// Renders the trace in its textual form.
    ///
    /// # Errors
    ///
    /// Returns an error for the same reasons the binary encoder does.
    pub fn render(&self) -> Result<String, TraceError> {
        let mut out = String::from("{\n");
        let _ = writeln!(out, "  \"version\": {VERSION},");
        let _ = writeln!(
            out,
            "  \"instance\": \"{}\",",
            crate::hash::hex(&self.instance)
        );
        out.push_str("  \"events\": [\n");
        for (index, event) in self.events.iter().enumerate() {
            let comma = if index + 1 == self.events.len() {
                ""
            } else {
                ","
            };
            let _ = writeln!(out, "    {}{comma}", text_fields(event)?);
        }
        out.push_str("  ]\n}\n");
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// Numbers
// ---------------------------------------------------------------------------

/// Appends an unsigned little-endian base-128 integer.
fn leb(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

/// Reads one, refusing an encoding wider than a machine word.
fn read_leb(raw: &[u8], at: &mut usize) -> Result<u64, TraceError> {
    let mut value = 0u64;
    let mut shift = 0u32;
    loop {
        let byte = *raw
            .get(*at)
            .ok_or_else(|| TraceError::new("the trace ends inside a number"))?;
        *at += 1;
        if shift >= 64 {
            return Err(TraceError::new("a number wider than a machine word"));
        }
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
        shift += 7;
    }
}

/// Appends a length and its bytes.
fn write_text(out: &mut Vec<u8>, text: &str) {
    leb(out, text.len() as u64);
    out.extend_from_slice(text.as_bytes());
}

/// Reads one.
fn read_text(raw: &[u8], at: &mut usize) -> Result<String, TraceError> {
    let length = usize::try_from(read_leb(raw, at)?)
        .map_err(|_| TraceError::new("a length wider than this machine"))?;
    let bytes = raw
        .get(*at..*at + length)
        .ok_or_else(|| TraceError::new("the trace ends inside a value"))?;
    *at += length;
    core::str::from_utf8(bytes)
        .map(ToOwned::to_owned)
        .map_err(|_| TraceError::new("a value that is not text"))
}

/// Appends a finite binary64.
fn write_float(out: &mut Vec<u8>, value: f64) -> Result<(), TraceError> {
    if !value.is_finite() {
        return Err(TraceError::new(
            "a trace float is not finite, which is a defect in whatever produced it",
        ));
    }
    out.extend_from_slice(&value.to_le_bytes());
    Ok(())
}

/// Reads one.
fn read_float(raw: &[u8], at: &mut usize) -> Result<f64, TraceError> {
    let bytes: [u8; 8] = raw
        .get(*at..*at + 8)
        .ok_or_else(|| TraceError::new("the trace ends inside a float"))?
        .try_into()
        .map_err(|_| TraceError::new("a float of the wrong width"))?;
    *at += 8;
    let value = f64::from_le_bytes(bytes);
    if !value.is_finite() {
        return Err(TraceError::new("a trace float is not finite"));
    }
    Ok(value)
}

/// The member a word field names.
fn word(list: &[&'static str], index: u8) -> Result<&'static str, TraceError> {
    list.get(index as usize).copied().ok_or_else(|| {
        TraceError::new(format!(
            "a word field names member {index}, which its list does not have"
        ))
    })
}

/// A float as the textual encoding writes it.
///
/// Always with a decimal point or an exponent, so that a consumer can never
/// mistake it for a count, and always the shortest text that reads back as the
/// same value.
fn float_text(value: f64) -> String {
    let mut out = format!("{value:?}");
    if !out.contains('.') && !out.contains('e') {
        out.push_str(".0");
    }
    out
}

/// A string as a JSON string.
fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

// ---------------------------------------------------------------------------
// The events, in encoding order
// ---------------------------------------------------------------------------

/// Appends one event's fields, in the order the schema lists them.
fn write_fields(out: &mut Vec<u8>, event: &Event) -> Result<(), TraceError> {
    match event {
        Event::TransformApplied { rule, position } => {
            leb(out, *rule);
            leb(out, *position);
        }
        Event::RootLp {
            iterations,
            objective,
        } => {
            leb(out, *iterations);
            write_float(out, *objective)?;
        }
        Event::NodeOpen {
            node,
            parent,
            depth,
        } => {
            leb(out, *node);
            leb(out, *parent);
            leb(out, *depth);
        }
        Event::NodeSolved {
            node,
            iterations,
            objective,
        } => {
            leb(out, *node);
            leb(out, *iterations);
            write_float(out, *objective)?;
        }
        Event::NodePruned { node, bound } => {
            leb(out, *node);
            write_text(out, bound);
        }
        Event::NodeInfeasible { node } => leb(out, *node),
        Event::NodeIntegral { node, objective } => {
            leb(out, *node);
            write_text(out, objective);
        }
        Event::Branch {
            node,
            variable,
            split,
        } => {
            leb(out, *node);
            leb(out, *variable);
            write_text(out, split);
        }
        Event::CutAdded {
            identifier,
            scope,
            family,
            violation,
        } => {
            leb(out, *identifier);
            leb(out, *scope);
            word(&FAMILIES, *family)?;
            out.push(*family);
            write_float(out, *violation)?;
        }
        Event::Incumbent { objective, source } => {
            write_text(out, objective);
            word(&SOURCES, *source)?;
            out.push(*source);
        }
        Event::SearchGap { lower, upper } => {
            write_float(out, *lower)?;
            write_float(out, *upper)?;
        }
        Event::CertifyStart => {}
        Event::CertifyNode { node, bound } => {
            leb(out, *node);
            write_text(out, bound);
        }
        Event::CertifyBlocked { node, reason } => {
            leb(out, *node);
            word(&BLOCKED, *reason)?;
            out.push(*reason);
        }
        Event::CertifyComplete { lower, upper, gap } => {
            write_text(out, lower);
            write_text(out, upper);
            write_text(out, gap);
        }
        Event::Escalation { stage } => {
            word(&STAGES, *stage)?;
            out.push(*stage);
        }
        Event::ProofAssembled { bytes, checksum } => {
            leb(out, *bytes);
            leb(out, *checksum);
        }
        Event::VerifyResult {
            accepted,
            rejection,
            work,
        } => {
            out.push(u8::from(*accepted));
            write_text(out, rejection.as_deref().unwrap_or_default());
            leb(out, *work);
        }
        Event::Terminal { status } => {
            word(&STATUSES, *status)?;
            out.push(*status);
        }
    }
    Ok(())
}

/// Reads one event's fields.
fn read_fields(code: u8, raw: &[u8], at: &mut usize) -> Result<Event, TraceError> {
    let byte = |at: &mut usize| -> Result<u8, TraceError> {
        let value = *raw
            .get(*at)
            .ok_or_else(|| TraceError::new("the trace ends inside an event"))?;
        *at += 1;
        Ok(value)
    };
    Ok(match code {
        0 => Event::TransformApplied {
            rule: read_leb(raw, at)?,
            position: read_leb(raw, at)?,
        },
        1 => Event::RootLp {
            iterations: read_leb(raw, at)?,
            objective: read_float(raw, at)?,
        },
        2 => Event::NodeOpen {
            node: read_leb(raw, at)?,
            parent: read_leb(raw, at)?,
            depth: read_leb(raw, at)?,
        },
        3 => Event::NodeSolved {
            node: read_leb(raw, at)?,
            iterations: read_leb(raw, at)?,
            objective: read_float(raw, at)?,
        },
        4 => Event::NodePruned {
            node: read_leb(raw, at)?,
            bound: read_text(raw, at)?,
        },
        5 => Event::NodeInfeasible {
            node: read_leb(raw, at)?,
        },
        6 => Event::NodeIntegral {
            node: read_leb(raw, at)?,
            objective: read_text(raw, at)?,
        },
        7 => Event::Branch {
            node: read_leb(raw, at)?,
            variable: read_leb(raw, at)?,
            split: read_text(raw, at)?,
        },
        8 => {
            let identifier = read_leb(raw, at)?;
            let scope = read_leb(raw, at)?;
            let family = byte(at)?;
            word(&FAMILIES, family)?;
            Event::CutAdded {
                identifier,
                scope,
                family,
                violation: read_float(raw, at)?,
            }
        }
        9 => {
            let objective = read_text(raw, at)?;
            let source = byte(at)?;
            word(&SOURCES, source)?;
            Event::Incumbent { objective, source }
        }
        10 => Event::SearchGap {
            lower: read_float(raw, at)?,
            upper: read_float(raw, at)?,
        },
        11 => Event::CertifyStart,
        12 => Event::CertifyNode {
            node: read_leb(raw, at)?,
            bound: read_text(raw, at)?,
        },
        13 => {
            let node = read_leb(raw, at)?;
            let reason = byte(at)?;
            word(&BLOCKED, reason)?;
            Event::CertifyBlocked { node, reason }
        }
        14 => Event::CertifyComplete {
            lower: read_text(raw, at)?,
            upper: read_text(raw, at)?,
            gap: read_text(raw, at)?,
        },
        15 => {
            let stage = byte(at)?;
            word(&STAGES, stage)?;
            Event::Escalation { stage }
        }
        16 => Event::ProofAssembled {
            bytes: read_leb(raw, at)?,
            checksum: read_leb(raw, at)?,
        },
        17 => {
            let accepted = match byte(at)? {
                0 => false,
                1 => true,
                other => {
                    return Err(TraceError::new(format!("a flag byte of {other}")));
                }
            };
            let rejection = read_text(raw, at)?;
            Event::VerifyResult {
                accepted,
                rejection: (!rejection.is_empty()).then_some(rejection),
                work: read_leb(raw, at)?,
            }
        }
        18 => {
            let status = byte(at)?;
            word(&STATUSES, status)?;
            Event::Terminal { status }
        }
        other => {
            // No forward compatibility here. A consumer that skipped an event
            // it did not recognise would be reporting a search that did not
            // happen.
            return Err(TraceError::new(format!(
                "event kind {other} is not one this schema has"
            )));
        }
    })
}

/// Renders one event as a JSON object.
fn text_fields(event: &Event) -> Result<String, TraceError> {
    let head = format!("{{\"kind\": {}", quoted(event.name()));
    let body = match event {
        Event::TransformApplied { rule, position } => {
            format!(", \"rule\": {rule}, \"position\": {position}")
        }
        Event::RootLp {
            iterations,
            objective,
        } => format!(
            ", \"iterations\": {iterations}, \"objective\": {}",
            float_text(*objective)
        ),
        Event::NodeOpen {
            node,
            parent,
            depth,
        } => format!(", \"node\": {node}, \"parent\": {parent}, \"depth\": {depth}"),
        Event::NodeSolved {
            node,
            iterations,
            objective,
        } => format!(
            ", \"node\": {node}, \"iterations\": {iterations}, \"objective\": {}",
            float_text(*objective)
        ),
        Event::NodePruned { node, bound } => {
            format!(", \"node\": {node}, \"bound\": {}", quoted(bound))
        }
        Event::NodeInfeasible { node } => format!(", \"node\": {node}"),
        Event::NodeIntegral { node, objective } => {
            format!(", \"node\": {node}, \"objective\": {}", quoted(objective))
        }
        Event::Branch {
            node,
            variable,
            split,
        } => format!(
            ", \"node\": {node}, \"variable\": {variable}, \"split\": {}",
            quoted(split)
        ),
        Event::CutAdded {
            identifier,
            scope,
            family,
            violation,
        } => format!(
            ", \"identifier\": {identifier}, \"scope\": {scope}, \"family\": {}, \"violation\": {}",
            quoted(word(&FAMILIES, *family)?),
            float_text(*violation)
        ),
        Event::Incumbent { objective, source } => format!(
            ", \"objective\": {}, \"source\": {}",
            quoted(objective),
            quoted(word(&SOURCES, *source)?)
        ),
        Event::SearchGap { lower, upper } => format!(
            ", \"lower\": {}, \"upper\": {}",
            float_text(*lower),
            float_text(*upper)
        ),
        Event::CertifyStart => String::new(),
        Event::CertifyNode { node, bound } => {
            format!(", \"node\": {node}, \"bound\": {}", quoted(bound))
        }
        Event::CertifyBlocked { node, reason } => format!(
            ", \"node\": {node}, \"reason\": {}",
            quoted(word(&BLOCKED, *reason)?)
        ),
        Event::CertifyComplete { lower, upper, gap } => format!(
            ", \"lower\": {}, \"upper\": {}, \"gap\": {}",
            quoted(lower),
            quoted(upper),
            quoted(gap)
        ),
        Event::Escalation { stage } => {
            format!(", \"stage\": {}", quoted(word(&STAGES, *stage)?))
        }
        Event::ProofAssembled { bytes, checksum } => {
            format!(", \"bytes\": {bytes}, \"checksum\": {checksum}")
        }
        Event::VerifyResult {
            accepted,
            rejection,
            work,
        } => format!(
            ", \"accepted\": {accepted}, \"rejection\": {}, \"work\": {work}",
            rejection
                .as_deref()
                .map_or_else(|| "null".to_owned(), quoted)
        ),
        Event::Terminal { status } => {
            format!(", \"status\": {}", quoted(word(&STATUSES, *status)?))
        }
    };
    Ok(format!("{head}{body}}}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Trace {
        Trace {
            instance: [7; 32],
            events: vec![
                Event::TransformApplied {
                    rule: 3,
                    position: 0,
                },
                Event::RootLp {
                    iterations: 12,
                    objective: -2.5,
                },
                Event::Branch {
                    node: 1,
                    variable: 2,
                    split: "3".to_owned(),
                },
                Event::CutAdded {
                    identifier: 7,
                    scope: 1,
                    family: 1,
                    violation: 0.125,
                },
                Event::SearchGap {
                    lower: -10.0,
                    upper: 3.0,
                },
                Event::CertifyBlocked { node: 4, reason: 1 },
                Event::Escalation { stage: 3 },
                Event::ProofAssembled {
                    bytes: 160,
                    checksum: 3_838_920_469,
                },
                Event::VerifyResult {
                    accepted: false,
                    rejection: Some("HCC-DUAL-03".to_owned()),
                    work: 900_000,
                },
                Event::Terminal { status: 5 },
            ],
        }
    }

    #[test]
    fn every_event_survives_a_round_trip_through_the_binary_form() {
        let raw = sample().encode().expect("writable");
        assert_eq!(Trace::decode(&raw).expect("readable"), sample());
        assert_eq!(
            Trace::decode(&raw)
                .expect("readable")
                .encode()
                .expect("writable"),
            raw
        );
    }

    #[test]
    fn the_textual_form_distinguishes_a_float_from_an_exact_value() {
        let text = sample().render().expect("renderable");
        assert!(text.contains("\"objective\": -2.5"), "{text}");
        assert!(text.contains("\"split\": \"3\""), "{text}");
        assert!(text.contains("\"violation\": 0.125"), "{text}");
        // A whole float still carries a decimal point, so it can never be read
        // back as a count.
        assert!(text.contains("\"upper\": 3.0"), "{text}");
        assert!(text.contains("\"rejection\": \"HCC-DUAL-03\""), "{text}");
        assert!(text.contains("\"stage\": \"bland\""), "{text}");
        assert!(text.contains("\"status\": \"no-progress\""), "{text}");
    }

    #[test]
    fn an_absent_rejection_is_null_rather_than_an_empty_string() {
        let trace = Trace {
            instance: [0; 32],
            events: vec![Event::VerifyResult {
                accepted: true,
                rejection: None,
                work: 1,
            }],
        };
        assert!(
            trace
                .render()
                .expect("renderable")
                .contains("\"rejection\": null")
        );
        let raw = trace.encode().expect("writable");
        assert_eq!(Trace::decode(&raw).expect("readable"), trace);
    }

    #[test]
    fn a_truncated_trace_is_refused_rather_than_read_as_a_shorter_run() {
        let raw = sample().encode().expect("writable");
        for cut in 0..raw.len() {
            assert!(
                Trace::decode(&raw[..cut]).is_err(),
                "a trace truncated to {cut} bytes was accepted"
            );
        }
        // And content after the terminator is not a second trace.
        let mut extended = raw.clone();
        extended.push(0);
        assert!(Trace::decode(&extended).is_err());
    }

    #[test]
    fn a_damaged_byte_is_caught_by_the_checksum_or_by_the_grammar() {
        let raw = sample().encode().expect("writable");
        for at in 0..raw.len() {
            let mut damaged = raw.clone();
            damaged[at] = damaged[at].wrapping_add(1);
            let read = Trace::decode(&damaged);
            match read {
                Err(_) => {}
                Ok(other) => panic!("byte {at} changed and the trace still read as {other:?}"),
            }
        }
    }

    #[test]
    fn a_kind_this_schema_does_not_have_is_refused_rather_than_skipped() {
        // No forward compatibility. A consumer that skipped an event it did
        // not recognise would be reporting a search that did not happen.
        let mut raw = Vec::from(MAGIC.as_slice());
        raw.push(VERSION);
        raw.extend_from_slice(&[0; 32]);
        raw.push(200);
        let refusal = Trace::decode(&raw).expect_err("an unknown kind");
        assert!(
            refusal.detail.contains("not one this schema has"),
            "{refusal}"
        );
    }

    #[test]
    fn a_word_outside_its_list_is_refused_on_both_sides() {
        let trace = Trace {
            instance: [0; 32],
            events: vec![Event::Terminal { status: 99 }],
        };
        assert!(trace.encode().is_err(), "an unlisted word was written");
        assert!(trace.render().is_err(), "an unlisted word was rendered");

        let mut raw = Vec::from(MAGIC.as_slice());
        raw.push(VERSION);
        raw.extend_from_slice(&[0; 32]);
        raw.extend_from_slice(&[18, 99]);
        assert!(Trace::decode(&raw).is_err(), "an unlisted word was read");
    }

    #[test]
    fn a_float_that_is_not_finite_is_a_defect_rather_than_a_value() {
        for value in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
            let trace = Trace {
                instance: [0; 32],
                events: vec![Event::SearchGap {
                    lower: value,
                    upper: 0.0,
                }],
            };
            assert!(trace.encode().is_err(), "{value} was written into a trace");
        }
    }

    #[test]
    fn the_count_in_the_terminator_is_compared_against_what_was_read() {
        let mut raw = sample().encode().expect("writable");
        // The count sits between the terminator byte and the checksum, so it
        // is four from the end for a one-byte count.
        let at = raw.len() - 5;
        assert_eq!(raw[at], 10, "the sample no longer has ten events");
        raw[at] = 8;
        assert!(Trace::decode(&raw).is_err());
    }

    #[test]
    fn every_kind_has_a_distinct_code_and_a_distinct_name() {
        let all = [
            Event::TransformApplied {
                rule: 0,
                position: 0,
            },
            Event::RootLp {
                iterations: 0,
                objective: 0.0,
            },
            Event::NodeOpen {
                node: 0,
                parent: 0,
                depth: 0,
            },
            Event::NodeSolved {
                node: 0,
                iterations: 0,
                objective: 0.0,
            },
            Event::NodePruned {
                node: 0,
                bound: String::new(),
            },
            Event::NodeInfeasible { node: 0 },
            Event::NodeIntegral {
                node: 0,
                objective: String::new(),
            },
            Event::Branch {
                node: 0,
                variable: 0,
                split: String::new(),
            },
            Event::CutAdded {
                identifier: 0,
                scope: 0,
                family: 0,
                violation: 0.0,
            },
            Event::Incumbent {
                objective: String::new(),
                source: 0,
            },
            Event::SearchGap {
                lower: 0.0,
                upper: 0.0,
            },
            Event::CertifyStart,
            Event::CertifyNode {
                node: 0,
                bound: String::new(),
            },
            Event::CertifyBlocked { node: 0, reason: 0 },
            Event::CertifyComplete {
                lower: String::new(),
                upper: String::new(),
                gap: String::new(),
            },
            Event::Escalation { stage: 0 },
            Event::ProofAssembled {
                bytes: 0,
                checksum: 0,
            },
            Event::VerifyResult {
                accepted: false,
                rejection: None,
                work: 0,
            },
            Event::Terminal { status: 0 },
        ];
        let mut codes: Vec<u8> = all.iter().map(Event::code).collect();
        let mut names: Vec<&str> = all.iter().map(Event::name).collect();
        codes.sort_unstable();
        codes.dedup();
        names.sort_unstable();
        names.dedup();
        assert_eq!(codes.len(), all.len(), "two kinds share a code");
        assert_eq!(names.len(), all.len(), "two kinds share a name");
        assert!(!codes.contains(&END), "a kind uses the terminator's byte");

        // And every one of them round-trips, which is what makes the list
        // closed rather than merely written down.
        let trace = Trace {
            instance: [3; 32],
            events: all.to_vec(),
        };
        let raw = trace.encode().expect("writable");
        assert_eq!(Trace::decode(&raw).expect("readable"), trace);
        assert!(trace.render().is_ok());
    }
}
