//! The verifier's rejection identifiers.
//!
//! A second hand-written copy of the same public list. Not shared with the
//! solver's copy and not generated from the specification: the point of two
//! lists is that they can disagree, and a disagreement between them is a real
//! finding rather than a formality. A generated list agrees with the
//! specification by construction, which is not agreement.
//!
//! When this side says no, it says which rule said no. That is what turns a
//! disagreement between the two implementations into a report with an address,
//! and it is why the identifiers are an interface rather than an internal
//! detail.

use core::fmt;

/// One rejection identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Code(&'static str);

impl Code {
    /// The identifier as printed.
    #[must_use]
    pub const fn text(self) -> &'static str {
        self.0
    }

    /// The group the identifier belongs to.
    #[must_use]
    pub fn family(self) -> &'static str {
        match self.0.rfind('-') {
            Some(at) => &self.0[..at],
            None => self.0,
        }
    }
}

impl fmt::Display for Code {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

macro_rules! codes {
    ($( $(#[$note:meta])* $name:ident = $text:literal ),* $(,)?) => {
        impl Code {
            $( $(#[$note])* pub const $name: Self = Self($text); )*
        }

        /// Every identifier this crate names, in order.
        pub const KNOWN: &[Code] = &[ $(Code::$name),* ];
    };
}

/// The groups this crate names completely.
pub const FAMILIES: &[&str] = &[
    "HC-INSTANCE",
    "HCC-CLAIM",
    "HCC-CUT",
    "HCC-DUAL",
    "HCC-PARSE",
    "HCC-POINT",
    "HCC-RAY",
    "HCC-SCOPE",
    "HCC-TRANSFORM",
    "HCC-TREE",
];

codes! {
    /// The file is longer than the accepted size.
    FILE_TOO_LARGE = "HC-INSTANCE-01",
    /// More coefficients than the accepted count.
    TOO_MANY_NONZEROS = "HC-INSTANCE-02",
    /// A name is too long or holds a line feed.
    NAME_LENGTH = "HC-INSTANCE-03",
    /// A numeric token is longer than the accepted length.
    TOKEN_LENGTH = "HC-INSTANCE-04",
    /// A numeric token names a value wider than the cap.
    TOKEN_WIDTH = "HC-INSTANCE-05",
    /// An exact nonzero value does not survive the round trip into binary64.
    NOT_REPRESENTABLE = "HC-INSTANCE-06",
    /// A numeric token is malformed or spells a not-a-number or infinity.
    BAD_NUMBER = "HC-INSTANCE-07",
    /// A section header outside the accepted list.
    UNKNOWN_SECTION = "HC-INSTANCE-08",
    /// A section out of order or repeated.
    SECTION_ORDER = "HC-INSTANCE-09",
    /// A required section is missing.
    SECTION_MISSING = "HC-INSTANCE-10",
    /// No free row was declared to serve as the objective.
    NO_OBJECTIVE = "HC-INSTANCE-11",
    /// A record names a row that was never declared.
    UNKNOWN_ROW = "HC-INSTANCE-12",
    /// A record names a column that was never declared.
    UNKNOWN_COLUMN = "HC-INSTANCE-13",
    /// Two rows share a name.
    DUPLICATE_ROW = "HC-INSTANCE-14",
    /// Two columns share a name.
    DUPLICATE_COLUMN = "HC-INSTANCE-15",
    /// A section carries more than one named vector.
    MULTIPLE_VECTORS = "HC-INSTANCE-16",
    /// A bound type outside the accepted list.
    UNKNOWN_BOUND = "HC-INSTANCE-17",
    /// An integer bound record carries a value that is not an exact integer.
    FRACTIONAL_INTEGER_BOUND = "HC-INSTANCE-18",
    /// A negative upper bound on a column still at the default lower bound.
    AMBIGUOUS_NEGATIVE_UPPER = "HC-INSTANCE-19",
    /// Content runs past a fixed-form column window.
    FIELD_OVERRUN = "HC-INSTANCE-20",
    /// Integrality markers are unbalanced or nested.
    MARKER_BALANCE = "HC-INSTANCE-21",
    /// A column's records are not contiguous.
    COLUMN_SPLIT = "HC-INSTANCE-22",
    /// A range record names the objective row or a free row.
    RANGE_ON_FREE_ROW = "HC-INSTANCE-23",
    /// A second bound of the same type on one column.
    DUPLICATE_BOUND = "HC-INSTANCE-24",
    /// A free-form record carries more fields than its type accepts.
    TOO_MANY_FIELDS = "HC-INSTANCE-25",
    /// Input ended before the terminating section.
    UNEXPECTED_END = "HC-INSTANCE-26",
    /// The input does not match the accepted LP-format grammar.
    LP_GRAMMAR = "HC-INSTANCE-27",
    /// The input does not match the accepted JSON instance schema.
    JSON_SCHEMA = "HC-INSTANCE-28",
    /// The input is not ASCII text with line-feed line endings.
    NOT_TEXT = "HC-INSTANCE-29",

    /// A claim record and the header disagree about what is claimed.
    CLAIM_HEADER = "HCC-CLAIM-01",
    /// The records present do not close the status claimed.
    CLAIM_NOT_CLOSED = "HCC-CLAIM-02",
    /// A witness record the claim requires is absent.
    CLAIM_WITNESS_MISSING = "HCC-CLAIM-03",
    /// An optimal claim leaves a frontier node open.
    CLAIM_OPEN_FRONTIER = "HCC-CLAIM-04",
    /// An optimal claim has a leaf bound below the incumbent.
    CLAIM_LEAF_BELOW = "HCC-CLAIM-05",
    /// A gap claim has no open frontier node.
    CLAIM_NO_FRONTIER = "HCC-CLAIM-06",
    /// A gap claim's lower bound is not strictly below its incumbent.
    CLAIM_BOUND_NOT_BELOW = "HCC-CLAIM-07",
    /// The recomputed gap differs from the gap stated.
    CLAIM_GAP = "HCC-CLAIM-08",
    /// An infeasible claim has a leaf that is not a contradiction.
    CLAIM_LEAF_NOT_INFEASIBLE = "HCC-CLAIM-09",
    /// The claim kind is not permitted with the records present.
    CLAIM_KIND_FORBIDDEN = "HCC-CLAIM-10",

    /// A tableau combination does not isolate one integer basic variable.
    CUT_NOT_ISOLATED = "HCC-CUT-01",
    /// A nonbasic variable sits at no finite bound.
    CUT_FREE_NONBASIC = "HCC-CUT-02",
    /// An integer displacement is measured from a fractional endpoint.
    CUT_FRACTIONAL_ENDPOINT = "HCC-CUT-03",
    /// The fractional part is not strictly between zero and one.
    CUT_FRACTION_RANGE = "HCC-CUT-04",
    /// A recomputed Gomory coefficient differs from the one stated.
    CUT_GMI_COEFFICIENT = "HCC-CUT-05",
    /// The cover source row is not a positive-integer knapsack.
    CUT_SOURCE_NOT_KNAPSACK = "HCC-CUT-06",
    /// The cover weights do not strictly exceed the capacity.
    CUT_COVER_NOT_STRICT = "HCC-CUT-07",
    /// The cover source row holds a variable that is not binary.
    CUT_SOURCE_NOT_BINARY = "HCC-CUT-08",
    /// The lifting program has no feasible state.
    CUT_LIFT_INFEASIBLE = "HCC-CUT-09",
    /// A recomputed lifting coefficient is negative or fractional.
    CUT_LIFT_SIGN = "HCC-CUT-10",
    /// A recomputed lifting coefficient differs from the one stated.
    CUT_LIFT_COEFFICIENT = "HCC-CUT-11",
    /// The lifting program exceeds the caps that make it admissible.
    CUT_LIFT_CAPS = "HCC-CUT-12",

    /// An inference uses a negative multiplier.
    DUAL_NEGATIVE = "HCC-DUAL-01",
    /// An inference carries a zero multiplier, which is not canonical.
    DUAL_ZERO = "HCC-DUAL-02",
    /// The bound stated is not the bound the combination establishes.
    DUAL_BOUND_VALUE = "HCC-DUAL-03",
    /// The multiplied coefficients do not sum to the objective vector.
    DUAL_COEFFICIENTS = "HCC-DUAL-04",
    /// A contradiction's coefficients do not sum to zero.
    DUAL_NOT_ZERO_SUM = "HCC-DUAL-05",
    /// A contradiction's right-hand sides do not sum above zero.
    DUAL_NOT_POSITIVE = "HCC-DUAL-06",
    /// An inference cites an inequality identifier that does not exist.
    DUAL_NO_SUCH_INEQUALITY = "HCC-DUAL-07",

    /// The first four bytes are not the certificate magic.
    PARSE_MAGIC = "HCC-PARSE-01",
    /// The certificate or canonicalization version is not supported.
    PARSE_VERSION = "HCC-PARSE-02",
    /// The digest in the header is not this instance's digest.
    PARSE_NOT_THIS_INSTANCE = "HCC-PARSE-03",
    /// A declared size exceeds a cap.
    PARSE_DECLARED_TOO_LARGE = "HCC-PARSE-04",
    /// A variable-length integer is encoded in more bytes than it needs.
    PARSE_NON_MINIMAL = "HCC-PARSE-05",
    /// A variable-length integer does not fit in sixty-four bits.
    PARSE_LENGTH_OVERFLOW = "HCC-PARSE-06",
    /// The stream ends inside a record.
    PARSE_TRUNCATED = "HCC-PARSE-07",
    /// Bytes follow the terminating record.
    PARSE_TRAILING = "HCC-PARSE-08",
    /// The checksum does not match the bytes it covers.
    PARSE_CHECKSUM = "HCC-PARSE-09",
    /// A record type outside the closed list.
    PARSE_UNKNOWN_RECORD = "HCC-PARSE-10",
    /// A payload consumed a different number of bytes than it declared.
    PARSE_RECORD_LENGTH = "HCC-PARSE-11",
    /// Record identifiers are not strictly increasing.
    PARSE_IDENTIFIER_ORDER = "HCC-PARSE-12",
    /// Rational text is not in canonical form.
    PARSE_RATIONAL_FORM = "HCC-PARSE-13",
    /// A rational exceeds the bit cap.
    PARSE_RATIONAL_BITS = "HCC-PARSE-14",
    /// A rational token exceeds the token length cap.
    PARSE_RATIONAL_TOKEN = "HCC-PARSE-15",
    /// A set-valued array is unsorted or repeats an element.
    PARSE_SET_ORDER = "HCC-PARSE-16",
    /// The terminator's counts disagree with the stream.
    PARSE_END_COUNTS = "HCC-PARSE-17",
    /// The record count exceeds the cap.
    PARSE_RECORD_COUNT = "HCC-PARSE-19",
    /// The byte length exceeds the cap.
    PARSE_BYTE_COUNT = "HCC-PARSE-20",

    /// A point violates a row side.
    POINT_ROW = "HCC-POINT-01",
    /// A point violates a variable bound.
    POINT_BOUND = "HCC-POINT-02",
    /// A point is fractional where the model declares integrality.
    POINT_INTEGRALITY = "HCC-POINT-03",
    /// The recomputed objective differs from the objective claimed.
    POINT_OBJECTIVE = "HCC-POINT-04",
    /// The two witnesses do not correspond through the transform chain.
    POINT_CORRESPONDENCE = "HCC-POINT-05",
    /// A point's length is not the model's dimension.
    POINT_DIMENSION = "HCC-POINT-06",
    /// A point-only certificate carries a transform, root, or tree record.
    POINT_ONLY_EXTRA_RECORDS = "HCC-POINT-07",

    /// The direction is not a recession direction of a row.
    RAY_ROW = "HCC-RAY-01",
    /// The direction leaves a finite variable bound.
    RAY_BOUND = "HCC-RAY-02",
    /// The objective does not strictly improve along the direction.
    RAY_NOT_IMPROVING = "HCC-RAY-03",
    /// The base point of a ray is not feasible.
    RAY_BASE_INFEASIBLE = "HCC-RAY-04",
    /// The base point of a ray is fractional where integrality is declared.
    RAY_BASE_INTEGRALITY = "HCC-RAY-05",
    /// A mixed-integer ray has no positive integer step.
    RAY_NO_STEP = "HCC-RAY-06",
    /// The stepped direction is fractional on an integer variable.
    RAY_STEP_INTEGRALITY = "HCC-RAY-07",
    /// A ray certificate carries a transform, root, or tree record.
    RAY_EXTRA_RECORDS = "HCC-RAY-08",

    /// A cited inequality is not visible at the citing node.
    SCOPE_NOT_VISIBLE = "HCC-SCOPE-01",
    /// A record cites an identifier that does not precede it.
    SCOPE_FORWARD = "HCC-SCOPE-02",
    /// A cut's scope node is not the derivation node or an ancestor.
    SCOPE_BAD_SCOPE_NODE = "HCC-SCOPE-04",
    /// A tableau combination uses a row not visible at its node.
    SCOPE_TABLEAU_ROW = "HCC-SCOPE-05",

    /// A transform names a rule outside the closed list.
    TRANSFORM_UNKNOWN_RULE = "HCC-TRANSFORM-01",
    /// A transform is not applicable to the recomputed model.
    TRANSFORM_NOT_APPLICABLE = "HCC-TRANSFORM-02",
    /// The recomputed model does not match the hash the record states.
    TRANSFORM_HASH = "HCC-TRANSFORM-03",
    /// A transform contradiction does not follow from the recomputed model.
    TRANSFORM_CONTRADICTION = "HCC-TRANSFORM-06",
    /// A transform record appears after the root.
    TRANSFORM_AFTER_ROOT = "HCC-TRANSFORM-07",

    /// A branch variable is not declared integer.
    TREE_BRANCH_NOT_INTEGER = "HCC-TREE-01",
    /// A branch split value is not an exact integer.
    TREE_SPLIT_NOT_INTEGER = "HCC-TREE-02",
    /// A branch child identifier already exists.
    TREE_CHILD_EXISTS = "HCC-TREE-03",
    /// A node has more than one parent.
    TREE_MULTIPLE_PARENTS = "HCC-TREE-04",
    /// A node is not reachable from the root.
    TREE_UNREACHABLE = "HCC-TREE-05",
    /// A node is neither branched nor terminated.
    TREE_UNTERMINATED = "HCC-TREE-06",
    /// A node carries more than one terminal record.
    TREE_DOUBLE_TERMINAL = "HCC-TREE-07",
    /// A leaf cites an inference made at a different node.
    TREE_LEAF_NODE = "HCC-TREE-08",
    /// An integral leaf's point is not feasible or not integral.
    TREE_INTEGRAL_POINT = "HCC-TREE-09",
    /// An integral leaf's objective does not equal its cited bound.
    TREE_INTEGRAL_OBJECTIVE = "HCC-TREE-10",
    /// A frontier bound is at least the incumbent and must close its node.
    TREE_FRONTIER_CLOSED = "HCC-TREE-12",
    /// The replay stack exceeds the depth cap.
    TREE_DEPTH = "HCC-TREE-13",
    /// The root names a model the transform chain does not produce.
    TREE_ROOT_HASH = "HCC-TREE-14",
}

/// A refusal: the rule, what it was reading, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rejected {
    /// The rule that refused.
    pub code: Code,
    /// What was being read.
    pub detail: String,
    /// The one-based line, when there is one.
    pub line: Option<usize>,
}

impl Rejected {
    /// A refusal with no position.
    #[must_use]
    pub fn new(code: Code, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
            line: None,
        }
    }

    /// The same refusal, positioned.
    #[must_use]
    pub fn on_line(mut self, line: usize) -> Self {
        self.line = Some(line);
        self
    }
}

impl fmt::Display for Rejected {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(formatter, "{} at line {}: {}", self.code, line, self.detail),
            None => write!(formatter, "{}: {}", self.code, self.detail),
        }
    }
}

impl core::error::Error for Rejected {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_identifier_prints_as_itself_and_knows_its_family() {
        assert_eq!(Code::NO_OBJECTIVE.text(), "HC-INSTANCE-11");
        assert_eq!(Code::NO_OBJECTIVE.to_string(), "HC-INSTANCE-11");
        assert_eq!(Code::NO_OBJECTIVE.family(), "HC-INSTANCE");
        assert!(FAMILIES.contains(&Code::NO_OBJECTIVE.family()));
    }

    #[test]
    fn the_list_has_no_repeats_and_is_ordered() {
        let ordered: Vec<&str> = KNOWN.iter().map(|code| code.text()).collect();
        let mut sorted = ordered.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            ordered.len(),
            "the list repeats an identifier"
        );
        assert_eq!(ordered, sorted, "the list is not in identifier order");
    }

    #[test]
    fn a_refusal_carries_its_rule_and_position() {
        let refusal = Rejected::new(Code::UNKNOWN_SECTION, "QUADOBJ").on_line(12);
        assert_eq!(refusal.to_string(), "HC-INSTANCE-08 at line 12: QUADOBJ");
        let plain = Rejected::new(Code::UNEXPECTED_END, "no ENDATA");
        assert_eq!(plain.to_string(), "HC-INSTANCE-26: no ENDATA");
    }
}
