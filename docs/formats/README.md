<!--
  Generated from spec/v1 by scripts/gen-docs.mjs. Do not edit.
  The assets are the specification; this page is a rendering of them.
-->
# Generated format documentation

Every page here is rendered from the inert assets in `spec/v1` by
`scripts/gen-docs.mjs`. Editing a page has no effect: continuous integration
regenerates into a temporary directory and fails on any difference. Change the
asset instead, and remember that a change to a normative rule also needs a
decision record, a version bump on whichever contract moved, and regenerated
fixtures.

| Page | Asset | What it fixes |
|---|---|---|
| [Canonical exact model v1](canonical-model.md) | `spec/v1/canonical.toml` | How raw instance bytes become the exact model a proof is about, and how two parsers demonstrate they agree. |
| [MPS acceptance v1](mps.md) | `spec/v1/mps.toml` | Accepted sections, bound types, the RANGES table, and the fixed-form field windows. |
| [LP-format acceptance v1](lp-format.md) | `spec/v1/lp.toml` | The accepted LP-format subset: sections, statements, terms, bounds, and the reserved words. |
| [JSON instance schema v1](json-schema.md) | `spec/v1/json.toml` | The accepted JSON document: its members, its restricted JSON grammar, and the rule that no model value is a number. |
| [Numeric acceptance, caps, and budgets v1](numeric-caps.md) | `spec/v1/numeric.toml` | The acceptance fence every strict parser applies before allocating, and the absolute size, bit, and work caps a proof must fit inside. |
| [Deterministic generation v1](determinism.md) | `spec/v1/rng.toml` | The digest-to-seed derivation, the two recurrences, and frozen state and output vectors. |
| [Terminal claims v1](claims.md) | `spec/v1/claims.toml` | The closed terminal vocabulary, what each claim asserts, and the mechanical closure rule that decides it. |
| [HCC certificate format and proof calculus v1](certificate.md) | `spec/v1/hcc.toml` | The certificate encoding, the closed record kinds and their order, and the four inference rules. |
| [Presolve transforms v1](presolve-transforms.md) | `spec/v1/transforms.toml` | The eight closed transforms, their applicability conditions, and their forward and reverse maps. |
| [Cut derivations v1](cuts.md) | `spec/v1/cuts.toml` | The literal Gomory mixed-integer formula and the exact cover lifting dynamic program. |
| [Trace event schema v1](trace-events.md) | `spec/v1/trace.toml` | The closed list of trace events, their fields, and the two encodings a trace is written in. |
| [Rejection identifiers v1](rejections.md) | `spec/v1/rejections.toml` | Every rejection identifier, the group it belongs to, and the earliest check that emits it. |
