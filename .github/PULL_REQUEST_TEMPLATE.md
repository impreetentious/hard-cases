## Summary

<!-- What changed and why? -->

## Verification

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `./scripts/ci/forbidden-grep.sh`
- [ ] `cargo test --workspace --all-targets`
- [ ] `./scripts/preflight.sh`

## Correctness and compatibility

- [ ] Every touched failure mode has a named regression test.
- [ ] The verifier gained no workspace or external dependency, and shares no
      executable code with the solver.
- [ ] No proof rule was loosened; a rule change carries a hand-worked
      derivation and a new ADR.
- [ ] No floating-point quantity became a claim; bounds are exact inferences
      and incumbents are exactly screened.
- [ ] Tolerances used are named constants, not literals at the call site.
- [ ] Caps are checked before allocation on any attacker-influenced count.
- [ ] Determinism holds: the double-run comparison is unchanged or extended.
- [ ] Format changes bump a version and regenerate fixtures.
