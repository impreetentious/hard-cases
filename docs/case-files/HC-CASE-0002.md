---
id: HC-CASE-0002
crate: hc-io, hc-verify
found-by: the fuzz corpus, parser target
reproduce: cargo run -p hc-fuzz --bin hc-fuzz -- --from 6260705749512347485 1
fixed-in: v0.11.7
rejection: HC-INSTANCE-07
---

# A malformed number inside a malformed statement

## Symptom

The fuzzer mutates a generated LP file one byte at a time. It produced a
constraint whose continuation line began with a full stop:

```
 r3: - 3 x0 + 4 x1
     . 0.5 x2 + 0.5 x3 = 2
```

The solver's reader refused it as `HC-INSTANCE-07`, a malformed numeric token.
The verifier's refused it as `HC-INSTANCE-27`, a statement that does not match
the grammar. A second seed produced the mirror image: a `+` followed by a lone
full stop, which the solver called a grammar failure and the verifier called a
malformed number.

## Hunt

The full stop begins a numeric token in both readers — a number may be written
`.5` — and `.` alone is not a number. So both readers had a bad token and a bad
statement shape in one line, and each reported whichever it happened to reach.

The solver tokenizes a whole line and then interprets it, so it saw the token.
The verifier parses with a predictive cursor and stops reading the moment the
statement stops making sense, so on one seed it never reached the token and on
another it reached it first.

The question underneath is not about numbers at all. It is: *when is a run of
characters a token?* If that is a property of the run, it is settled before
anything decides what the statement means. If it is a property of the position
the run was read in, then a file's tokens depend on its grammar and the two
layers are not separable.

## Root cause

The two readers had answered that question differently without either of them
being asked it.

## Fix

One rule, written where each reader implements it: **a line is lexed, and then
it is interpreted.** The solver validates every numeric token as part of
splitting a line into tokens, before anything looks at the statement. The
verifier grew a lexing pass over each line — every character must begin a
token, and then every numeric token must be a number — that runs before the
statement it belongs to is parsed.

The same rule closed a second finding from the same run, in the other format:
the verifier's MPS reader was checking the fixed-form column windows only when
a section asked for a record's fields, so a record under `NAME`, which reads no
fields, and a record before any section header, which has no section to read
them, both went through. The windows are a property of the line. They are
checked for every record now, before anything decides what the line means.

## Lesson

Independence is not the same as freedom. The two implementations are meant to
be structurally unlike each other, and they are — but they are answerable to
one specification, and a question the specification never asked out loud is
exactly where they will drift apart.

The fuzzer is worth what it cost for this reason rather than for crashes. It
found no panic and no memory fault. It found two places where the grammar had
a seam nobody had noticed, and it found them in the first million cases.
