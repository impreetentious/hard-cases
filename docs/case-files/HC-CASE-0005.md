---
id: HC-CASE-0005
crate: hc-presolve, hc-verify
found-by: the fuzz corpus, solver target
reproduce: cargo run -p hc-fuzz --bin hc-fuzz -- --from 1910 1
fixed-in: v0.11.12
rejection: HCC-POINT-02
---

# A column fixed at a value outside its own bounds

## Symptom

The solver target reported the worst thing it can report: the verifier refused
a certificate the solver had just produced.

```
HCC-POINT-02: column x3 holds 3 above its upper bound 1
```

The instance was a mutation that turned `0 <= x3 <= 1` into `03<= x3 <= 1`, so
the column's lower bound is three and its upper bound is one. That is a
perfectly well-formed file describing a model with no feasible point.

## Hunt

The column appears in no row and has a zero objective coefficient, so the
reduction chain applies its empty-column rule: a column nothing constrains is
dropped, resting at whichever endpoint the objective prefers. The rule asks for
zero if zero is inside the bounds, and otherwise takes whichever endpoint
exists.

Neither branch asks whether the bounds cross. With a lower bound of three and
an upper bound of one, zero is not inside, so the rule took the lower bound and
fixed the column at three — above the upper bound it had just read.

The reduced model was then solved, the point was lifted back through the chain,
and the incumbent that came out held a value outside its own bounds. The
verifier caught it at the point check, which is exactly where it should be
caught and exactly the wrong place for it to have got to.

## Root cause

An applicability condition that was not checked because it looked impossible. A
column with crossed bounds is an infeasible model, and the reduction treated it
as an ordinary column with an awkward objective.

## Fix

The rule declines. A column whose bounds cross has no value to rest at, so
there is no reduction to make: the column stays, the search solves the model it
is actually holding, and the relaxation reports infeasible with a proof of it.

The fix went into both implementations. The verifier has its own copy of the
transform rules, and a rule it accepts that the solver refuses is a rule a
forged certificate could still use — so the same condition is checked there,
written separately, for the same reason.

## Lesson

The verifier did its job, and that is not comfort. It caught the defect three
stages downstream of where it happened, in a check about points, on a model
that had been reduced, solved, and lifted since. Had the incumbent happened to
be lifted to a value that was *inside* its bounds by coincidence, the same
reduction would have gone through unnoticed.

The independent checker is a net, not a design. A rule that produces a value
has to be the thing that knows a value exists.
