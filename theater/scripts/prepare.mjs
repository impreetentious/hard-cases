// Collects what the page serves: the WebAssembly module and the instances the
// picker offers.
//
// The module is a build artifact and is not committed, so this copies the one
// the Rust build just produced. A page bundled around a stale module would be
// a page reporting on a version of the solver nobody is running, and the check
// below is the difference between that and noticing.

import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import process from "node:process";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..", "..");
const publicDir = join(here, "..", "public");
const module = join(root, "target", "wasm32-unknown-unknown", "release", "hc_wasm.wasm");

try {
  statSync(module);
} catch {
  console.error("prepare: the WebAssembly module is missing.");
  console.error("prepare: build it with");
  console.error("  cargo build --release --target wasm32-unknown-unknown -p hc-wasm");
  process.exit(1);
}

mkdirSync(join(publicDir, "instances"), { recursive: true });
copyFileSync(module, join(publicDir, "hc.wasm"));

// The instances the picker offers, with the format each is read under. They
// are copied rather than listed, so the page is serving the same bytes the
// command line reads and the digests match.
const offered = [
  // The mixed-integer entries first: they are the ones with a tree to draw,
  // and a picker that opened on a model the search closes at the root would
  // introduce the situation room with an empty room.
  [
    "fixcharge6.mps",
    join(root, "instances", "integer", "fixcharge6.mps"),
    2,
    "a fixed-charge facility model: continuous columns capped by binary ones",
  ],
  [
    "knap9.mps",
    join(root, "instances", "integer", "knap9.mps"),
    2,
    "a binary knapsack: the one entry whose proof carries cuts",
  ],
  [
    "stock3.mps",
    join(root, "instances", "integer", "stock3.mps"),
    2,
    "general integers, so the splits are at thresholds other than zero and one",
  ],
  [
    "parity5.mps",
    join(root, "instances", "integer", "parity5.mps"),
    2,
    "an infeasibility the relaxation cannot see",
  ],
  ["cover6.mps", join(root, "instances", "local", "cover6.mps"), 1, "a set-covering relaxation"],
  ["transport33.mps", join(root, "instances", "local", "transport33.mps"), 1, "a transportation problem"],
  ["blend4.mps", join(root, "instances", "local", "blend4.mps"), 1, "a blending problem"],
  ["infeasible4.mps", join(root, "instances", "local", "infeasible4.mps"), 1, "no feasible point"],
  ["unbounded3.mps", join(root, "instances", "local", "unbounded3.mps"), 1, "objective without limit"],
  ["cycle7.mps", join(root, "instances", "local", "cycle7.mps"), 1, "a degenerate cycling example"],
  ["tiny.mps", join(root, "spec", "v1", "vectors", "mps", "tiny.mps"), 1, "the smallest complete instance"],
  ["ranges.lp", join(root, "spec", "v1", "vectors", "lp", "ranges.lp"), 3, "LP format, two-sided rows"],
  ["exact.json", join(root, "spec", "v1", "vectors", "json", "exact.json"), 4, "the JSON schema"],
];

const catalogue = [];
for (const [name, from, format, covers] of offered) {
  copyFileSync(from, join(publicDir, "instances", name));
  catalogue.push({ name, format, covers, bytes: statSync(from).size });
}
writeFileSync(join(publicDir, "instances", "catalogue.json"), `${JSON.stringify(catalogue, null, 2)}\n`);

