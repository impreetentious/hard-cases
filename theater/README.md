# The situation room

A static page that solves a mixed-integer program in front of you, shows the
proof it produced, and lets you break that proof and watch an independent
verifier refuse it by name.

There is no server. `hc-wasm` compiles the solver, the certificate assembler
and the standalone verifier into one WebAssembly module; the page hands it
bytes and reads answers back out of its memory. The whole of the page's side of
that interface is [`src/hc.ts`](src/hc.ts), and it is the reason this project
has no binding generator.

## The two rules the page is built on

**Everything came from one of two places, and the header says which.** Either
this module solved the instance here and now, or a run recorded natively by the
command line is being replayed. There is no third source. A panel with nothing
behind it stays empty and says so.

**A number out of the search and a number a proof can cite are different
things, and they are never drawn the same way.** Three states, not two:

| | what it is | how it is drawn |
|---|---|---|
| float | a quantity the floating-point simplex reported | amber, dashed, written with a leading tilde |
| exact | a rational the solver worked out, which nothing has checked | dim cyan, hollow |
| verified | a quantity the standalone verifier recomputed from the certificate and the original file | bright cyan, solid |

The middle state is the one worth having. Collapsing it into "float"
understates what exact assembly did; collapsing it into "verified" is the claim
this whole project exists to refuse.

## The panels

- **Tree** — every node, how it was closed, and what it cost. Selecting a cut
  shades the subtree it may be used in and nothing else, which is an invariant
  a reader can check by eye.
- **Convergence** — the relaxation objectives the search reported, the exact
  node bounds it derived, and the bounds the verifier accepted, in the three
  styles above.
- **Frontier** — pause after a fixed number of nodes and certify anyway. Every
  open node takes an exact lower bound, and what comes back is a proved
  interval rather than a number to be trusted. Raise the budget and the
  interval closes.
- **Glass** — the root relaxation under the textbook pricing rule, pivot by
  pivot, with the columns named as the file names them. Every column of that
  table is floating point and none of it is evidence.
- **Certificate** — the proof as the *verifier's own reader* sees it, grouped
  by what each record is for, with the bytes of the selected record lit.
- **Tamper lab** — the corpus of named mutations, applied to the certificate on
  the page, plus a field editor that changes a byte and repairs the checksum in
  the page so that the mathematics has to be what refuses it.
- **Case files** — real defects this repository has had.

The address bar carries the instance, the node budget, the panel, the event the
scrubber is on and the record the viewer has selected, so a link shares the
view rather than the page.

## Your own model

The picker offers this repository's committed instances, and beside it is a
file control that takes one of yours.

**The format is named, not guessed.** A select appears the moment a file does,
and it stays visible. That is this project's rule rather than a preference
about controls: the format tag is part of the instance digest, so the same
bytes read under two formats are two different instances, and a heuristic there
would make the identity of a model depend on a guess. The extension picks a
starting position for the control and nothing else.

**The file does not leave the tab.** There is no server behind this page and no
telemetry in it. The bytes are read by the browser, handed to a WebAssembly
module in the same tab, and are gone when the tab closes. Nothing is uploaded,
because there is nowhere to upload it to.

The consequence is that a link cannot carry it. The share button says so and is
disabled while a file of your own is open, rather than producing an address
that would open on something else for whoever receives it.

## Building it

```sh
cargo build --release --target wasm32-unknown-unknown -p hc-wasm
cargo build --release -p hc-cli
npm --prefix theater ci
npm --prefix theater run dev
```

`scripts/prepare.mjs` collects what the page serves: the module the Rust build
just produced, the instances the picker offers copied byte for byte from the
repository so their digests match, the case files parsed out of their Markdown,
and one run recorded by the command line for the replay path. It replays that
recording through the module before finishing, so a broken replay button is a
failed build rather than something a visitor finds.

`../scripts/ci/theater.sh` is the gate: assets, types, build, byte budgets, and
the browser suite in [`tests/`](tests/) against the built bundle. It runs as the
last step of `../scripts/preflight.sh`.
