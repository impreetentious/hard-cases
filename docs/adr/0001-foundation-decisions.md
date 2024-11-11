# ADR-0001: Foundation decisions

## Context

Hard Cases exists to make one claim credible: that every terminal status it
reports is backed by a proof someone else can check. That claim is destroyed by
a single shared helper between the solver and the verifier, by one float
compared against a threshold and then called exact, or by a proof calculus
invented after the search code that has to satisfy it. The following decisions
close the highest-risk architectural choices before implementation begins.

## Decisions

1. **MIT license** — solvers are infrastructure; embedding is the point and
   nothing here needs copyleft.
2. **The proof calculus is frozen before the solver** — canonicalization,
   claim semantics, the certificate grammar, transform rules, cut formulas, and
   rejection identifiers are inert checked-in assets with hand-worked positive
   and negative vectors; documentation is generated from them and diffed, so
   neither implementation can become its own specification.
3. **Two arithmetics, one boundary** — the search runs in floating point, the
   solver assembles exact candidates from it, and the verifier independently
   proves or rejects them. A floating-point result is never a conclusion.
4. **The verifier has no dependencies at all** — its own big integers, its own
   hashing, its own parsers, its own rules, no workspace or external crate. It
   ships as a standalone binary so verification never requires linking the
   solver.
5. **The trust boundary is published, not implied** — the verifier's trusted
   base is exactly its source at the printed hash, a pinned compiler and
   standard library, and target integer semantics. "Independent" means
   code-path independent inside this project; it does not mean independently
   authored or formally verified, and the receipts say so.
6. **Exact values come from the original file's text** — a coefficient becomes
   a rational from its decimal characters, never from a float round-trip.
   Verifying against float-rounded data proves the wrong theorem.
7. **Certificates are bound to raw bytes** — the proof carries a hash over a
   format tag and the original file's bytes, so a proof cannot be quietly
   reattached to a different model.
8. **Caps are checked before allocation** — a short exponent token can name an
   enormous rational, and a legal proof can be very deep, so numeric,
   allocation, and depth limits are enforced ahead of the work and deep proofs
   are parsed and replayed iteratively with an explicit stack.
9. **The verifier recomputes; it never restates** — a coefficient asserted by
   the certificate is recomputed from the model, because trusting the
   certificate's own arithmetic would make the check circular.
10. **No naked tolerances** — feasibility, integrality, optimality, pivot, and
    drop thresholds are named constants in one module, and every use site reads
    as policy.
11. **Determinism is structural** — single-threaded search, ordered containers,
    fixed tie-breaking with a final lowest-index rule, and a from-scratch seeded
    generator initialized from the instance digest. The same bytes, seed, and
    version produce a byte-identical trace, log, and certificate.
12. **Effort is counted, never timed** — iterations, nodes, and cut rounds are
    the budget unit. Clocks would break both determinism and honesty.
13. **No fused multiply-add, no fast-math** — native and WebAssembly must agree
    bit for bit on the search path.
14. **From-scratch big integers on both sides** — two unrelated
    implementations, cross-checked against the same inert vectors, neither
    generated from the other.
15. **Factorization with updates and a drift guard** — the basis is refactorized
    on a measured drift bound rather than on a hunch, and update-versus-
    refactor equivalence is tested on pivot chains.
16. **A named numerical escalation ladder** — devex pricing and a Harris ratio
    test, escalating through refactorization and perturbation to Bland's rule;
    when the ladder is exhausted the status is honest, not optimistic.
17. **Two-phase simplex, no big-M** — a synthetic penalty constant would put a
    magic number inside a proof.
18. **Partial results are first-class** — a stopped solve reports either an
    exact bound with a certified gap or, when no bound can be certified, a
    plain feasible point the verifier checks against the original model. It
    never reports a floating-point gap as if it were certified.
19. **A closed list of presolve transforms, replayed by the verifier** — each
    transform has forward and reverse maps checked as exact equivalences.
    General dominated-column and coefficient-strengthening rules are excluded
    from v1 precisely because their witnesses are hand-wavy.
20. **A cut-free proof slice before anything clever** — a tiny branch-and-bound
    with a complete recursive certificate is built and verified before
    presolve, cuts, or pseudocosts exist, so a hole in the calculus surfaces
    early and cheaply.
21. **Every integral leaf carries a bound** — an integer-feasible node without a
    dual bound proves nothing about optimality, and the format cannot express
    one.
22. **Cuts are scoped and their scope is enforced twice** — a locally valid cut
    used outside its subtree invalidates every descendant proof, so ancestry
    visibility is checked in the solver and again by the verifier, with a
    dedicated tamper corpus.
23. **Cut derivations are exact and replayable** — Gomory mixed-integer cuts and
    capped exact sequential-lifting cover cuts record literal derivations that
    the verifier reproduces from the model.
24. **Unboundedness needs an integer-compatible ray** — an LP ray does not by
    itself prove a mixed-integer model unbounded, and labelling it so is
    forbidden.
25. **Hard certificate and work caps, stated absolutely** — proof size, rational
    bit length, and verifier work units have fixed limits, never a ratio
    against solve effort, so a hard instance cannot buy itself a bigger budget.
26. **The tamper corpus is a product surface** — corrupted certificates with
    expected rejection identifiers ship with the project, and the browser lets
    a visitor forge one and watch it fail.
27. **Independent parsers, differentially tested** — the solver's and verifier's
    readers must produce identical canonical models on a hand-built fixture
    matrix, and a disagreement is resolved from the frozen format rules rather
    than from either implementation.
28. **Scoreboards are permanent and self-referential** — published numbers
    compare this solver with itself under a disclosed environment; racing
    another solver on wall clock is out of scope forever.
29. **Pause and certify** — the browser can stop a running search and produce a
    verified bound for the current frontier, because a proof that only exists
    at termination is a weaker claim than one available at any moment.
30. **No facade** — every panel renders data from a real solve in that session
    or from a trace labelled as a replay, and float-derived and exact quantities
    are labelled differently wherever both appear.
31. **From-scratch dependencies** — solver crates use the standard library
    only; the big integers, sparse linear algebra, and factorization are the
    product. Serialization is allowed at the reporting edges, `proptest` in
    tests outside the verifier, and the WebAssembly bindings are sanctioned in
    advance rather than discovered later.
32. **Single-threaded search in v1** — parallel tree search changes the
    determinism and reproducibility story and is a separate decision, not a
    default.
33. **Owner-gated commits, tags, and releases** — implementation work leaves an
    auditable change queue; repository state and release timing remain the
    owner's decision, and build coordination artifacts stay outside the
    repository so it stands alone.

## Consequences

The solver carries a narrow dependency surface and pays real costs for its
claims: exact assembly is slower than trusting a float, a duplicated verifier
is more code than a shared one, and refusing to certify a bound sometimes means
publishing a weaker status than a competitor would. Those costs are the
product. Any reversal requires a new numbered record, and anything touching the
certificate format or a proof rule also requires a version bump and regenerated
fixtures.

## Alternatives considered

A third-party big-integer crate, a shared parser between solver and verifier,
trusting basis information stated in the certificate, floating-point gap
reporting, wall-clock effort budgets, and a general presolve rule set were each
rejected. Every one of them would either make the verifier's independence
nominal or make a published claim unfalsifiable.
