// The browser surface, exercised in a real WebAssembly host.
//
// This is the only place the module is driven the way a page drives it: bytes
// in through the reserve, an answer pointer back, the length read from the
// four bytes in front of it, and the answer released. The Rust tests beside
// the exports run on the host, which catches the logic and cannot catch a
// pointer that is thirty-two bits wide only on one of the two targets.
//
// It is also the whole of the page's side of the interface, at about forty
// lines. That is what buys the absence of a binding generator.

import { readFileSync } from "node:fs";
import process from "node:process";

const [, , modulePath, instancePath] = process.argv;
if (!modulePath || !instancePath) {
  console.error("usage: wasm-smoke.mjs <module.wasm> <instance>");
  process.exit(2);
}

const { instance } = await WebAssembly.instantiate(readFileSync(modulePath), {});
const api = instance.exports;
const decoder = new TextDecoder();

/** Copies bytes into the module and returns the pointer and length. */
function give(data) {
  const pointer = api.hc_reserve(data.length);
  new Uint8Array(api.memory.buffer, pointer, data.length).set(data);
  return [pointer, data.length];
}

/** Reads an answer out of the module and releases it. */
function take(pointer) {
  const length = new DataView(api.memory.buffer).getUint32(pointer, true);
  const text = decoder.decode(new Uint8Array(api.memory.buffer, pointer + 4, length));
  api.hc_release_answer(pointer);
  return text;
}

const failures = [];
const check = (condition, said) => {
  if (!condition) failures.push(said);
};

const expected = [
  "memory",
  "hc_reserve",
  "hc_release",
  "hc_release_answer",
  "hc_solve_live",
  "hc_replay",
  "hc_recheck",
  "hc_version",
];
for (const name of expected) {
  check(name in api, `the module exports no ${name}`);
}
if (failures.length) {
  console.error("wasm-smoke: FAILED");
  for (const said of failures) console.error(` - ${said}`);
  process.exit(1);
}

const version = JSON.parse(take(api.hc_version()));
check(typeof version.version === "string", "the module reports no version");
check(version.trace_schema === 1, "the module reports another trace schema");

const model = readFileSync(instancePath);
const [pointer, length] = give(model);
const report = JSON.parse(take(api.hc_solve_live(pointer, length, 1, 100)));
api.hc_release(pointer, length);

check(report.provenance === "live", "a live solve did not say so");
check(report.verdict?.accepted === true, "the verifier did not accept its own run");
check(report.certificate?.bytes > 0, "no certificate came back");
check(report.trace?.instance === report.instance.digest, "the trace is about another instance");
check(
  report.trace.events.at(-1)?.kind === "terminal",
  "the trace does not end with a terminal event",
);
// The distinction every panel depends on, checked on the document the page
// actually receives: a search objective is a number and an exact one is a
// string, and no consumer can confuse them without meaning to.
const floats = report.trace.events.filter((event) => typeof event.objective === "number");
const exacts = report.trace.events.filter((event) => typeof event.objective === "string");
check(floats.length > 0, "no event carried a search objective as a number");
check(exacts.length > 0, "no event carried an exact objective as a string");
check(
  floats.every((event) => Number.isFinite(event.objective)),
  "a search objective was not finite",
);

// A file that is not an instance comes back as an answer, not as a trap.
const junk = new TextEncoder().encode("not an instance\n");
const [junkPointer, junkLength] = give(junk);
const refused = JSON.parse(take(api.hc_solve_live(junkPointer, junkLength, 1, 10)));
api.hc_release(junkPointer, junkLength);
check(refused.provenance === null, "a refusal claimed a provenance");
check(typeof refused.refused === "string", "a refusal carried no message");

// And the verifier alone, on the certificate the live solve produced, with one
// byte of it changed.
const proof = Uint8Array.from(
  report.certificate.hex.match(/../g).map((pair) => Number.parseInt(pair, 16)),
);
const [proofPointer, proofLength] = give(proof);
const [modelPointer, modelLength] = give(model);
const accepted = JSON.parse(
  take(api.hc_recheck(proofPointer, proofLength, modelPointer, modelLength)),
);
check(accepted.accepted === true, "the verifier refused an untouched certificate");
check(accepted.work > 0, "the verifier reported no work");

proof[Math.floor(proof.length / 2)] ^= 1;
new Uint8Array(api.memory.buffer, proofPointer, proofLength).set(proof);
const rejected = JSON.parse(
  take(api.hc_recheck(proofPointer, proofLength, modelPointer, modelLength)),
);
check(rejected.accepted === false, "the verifier accepted a damaged certificate");
check(
  typeof rejected.rejection === "string" && rejected.rejection.startsWith("HCC-"),
  "a refusal named no rule",
);
api.hc_release(proofPointer, proofLength);
api.hc_release(modelPointer, modelLength);

if (failures.length) {
  console.error("wasm-smoke: FAILED");
  for (const said of failures) console.error(` - ${said}`);
  process.exit(1);
}
console.log(
  `wasm-smoke: OK — ${expected.length} export(s), a live solve verified, a damaged certificate refused`,
);
