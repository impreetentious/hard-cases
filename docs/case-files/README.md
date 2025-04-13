# Case files

Real defects, minimized, with the input that exposed them and the change that
closed them. Nothing here is invented: every entry names something that was
actually wrong in this repository, found by a harness rather than by reading,
and every one carries a reproduction that still runs.

A thin catalogue with a writeup beats a padded one. There is no target number
of case files and no incentive to reach one; an entry exists because a defect
did.

## The frontmatter

Every file opens with the same fields, and a test reads them:

- `id` — `HC-CASE-nnnn`, assigned in order and never reused.
- `crate` — where the defect lived.
- `found-by` — the harness that produced it, named precisely enough to run again.
- `reproduce` — the exact command, or the seed a generated case came from.
- `fixed-in` — the version the change landed in.
- `rejection` — the identifier the case turns on, where it turns on one.

Then the same five headings: **Symptom**, **Hunt**, **Root cause**, **Fix**,
**Lesson**. The last one is the point of the exercise. A defect that taught
nothing is a defect that will be back.
