# ADR-0014: A reduced model is not the file it came from

## Context

Every stage of a presolve chain is a model, and every model has a canonical
dump whose hash names it. Both implementations must produce the same dump for
the same stage, byte for byte, or the chain cannot be replayed at all.

The dump carries two lines that are not about the model. A `zerosum` line
records a position where two coefficients in the *file* cancelled exactly, and a
`discarded` line records a free row the parser dropped after choosing an
objective. Both are notes about how the file was read.

They cannot survive a reduction unedited. A `zerosum` line names a column and a
row by index, and a transform that deletes a column renumbers everything after
it, so the note would come to point at a different position. Carrying it
forward, dropping it, or renumbering it are three different answers, and the
two implementations have to give the same one.

The digest line is a different case and the reasoning runs the other way.

## Decision

**A reduced model's dump carries no `zerosum` and no `discarded` lines.** Both
describe the reading of a file. A reduced model is not a file and was never
read; it was computed. The moment the first transform is applied, the notes stop
being about anything the dump describes, and they go.

**The digest line is carried through unchanged.** It names the bytes the whole
proof is ultimately checked against. A reduced model is still a reduction *of
that file*, and losing the binding partway down the chain would leave the
reduced model attached to nothing — which is the same hazard as a certificate
detached from its instance, one layer in.

**The format tag and the original sense are carried through for the same
reason.** They are facts about the model's provenance rather than about the
parse, and a reduced model has the same provenance as the original.

## Consequences

The rule is mechanical and needs no case analysis: whatever a transform does, the
resulting stage's dump has the notes absent and the digest present. There is no
situation in which one implementation would keep a note the other dropped.

A reader comparing an original dump with its first reduced stage will see the
notes disappear even when the first transform touched nothing near them. That is
correct and slightly surprising, which is why it is written down here rather than
left to be inferred from two implementations agreeing.

The cost is a small loss of information: the reduced dump cannot tell a reader
that the original file had a cancelling pair. Nothing needs it to. The original
model's dump still says so, the instance digest still binds the chain to the
file, and a reader wanting the file's peculiarities has the file.

## Alternatives considered

**Renumber the notes alongside everything else.** Rejected: it makes the notes
mean something new — a position in a model nobody wrote — and it adds a second
renumbering rule that both implementations would have to get identically right
for no benefit.

**Forbid a transform from changing indices.** Rejected. It would mean carrying
deleted rows and columns as tombstones, so the reduced model would be the same
size as the original and its dump would describe a model with holes in it. The
whole point of a reduction is that the reduced model is smaller.

**Drop the digest too, on the grounds that a reduced model is not the file.**
Rejected, and the asymmetry is the substance of this record. The notes describe
the parse and stop being true; the digest describes what the proof is about and
stays true all the way down.

## Supersedes / superseded by

Refines the canonicalization rules frozen in the specification, which did not
say what a dump of a model nobody parsed should contain. No decision is
superseded.