// A recording, made natively, for the page's replay path to load.
//
// The situation room can solve in the browser, and it can also be handed a
// trace and a certificate that a native run produced and asked to draw them.
// The second path is the one that has to be able to say it is a recording, so
// there has to be a recording; a replay button with nothing behind it would be
// the first facade on the page.
//
// Two commands rather than one because the command line writes the two
// artifacts separately, and the pair is checked below: the trace records the
// length and the trailing checksum of the certificate its run assembled, so a
// mismatched pair is refused by the module rather than drawn.
const cli = join(root, "target", "release", "hc");
try {
  statSync(cli);
} catch {
  console.error("prepare: the command line is missing.");
  console.error("prepare: build it with");
  console.error("  cargo build --release -p hc-cli");
  process.exit(1);
}
const recordedDir = join(publicDir, "recorded");
mkdirSync(recordedDir, { recursive: true });
// The instance whose recording ships. Chosen for its tree rather than for
// its size: five nodes between them show every way this solver knows how to
// close one, and a replay of a model that closes at the root would be a
// replay of one event.
const recordedFrom = [
  "fixcharge6.mps",
  join(root, "instances", "integer", "fixcharge6.mps"),
  2,
];
execFileSync(cli, [
  "solve",
  recordedFrom[1],
  "--format",
  "mps-free",
  "--cert",
  join(recordedDir, "run.hcc"),
]);
execFileSync(cli, [
  "explain",
  recordedFrom[1],
  "--format",
  "mps-free",
  "--trace",
  join(recordedDir, "run.hct"),
]);
// The case files, as the page shows them. Parsed here rather than fetched and
// parsed in the browser: they are Markdown with a frontmatter block, and a
// Markdown parser in the page would be the largest dependency on it by an
// order of magnitude for six documents that never change between builds.
//
// Nothing is rewritten. The frontmatter becomes fields and the body is split
// at its own headings, so a case file that stops having the five sections the
// catalogue requires arrives here missing them and the page says so.
const casesDir = join(root, "docs", "case-files");
const cases = [];
for (const name of readdirSync(casesDir).sort()) {
  if (!name.endsWith(".md") || name === "README.md") continue;
  const text = readFileSync(join(casesDir, name), "utf8");
  const match = text.match(/^---\n([\s\S]*?)\n---\n([\s\S]*)$/);
  if (!match) {
    console.error(`prepare: ${name} has no frontmatter block`);
    process.exit(1);
  }
  const front = {};
  for (const line of match[1].split("\n")) {
    const at = line.indexOf(":");
    if (at > 0) front[line.slice(0, at).trim()] = line.slice(at + 1).trim();
  }
  const body = match[2];
  const title = body.match(/^#\s+(.+)$/m)?.[1] ?? name;
  const sections = {};
  const headings = [...body.matchAll(/^##\s+(.+)$/gm)];
  headings.forEach((heading, at) => {
    const from = heading.index + heading[0].length;
    const to = at + 1 < headings.length ? headings[at + 1].index : body.length;
    sections[heading[1].trim().toLowerCase()] = body.slice(from, to).trim();
  });
  cases.push({ ...front, title, sections });
}
if (cases.length === 0) {
  console.error("prepare: the case-file catalogue is empty");
  process.exit(1);
}
writeFileSync(join(publicDir, "case-files.json"), `${JSON.stringify(cases, null, 2)}\n`);

// The version the page shows comes from the workspace manifest rather than
// from the theater's own, so a page can never claim a version of the solver it
// was not built beside.
const manifest = readFileSync(join(root, "Cargo.toml"), "utf8");
const version = manifest.match(/\[workspace\.package\][\s\S]*?\nversion = "([^"]+)"/)?.[1];
if (!version) {
  console.error("prepare: the workspace manifest states no version");
  process.exit(1);
}
writeFileSync(join(publicDir, "built.json"), `${JSON.stringify({ version }, null, 2)}\n`);

writeFileSync(
  join(recordedDir, "run.json"),
  `${JSON.stringify(
    {
      instance: recordedFrom[0],
      format: recordedFrom[2],
      trace: "run.hct",
      certificate: "run.hcc",
      made: "by the command line in this repository",
      version,
    },
    null,
    2,
  )}\n`,
);

// And the recording is replayed through the module the page will load, here,
// before it ships. A recording that the browser surface refuses is a broken
// button nobody would notice until somebody pressed it.
const api = (await WebAssembly.instantiate(readFileSync(module), {})).instance.exports;
const decoder = new TextDecoder();
const hand = (data) => {
  const pointer = api.hc_reserve(data.length);
  new Uint8Array(api.memory.buffer, pointer, data.length).set(data);
  return [pointer, data.length];
};
const collect = (pointer) => {
  const length = new DataView(api.memory.buffer).getUint32(pointer, true);
  const text = decoder.decode(new Uint8Array(api.memory.buffer, pointer + 4, length));
  api.hc_release_answer(pointer);
  return text;
};
{
  const [tracePointer, traceLength] = hand(readFileSync(join(recordedDir, "run.hct")));
  const [proofPointer, proofLength] = hand(readFileSync(join(recordedDir, "run.hcc")));
  const [instancePointer, instanceLength] = hand(readFileSync(recordedFrom[1]));
  const replayed = JSON.parse(
    collect(
      api.hc_replay(
        tracePointer,
        traceLength,
        proofPointer,
        proofLength,
        instancePointer,
        instanceLength,
        recordedFrom[2],
      ),
    ),
  );
  api.hc_release(tracePointer, traceLength);
  api.hc_release(proofPointer, proofLength);
  api.hc_release(instancePointer, instanceLength);
  if (replayed.provenance !== "replay" || replayed.verdict?.accepted !== true) {
    console.error("prepare: the recorded run does not replay through the module");
    console.error(`prepare: ${replayed.refused ?? JSON.stringify(replayed.verdict)}`);
    process.exit(1);
  }
}

const size = statSync(module).size;
console.log(
  `prepare: ${size} byte module, ${catalogue.length} instance(s), ${cases.length} case file(s), one recording replayed, version ${version}`,
);
