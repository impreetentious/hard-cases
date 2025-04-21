# `spec/v1` — the normative assets

These files are the specification. Not a description of one, and not
documentation generated from working code: the rules that decide what a Hard
Cases certificate means are written down here first, and both implementations
are held against them.

That ordering is the whole point. A solver and a verifier written by the same
person, in the same week, from the same mental model, will agree with each other
long before either agrees with the truth. Freezing the calculus as inert data —
before the code that has to satisfy it — is the only mechanism available that
makes the two implementations answerable to something other than each other.

## What "inert" means here

Nothing in this directory is executable, generated, or derived. Specifically:

- **No implementation generates these files.** Not the solver, not the verifier,
  not a test. If a rule changes, a human edits the asset and both sides move to
  meet it.
- **No implementation is generated from these files.** There is no code
  generator pointed at `spec/v1`. Two independent hand-written implementations
  read the same frozen numbers and must produce the same answers; a shared
  generator would collapse that into one implementation wearing two hats.
- **The documentation is generated from these files, never the reverse.**
  `scripts/gen-docs.mjs` renders `docs/formats/*.md` from these assets. CI
  regenerates into a temporary directory and diffs, so prose cannot drift away
  from the rules it claims to describe.
- **The vectors are hand-worked or independently computed**, and are checked in
  as literal expected values. A vector produced by running the code it is meant
  to test is not a vector; it is a screenshot.

## The freeze

Every asset here is version `1`, matching the directory. A change to any
normative rule requires, together and in the same change:

1. an ADR recording what changed and why;
2. a version bump of whichever contract moved — canonicalization, HCC, or the
   trace schema;
3. regenerated fixtures under `spec/v1/vectors/` and `tamper/`.

The rejection identifiers in `rejections.toml` are a public interface. `hc
verify` prints them, the tamper lab displays them, and the case files cite them.
They are append-only: an identifier is never reused for a different check, and a
retired one is marked retired rather than deleted.

## The asset format

Every `.toml` file here is valid TOML restricted to a subset small enough that a
dependency-free reader on either side of the boundary can parse it in a few
dozen lines. `hc-verify` cannot take a TOML dependency, and a specification the
verifier cannot read is a specification the verifier does not have.

The subset is:

- UTF-8, LF line endings, no BOM.
- Comments occupy a whole line and begin with `#` after optional whitespace.
  There are no trailing comments — a `#` inside a value is part of the value.
- Table headers are `[name]` or `[name.sub]`, one per line.
- Assignments are `key = value` where value is exactly one of:
  - a **literal string** in single quotes, containing no single quote and no
    escape sequences of any kind — the formulas here are full of backslashes and
    a basic string's escape rules would silently eat them;
  - a **decimal integer**, optionally negative, with no separators;
  - a **boolean**, `true` or `false`;
  - an **array** of literal strings on one line, `['a', 'b']`.
- No inline tables, no arrays of tables, no multi-line values, no dotted keys
  inside a table body.

Anything outside the subset is a malformed asset, and both readers reject rather
than guess.

## The files

| File | What it fixes |
|---|---|
| `canonical.toml` | Format tags, the instance digest, the canonical exact model, id assignment, and the byte-comparable canonical dump both parsers must emit |
| `mps.toml` | Accepted MPS sections, bound types, and the RANGES sense-by-sign table |
| `numeric.toml` | Token, allocation, projection, certificate, and work-unit caps, and the numeric acceptance fence |
| `rng.toml` | The digest-to-seed derivation, the SplitMix64 and xoshiro256++ recurrences, and frozen state and output vectors |
| `claims.toml` | The closed terminal vocabulary and the closure rule for each claim |
| `hcc.toml` | The certificate encoding, the closed record kinds, record order, and the inference rules |
| `transforms.toml` | The eight presolve transforms, their applicability, and their forward and reverse maps |
| `cuts.toml` | The literal GMI formula and the cover lifting dynamic program |
| `rejections.toml` | Every rejection identifier, its group, and the earliest check that emits it |

## The vectors

`vectors/` holds the hand-worked cases. They are organized by what they
constrain, not by which implementation reads them — both read all of them.

| Directory | Contents |
|---|---|
| `vectors/arith/` | Big-integer and rational operations with literal operands and results |
| `vectors/hash/` | SHA-256 and CRC32C over named byte strings |
| `vectors/rng/` | Seeds, resulting generator states, and output prefixes |
| `vectors/mps/` | Instance files paired with their expected canonical dumps or rejection identifiers |
| `vectors/proof/` | Hand-worked inferences, transforms, and cut derivations with their verdicts |

A vector file states what it constrains in its own header. The negative vectors
matter more than the positive ones: they are the only evidence that a check
fires, and every one of them names the exact identifier it must fire with.
