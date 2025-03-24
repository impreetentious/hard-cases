<!--
  Generated from spec/v1 by scripts/gen-docs.mjs. Do not edit.
  The assets are the specification; this page is a rendering of them.
-->
# Cut derivations v1

The literal Gomory mixed-integer formula and the exact cover lifting dynamic program.

## `[families]`

- **closed** —
  - GmiCut
  - CoverCut
- **rule** — Two families in v1. A new family requires a hand-worked derivation, an ADR, a version bump, and its own exhaustive tiny-instance validity campaign before it may appear in a certificate.
- **validity_discipline** — Every accepted cut is exact-rechecked by solver-side arithmetic at assembly time, and independently replayed by the verifier before any descendant inference may cite it. An exhaustive campaign over tiny mixed-integer instances asserts that no derived cut removes an integer-feasible point.
- **traps** —
  - trap_gomory_formula_quadrants
  - trap_cover_not_strict
  - trap_cover_lift_dp
  - trap_cut_ledger_order

## `[scope]`

- **rule** — Every cut carries the node it was derived at. It may be derived from original and reduced rows, the branch assumptions on the path to that node, and earlier cuts visible there, and it may be used at that node and its descendants only.
- **reference** — hcc.toml scope

## `[gmi]`

- **source** — A rational combination of visible row equations, recorded by the certificate and reconstructed by the verifier over the tableau view defined in hcc.toml.
- **isolation** — The combination must isolate exactly one integer basic variable z with coefficient one, every other basic coefficient being exactly zero.
- **equation** — The reconstructed equation is z + sum over j of a_j w_j = beta.

## `[gmi.displacements]`

- **rule** — Each nonbasic variable enters as a nonnegative displacement from one of its finite bounds: w = x - l when the variable sits at a finite lower bound, or w = u - x when it sits at a finite upper bound.
- **integer_endpoints** — A displacement of an integer variable requires an integer endpoint, so that a unit step in w is a unit step in x.
- **free_nonbasic** — A nonbasic variable at no finite bound rejects. There is no displacement to write and the formula has no meaning without one.
- **bound_state** — The bound state of every nonbasic is recorded and checked against the model. A cut derived under one bound state and applied under another is a different inequality.

## `[gmi.fraction]`

- **beta** — f = beta - floor(beta), and the derivation requires 0 < f < 1. An integral beta yields no cut and rejects rather than producing a vacuous one.
- **coefficients** — f_j = a_j - floor(a_j) for each integer nonbasic j.

## `[gmi.formula]`

- **statement** — The cut, in displacement space, is the sum of the four terms below, greater than or equal to f.
- **term_integer_small** — For integer j with f_j <= f, the coefficient is f_j.
- **term_integer_large** — For integer j with f_j > f, the coefficient is f (1 - f_j) / (1 - f).
- **term_continuous_positive** — For continuous j with a_j >= 0, the coefficient is a_j.
- **term_continuous_negative** — For continuous j with a_j < 0, the coefficient is -f a_j / (1 - f).
- **rhs** — f
- **quadrants** — The four cases are the four quadrants of the classic derivation, and swapping two of them produces a cut that is valid on most instances and wrong on some. Each has its own vector.

## `[gmi.substitution]`

- **rule** — After computing the displacement-space cut, substitute the displacement definitions and the row activity variables back to obtain an inequality in the reduced variables x, normalize it to the canonical a dot x >= b form, and compare with the serialized cut.
- **comparison** — The comparison is on the recomputed inequality. The serialized coefficients are a checksum and a display hint, never an input to the check.

## `[cover]`

- **source** — A node-visible inequality of the form sum of a_j x_j <= b in which every variable is binary and every a_j and b is a positive integer.
- **primitive** — The weights and the capacity are required to be positive integers as they stand in the model. A cut derived after scaling a rational row into integers would be about a row the model does not contain.
- **selection** — The separation routine may choose the cover by any greedy or heuristic means. That choice is not trusted and is not checked; only the resulting cover condition is.
- **condition** — The recorded cover C must satisfy: the sum of a_j over j in C is strictly greater than b.
- **strictness** — The inequality is strict. A cover whose weight sum merely equals the capacity gives an invalid cut, and the check is written as strict for exactly that reason.
- **base_cut** — The sum of x_j over j in C is at most |C| - 1.

## `[cover.lifting]`

- **optional** — Lifting is optional. An unlifted cover cut is always available and always valid.
- **order** — The certificate records the deterministic order in which outside variables are lifted. Coefficients depend on that order, so it is part of the derivation rather than an implementation detail.
- **dp** — For each outside variable k in the recorded order, alpha_k = (|C| - 1) - max of the quantity: sum over j in C of x_j, plus sum over previously lifted l of alpha_l x_l, subject to the source inequality, with x_k fixed to one, over binary x.
- **computation** — The maximum is computed by an exact zero-one dynamic program over integer capacities, entirely in exact arithmetic. The verifier runs it rather than reading the stated coefficient.
- **infeasible_state** — If the dynamic program has no feasible state, variable k should have been fixed by the model rather than lifted, and the lifting rejects.
- **invalid_coefficient** — A negative alpha_k or a non-integral alpha_k rejects.
- **final_cut** — The lifted cut adds the sum of alpha_k x_k to the left-hand side of the base cut.

## `[cover.caps]`

- **vars** — 64
- **capacity** — 4096
- **states** — 1000000
- **outside** — When any cap would be exceeded, the certificate may carry only the unlifted base cut. Lifting some variables and stopping is not permitted: a partial application of the dynamic program computes coefficients against a different maximization than the one specified.
