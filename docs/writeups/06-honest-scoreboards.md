# Honest scoreboards

Every solver benchmark you have read is a comparison of two numbers produced on
one machine by somebody with a preference. That is not an accusation of bad
faith. It is a description of what the format permits: a table of times, a
geometric mean, a bar chart, and no way for a reader to tell the difference
between an engineering result and a tuning result.

This project publishes a board and takes three things off the table.

## No wall clock, ever

There are no wall-clock comparisons against other solvers anywhere in this
repository and there will not be any.

Not because timing is uninteresting — it is the most interesting thing about a
solver — but because a time is a statement about a machine, and a table of
times is a statement about the machine that generated it. A reader who cannot
reproduce the machine cannot check the table, and a number nobody can check is
the thing this whole project exists to refuse.

So effort is counted. Simplex iterations, nodes opened, cut rounds on the
solver's side; bytes parsed and limb-word steps on the verifier's. Two runs of
the board on two machines spend exactly the same amount, because there is no
ambient clock anywhere in the deterministic perimeter, no unordered container,
no fused multiply-add, and one thread. A budget in seconds would make the
program's own *behaviour* depend on the hardware.

Wall time and peak memory are disclosed as operational measurements. They gate
nothing and they are never divided into anything.

## A status is what the verifier accepted

The board has two columns where most have one. What the search concluded and
what the independent verifier accepted are separate facts, and they can differ:
a search can end `optimal` and have its certificate refused. That is a defect,
and a board that averaged the two into a single number would hide exactly the
defect it should be reporting.

The vocabulary is closed and it includes the honest failures. `optimal`.
`infeasible`. `unbounded`. Feasible with a certified gap — an exact lower
bound, an exact point, and the exact rational distance between them. Feasible
with *no* certified bound, which is a real status: here is a point that
verifiably satisfies your model and I cannot prove anything about how good it
is. And no progress, for a search that has nothing to say.

That fifth status is the one that makes the rest of it honest. A solver without
a word for "I could not prove anything" will always find something to print.

## What is missing is named

An instance that could not be fetched, could not be read, or could not be
solved stays on the board with the reason beside it. It is not dropped, and the
denominator does not quietly shrink.

This matters more than it sounds like it does, because the failure mode of a
harness is not producing wrong numbers — it is producing no numbers and looking
green. A test whose corpus directory is empty passes. A campaign whose
generator stopped generating passes. Every one of those leaves the output
looking exactly the same as success.

So the netlib harness in this repository walks its manifest and reports each
instance it cannot find *by name*, and the mixed-integer harness does the same,
and both check their own scaffolding unconditionally: the manifest parses, the
fetch script names exactly the instances the manifest does, the split names are
disjoint. Whatever is present is run and must not be a finding.

## What this board says today

`docs/scoreboard.md` is generated from the committed corpora by a binary in the
bench crate, and a test regenerates it and diffs, so a change to the solver that
moves an iteration count is a change that updates the board in the same commit.
A board edited by hand fails the suite.

It covers twelve files. Eight linear ones, each solved twice — once by the
revised simplex this project ships and once by a dense textbook tableau that
links none of the production linear algebra, a separation a source gate
enforces. Four mixed-integer ones: every one assembles a certificate the
standalone verifier accepts against the original bytes, three are small enough
that every assignment in their integer boxes is enumerated and the enumerated
answer compared by exact value, and all four are solved with separation and
without and must agree.

Twelve. That is a small number and the board does not dress it up.

## What this board leaves blank, and why that is the honest part

The netlib linear programs and a fixed MIPLIB 2017 subset are pinned:
manifests naming exactly which instances, fetch scripts that record a digest on
first fetch and verify it afterwards, harnesses that run whatever is present.

**No file from either has been fetched into this repository.** So there is no
row for either collection, and the board says that in a paragraph instead of
covering only what is committed and calling that the board.

It would have been easy to do otherwise. Fetch them once, run the harness,
paste the table in. It would look considerably more impressive than twelve
files, and it would be a statement about what one machine downloaded on one
day, published long after anybody could reproduce it. The generator prints that
board on demand, to a terminal, and writes nothing.

The manifests also deliberately do not carry published optimal values. A
cross-check against a published optimum is real evidence and it is only real if
the published value came from a source somebody can name; writing one from
memory would be the exact failure this project is built against. So the file
those values belong in is described, optional, and empty.

## The rule underneath all of it

A board is a claim about a program, and the standard for a claim here is the
same as everywhere else in this repository: somebody else has to be able to
check it.

That is why the numbers are counts rather than times, why the status column
belongs to the verifier rather than to the search, why the absent collections
are named rather than dropped, why the file is generated rather than written,
and why the one number that matters on it — findings: zero — is produced by
programs that do not share ideas with the one being measured.

A scoreboard that flattered this solver would be the easiest thing in the
repository to write, and it would undo everything else in it.
