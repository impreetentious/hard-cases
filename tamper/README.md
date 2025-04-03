# The tamper corpus

Thirty-one named mutations of genuine certificates, each paired with the
rejection identifier that must answer it.

The pairing is the point. A corpus that only asserted "these must all be
rejected" would pass while every one of them was caught by the checksum, which
would establish nothing about the proof calculus — and would go on passing the
day a proof rule was deleted.

[`manifest.toml`](manifest.toml) is the corpus: what each mutation does, which
rule must catch it, and why that rule and not a later one. The mutations
themselves live in the `hc-tamper` crate, which decodes a certificate, changes
one thing, and encodes it again. It verifies nothing; that is the other side's
job and the whole point of the exercise.

Most entries are **re-sealed**: the checksum is correct for the bytes they
ship, so the mathematics has to be what refuses them. Anyone holding the format
specification can repair a checksum, so a forgery that failed on corruption
would establish nothing. The few entries that are not re-sealed exist to pin
the reader's own rules, and they say so.

An entry's identifier is the earliest rule that can see the defect **on the
models the harness runs it against**. Earliest is a property of the model as
well as of the mutation: moving an incumbent a thousand units is caught by the
row check on a model with a bound in the way and by the objective check on one
without. The pin is held to on those models, where it says something about the
order of the checks. What holds on every model is that a forgery is refused at
all, and that is what the browser surface checks against whatever certificate
is on the page.

The corpus is a product surface rather than a test fixture. It is what the
[situation room's](../theater/README.md) tamper lab offers a visitor, it is what
a reader who disbelieves the whole arrangement should reach for first, and it
grows whenever an incident finds a mutation nobody had thought of. Entries are
added and never quietly removed: a mutation that stops being catchable is a
finding.

```sh
cargo test -p hc-bench --test tamper
```
