# ADR-0016: The remaining two input formats are frozen before their parsers

## Context

Two of the four format tags have been reserved since the first freeze and have
had nothing behind them. Tag 3 is the LP-format subset and tag 4 is the JSON
instance schema; both appear in the canonical model, in the verifier's tag
enumeration, and in the rejection table, which already carries an identifier for
an LP grammar failure and another for a JSON schema failure. Neither format had
a parser on either side.

The appendices that motivate them are thin by design. The LP format is given as
"the common subset" with an operator list and a name pattern, and the JSON
schema is given as a five-member sketch. That is enough to start writing a
parser and nowhere near enough for two parsers written separately to agree, and
agreeing is the entire reason there are two.

The LP format is also the most ambiguous thing this project accepts. It has no
statement terminator, so a reader has to decide where a wrapped constraint ends;
it defaults an omitted lower bound; it writes a term four ways; and every one of
those is a place where two readers build different models and neither reports
anything. A format whose disagreements are silent cannot be specified by writing
one parser and describing it.

## Decision

**Freeze both formats as normative assets first, `spec/v1/lp.toml` and
`spec/v1/json.toml`, and write the two parsers on each side against the assets.**

The assets follow the MPS asset in kind: every fork the format is known to have
is decided in writing with the losing reading named beside it. Three of those
decisions are worth naming here.

1. **Every objective and constraint carries a label, and a line that does not
   begin with one continues the statement before it.** This is what makes the
   continuation rule total. The usual heuristics — start a new statement at a
   leading sign, or at a line containing a relational operator — split a wrapped
   row differently from each other, silently.

2. **An infinity spelling is a reserved word in a bound position and a malformed
   number everywhere else.** It is how the subset expresses an absent side, and
   keeping it out of the numeric grammar means there is exactly one numeric
   grammar across all three formats rather than one with a local exception.

3. **No model value in a JSON document is a JSON number.** Coefficients, bounds,
   right-hand sides, ranges, and the objective constant are decimal strings. A
   JSON number is a binary64 in nearly every reader, and a value that has passed
   through a binary64 is not the value the file wrote. The accepted JSON grammar
   therefore has no floating-point literal in it at all, which makes the rule
   structural rather than a convention a reader has to remember.

The JSON schema gains one member the appendix sketch did not have,
`objective_constant`. A serialization that cannot carry the objective constant
cannot round-trip the canonical model, and a format that cannot round-trip the
model it serializes is not a serialization of it.

No new rejection identifiers are introduced. The grammar and schema identifiers
were reserved at the freeze, and every structural failure the two formats share
with MPS — an unknown section, a repeated one, a duplicate label, an undeclared
name, a repeated bound, a truncated file, an overlong name, the numeric fences —
keeps the identifier it already had. That is deliberate: the differential
compares two parsers by the identifier they refuse with, and identifiers that
varied by format would make a cross-format disagreement unreadable.

## Consequences

**The parsers are written twice against something neither of them wrote.** The
solver reads these assets nowhere at run time and neither does the verifier;
they are read by the people writing the code and by the documentation generator.
Both implementations are answerable to the same frozen text, which is the only
mechanism available that keeps two same-author implementations from agreeing
with each other instead of with the truth.

**The accepted LP subset is narrower than the files in the wild.** A file
without constraint labels is refused, and so is one that writes a constant
inside a constraint expecting the reader to move it. Both are ordinary in
practice. The cost is instance coverage and it is the same trade the MPS reader
already makes in three places.

**A model round-trips across all three formats.** The double-sided constraint
form and the JSON range member both map onto the canonical two-sided row by the
same table the MPS RANGES section uses, so a model written in one format and
rewritten in another is the same model below the digest line. That property is
what the round-trip fixtures check, and it would have been impossible had the LP
subset omitted ranged rows.

## Alternatives considered

**Write one parser and let the asset describe it.** This is what the format
documentation would have been if it were maintained by hand, and it fails the
same way: the description drifts toward whatever the implementation found easy,
and the second implementation is then written against the first one wearing a
disguise.

**Accept LP statement continuation by heuristic and refuse only on conflict.**
There is no conflict to detect. Both readings produce a well-formed model; they
just produce different ones. A check that fires only when a heuristic is
detectably wrong does not fire on the case that matters.

**Allow JSON numbers for model values and convert them exactly from their
source text.** The text is available in principle, but only if the reader is
written to keep it, and every off-the-shelf reader on the other side of a wire
will have discarded it before the document reaches this one. A schema that is
exact only when read by a particular implementation is not exact.

## Supersedes / superseded by

Extends ADR-0002, which reserved the two format tags and their rejection
identifiers at the format freeze without deciding what they accept. Nothing is
superseded.
